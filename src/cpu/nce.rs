//! Native execution engine (`--cpu=native`, PLAN.md step 4): guest AArch64 code runs directly on
//! an AArch64 host CPU. `ounce-nce` holds the `unsafe` parts (the host window, entering guest code,
//! fault signals) and the rewriter ported from nxnative; this side keeps it in step with the guest:
//!
//! - Guest memory is a [`Memory`] backed by an `ounce_nce::Arena` at [`WINDOW_BASE`], and the
//!   kernel's code, heap and alias regions move into that window (`HorizonKernel::set_addr_base`), so
//!   guest pointers are host pointers.
//! - Before each entry, executable pages mapped since the last one are rewritten (`svc`, thread
//!   registers, timers, `x18`) and sealed read+execute. The stubs live in a patch area just below the
//!   usual code base, plus satellite areas next to code beyond branch range of it.
//! - A trampoline exit hands the guest registers back as a [`Cpu`]: SVCs go to the same kernel
//!   dispatcher the other engines use, timer reads are answered here.
//! - Host pages can be larger than Horizon's (16 KiB on Apple Silicon). Code that shares a host page
//!   with writable guest data cannot be sealed read+execute, so it stays unrewritten and runs on the
//!   interpreter: native code that branches there faults on instruction fetch, the interpreter takes
//!   over until control leaves those pages, then native execution resumes.

use super::interpreter::Cpu;
use crate::mmu::{Memory, Permission};
use ounce_nce::patch::{Patcher, SiteKind, Stats, CONTEXT_AREA, SYSREG_CNTFRQ_EL0};
use ounce_nce::{Exit, NceError};
use std::collections::HashSet;

/// Signal numbers of an instruction fetch from a non-executable page (they differ per host).
fn libc_sigsegv() -> i32 { 11 }
fn libc_sigbus() -> i32 { if cfg!(target_os = "macos") { 10 } else { 7 } }

/// The host window guest memory lives in: the top 64 GiB of Horizon's 39-bit address space. Guest
/// addresses must stay below 2^39 (HIPC buffer descriptors carry 39 bits), and macOS on Apple Silicon
/// keeps everything else below it for itself (its executable and dyld cache under 16 GiB, a reserved
/// range from 64 to 448 GiB).
pub const WINDOW_BASE: u64 = 0x70_0000_0000;
pub const WINDOW_SIZE: u64 = 0x10_0000_0000;
/// AliasRegionSize inside the window (the usual 64 GiB does not fit next to the stack region).
pub const ALIAS_SIZE: u64 = 0x8_0000_0000;

/// Reserves the native window.
pub fn reserve_window() -> Result<ounce_nce::Arena, String> {
    ounce_nce::Arena::reserve_at(WINDOW_BASE, WINDOW_SIZE).map_err(err)
}
/// Patch area offset in the layout: 64 MiB below the usual code base (`0x8000_0000`), so code up to
/// 64 MiB long is within `B` range of its stubs.
const PATCH_AREA: u64 = 0x7C00_0000;
const STUB_CAPACITY: u64 = 0x10_0000;
const SATELLITE_CAPACITY: u64 = 0x1_0000;
/// Alignment and clearance of patch areas, so they never share a host page with guest data.
const HOST_PAGE_MAX: u64 = 0x1_0000;
/// Time slice before a guest thread that makes no SVC is preempted.
const SLICE: std::time::Duration = std::time::Duration::from_millis(1);
/// Horizon's timer frequency (`CNTFRQ_EL0`).
const TIMER_HZ: u64 = 19_200_000;

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn interpreted_backward_branch_returns_to_scheduler() {
        let mut memory = Memory::new();
        memory.map_range(0x8000, 4096, Permission::READ_WRITE_EXECUTE);
        memory.write_u32(0x8000, 0x1400_0000).unwrap();
        let mut cpu = Cpu::new();
        cpu.regs.pc = 0x8000;
        let mut engine = NceEngine {
            patcher: Patcher::new(0x1000, STUB_CAPACITY).unwrap(), areas: vec![], written: vec![],
            patched: HashSet::new(), interpreted: HashSet::from([8]), interpreted_steps: 0,
            exec_gen: 0, skip: vec![], stats: Stats::default(), entries: 0,
        };
        assert_eq!(engine.interpret(&mut cpu, &mut memory, std::time::Instant::now() + SLICE),
            Some(NceExit::Preempted));
        assert_eq!(cpu.regs.pc, 0x8000);
    }
}

/// Why [`NceEngine::run`] returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NceExit {
    /// The guest called `svc #id`; `cpu.regs.pc` is past the `svc`.
    Svc(u16),
    /// The time slice ran out; the scheduler may switch threads. `cpu` holds the full state.
    Preempted,
    /// The guest faulted (signal number, faulting pc, fault address).
    Fault { pc: u64, signal: i32, addr: u64 },
    /// The engine could not rewrite, map or enter guest code.
    Error(String),
}

pub struct NceEngine {
    patcher: Patcher,
    /// Stub areas `(start, end)` in guest memory: mapped executable, never rewritten themselves.
    areas: Vec<(u64, u64)>,
    /// Words of each area already in guest memory.
    written: Vec<usize>,
    /// 4 KiB guest pages already rewritten (or, for `interpreted` ones, examined).
    patched: HashSet<u64>,
    /// Executable 4 KiB pages in host pages shared with writable data: run on the interpreter.
    interpreted: HashSet<u64>,
    pub interpreted_steps: u64,
    exec_gen: u64,
    /// Ranges never rewritten (the NRO header, which is data).
    skip: Vec<(u64, u64)>,
    pub stats: Stats,
    pub entries: u64,
}

fn err(e: NceError) -> String {
    e.to_string()
}

impl NceEngine {
    /// Sets up the patch area for a guest whose layout starts at `base` (`HorizonKernel::addr_base`).
    /// `mmu` must be arena-backed.
    pub fn new(mmu: &mut Memory, base: u64, skip: Vec<(u64, u64)>) -> Result<Self, String> {
        if mmu.arena().is_none() {
            return Err("native execution needs arena-backed guest memory".into());
        }
        let ctx = base + PATCH_AREA;
        let patcher = Patcher::new(ctx, STUB_CAPACITY).map_err(err)?;
        mmu.map_range(ctx, CONTEXT_AREA as usize, Permission::READ_WRITE);
        let code = patcher.code_base();
        mmu.map_range(code, STUB_CAPACITY as usize, Permission::READ_EXECUTE);
        Ok(Self { patcher, areas: vec![(code, code + STUB_CAPACITY)], written: vec![0], patched: HashSet::new(),
            interpreted: HashSet::new(), interpreted_steps: 0, exec_gen: u64::MAX, skip, stats: Stats::default(), entries: 0 })
    }

    /// A free, unmapped spot for a satellite area near `at`, or `None`.
    fn satellite_spot(mmu: &Memory, at: u64) -> Option<u64> {
        let need = SATELLITE_CAPACITY + 2 * HOST_PAGE_MAX;
        let step = 0x10_0000u64;
        (1..64u64).flat_map(|i| [at.checked_sub(i * step), at.checked_add(i * step)]).flatten().find_map(|c| {
            let c = c & !(HOST_PAGE_MAX - 1);
            let (lo, hi, mapped) = mmu.region_at(c - HOST_PAGE_MAX);
            (!mapped && lo <= c - HOST_PAGE_MAX && hi >= c - HOST_PAGE_MAX + need).then_some(c)
        })
    }

    /// Rewrites and seals executable pages mapped since the last call, and writes new stubs.
    fn sync_code(&mut self, mmu: &mut Memory) -> Result<(), String> {
        if mmu.exec_map_gen() == self.exec_gen {
            return Ok(());
        }
        let regions = mmu.regions();
        let host_page = mmu.arena().ok_or("no arena")?.host_page_size();
        let mut sealed = Vec::new();
        for &(start, end, perm) in &regions {
            if !perm.execute || self.areas.iter().any(|&(a, b)| start >= a && end <= b) {
                continue;
            }
            let mut page = start;
            while page < end {
                if self.patched.contains(&(page >> 12)) {
                    page += 0x1000;
                    continue;
                }
                if Self::shares_host_page(&regions, page, host_page) {
                    self.patched.insert(page >> 12);
                    self.interpreted.insert(page >> 12);
                    page += 0x1000;
                    continue;
                }
                let run_start = page;
                while page < end && !self.patched.contains(&(page >> 12)) && !Self::shares_host_page(&regions, page, host_page) {
                    page += 0x1000;
                }
                self.rewrite_run(mmu, run_start, page)?;
                sealed.push((run_start, page));
            }
        }
        // New stub code, then seal everything that executes.
        let arena = mmu.arena_mut().ok_or("no arena")?;
        for (i, area) in self.patcher.areas().iter().enumerate() {
            let words = &area.code[self.written[i]..];
            if !words.is_empty() {
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                arena.write(area.base + self.written[i] as u64 * 4, &bytes).map_err(err)?;
                self.written[i] = area.code.len();
            }
        }
        for &(a, b) in &self.areas {
            arena.seal_exec(a, b - a).map_err(err)?;
        }
        for (start, end) in sealed {
            arena.seal_exec(start, end - start).map_err(err)?;
        }
        self.exec_gen = mmu.exec_map_gen();
        Ok(())
    }

    /// True if the host page holding guest `page` also holds writable, non-executable guest pages,
    /// which sealing would make read-only.
    fn shares_host_page(regions: &[(u64, u64, Permission)], page: u64, host_page: u64) -> bool {
        let lo = page & !(host_page - 1);
        let hi = lo + host_page;
        regions.iter().any(|&(s, e, perm)| s < hi && e > lo && perm.write && !perm.execute)
    }

    fn rewrite_run(&mut self, mmu: &mut Memory, start: u64, end: u64) -> Result<(), String> {
        if !self.patcher.reaches(start) || !self.patcher.reaches(end - 4) {
            let spot = Self::satellite_spot(mmu, start).ok_or_else(|| format!("no room for stubs near {start:#x}"))?;
            mmu.map_range(spot, SATELLITE_CAPACITY as usize, Permission::READ_EXECUTE);
            self.patcher.add_area(spot, SATELLITE_CAPACITY);
            self.areas.push((spot, spot + SATELLITE_CAPACITY));
            self.written.push(0);
        }
        let mut bytes = vec![0u8; (end - start) as usize];
        mmu.read_bytes(start, &mut bytes).map_err(|e| e.to_string())?;
        let mut words: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        // Rewrite each stretch between skipped ranges.
        let mut cuts: Vec<(u64, u64)> = self.skip.iter().copied().filter(|&(a, b)| a < end && b > start).collect();
        cuts.sort_unstable();
        let mut at = start;
        for (a, b) in cuts.into_iter().chain([(end, end)]) {
            let (seg_end, next) = (a.max(at).min(end), b.max(at).min(end));
            if seg_end > at {
                let (i, j) = (((at - start) / 4) as usize, ((seg_end - start) / 4) as usize);
                let s = self.patcher.rewrite(at, &mut words[i..j]).map_err(err)?;
                self.stats.svc += s.svc;
                self.stats.timer += s.timer;
                self.stats.tpidr += s.tpidr;
                self.stats.x18 += s.x18;
                self.stats.constant += s.constant;
            }
            at = next;
        }
        let out: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        mmu.arena_mut().ok_or("no arena")?.write(start, &out).map_err(err)?;
        for p in (start >> 12)..(end >> 12) {
            self.patched.insert(p);
        }
        Ok(())
    }

    /// Runs the current guest thread natively until it calls an SVC or faults. Timer reads are
    /// answered with `tick()` (19.2 MHz) without leaving this call.
    pub fn run(&mut self, cpu: &mut Cpu, mmu: &mut Memory, tick: &dyn Fn() -> u64) -> NceExit {
        match self.run_inner(cpu, mmu, tick) {
            Ok(exit) => exit,
            Err(error) => NceExit::Error(error),
        }
    }

    fn run_inner(&mut self, cpu: &mut Cpu, mmu: &mut Memory, tick: &dyn Fn() -> u64) -> Result<NceExit, String> {
        self.sync_code(mmu)?;
        let deadline = std::time::Instant::now() + SLICE;
        loop {
            if std::time::Instant::now() >= deadline { return Ok(NceExit::Preempted); }
            if self.interpreted.contains(&(cpu.regs.pc >> 12)) {
                if let Some(exit) = self.interpret(cpu, mmu, deadline) {
                    return Ok(exit);
                }
                continue;
            }
            match self.native(cpu, mmu, tick, deadline)? {
                // Native code branched into an interpreted page: carry on there.
                NceExit::Fault { pc, signal, addr } if addr == pc && self.interpreted.contains(&(pc >> 12))
                    && (signal == libc_sigsegv() || signal == libc_sigbus()) => continue,
                exit => return Ok(exit),
            }
        }
    }

    /// Steps the interpreter while the pc stays in interpreted pages. `None` = left them.
    fn interpret(&mut self, cpu: &mut Cpu, mmu: &mut Memory, deadline: std::time::Instant) -> Option<NceExit> {
        use super::interpreter::StepResult;
        while self.interpreted.contains(&(cpu.regs.pc >> 12)) {
            if std::time::Instant::now() >= deadline { return Some(NceExit::Preempted); }
            let pc = cpu.regs.pc;
            self.interpreted_steps += 1;
            match cpu.step(mmu) {
                Ok(StepResult::Continue) => {}
                Ok(StepResult::Svc(id)) => return Some(NceExit::Svc(id)),
                Ok(StepResult::Halt(message)) => return Some(NceExit::Error(format!("interpreter halt at {pc:#x}: {message}"))),
                Err(error) => return Some(NceExit::Fault { pc, signal: 0, addr: 0 }).inspect(|_| {
                    eprintln!("[NCE] interpreted page fault at {pc:#x}: {error}");
                }),
            }
        }
        None
    }

    fn native(&mut self, cpu: &mut Cpu, mmu: &mut Memory, tick: &dyn Fn() -> u64, deadline: std::time::Instant) -> Result<NceExit, String> {
        let ctx_va = self.patcher.context();
        let mut no_preempt = vec![(ctx_va, ctx_va + ounce_nce::patch::CONTEXT_AREA)];
        no_preempt.extend(self.areas.iter().copied());
        if no_preempt.len() > ounce_nce::MAX_NO_PREEMPT {
            return Err("too many patch areas".into());
        }
        ounce_nce::set_preemption(Some(SLICE), &no_preempt);
        let arena = mmu.arena_mut().ok_or("no arena")?;
        {
            let ctx = arena.context_mut(ctx_va).map_err(err)?;
            let r = &cpu.regs;
            ctx.x = r.x;
            ctx.sp = r.sp;
            ctx.nzcv = r.pstate.to_bits() as u64;
            ctx.v = r.v;
            ctx.tpidr = r.tpidr_el0;
            ctx.tpidrro = r.tpidrro_el0;
        }
        let pc = cpu.regs.pc;
        let mut target = match self.patcher.resume_for(pc) {
            Some(resume) => resume,
            None => {
                let slot = self.patcher.launch_slot(pc).ok_or_else(|| format!("no launch slot reaches {pc:#x}"))?;
                let words = Patcher::launch_words(slot, pc).map_err(err)?;
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                arena.write(slot, &bytes).map_err(err)?;
                slot
            }
        };
        loop {
            self.entries += 1;
            let exit = ounce_nce::enter(arena, ctx_va, target).map_err(err)?;
            let ctx = arena.context_mut(ctx_va).map_err(err)?;
            let result = match exit {
                Exit::Trampoline(id) => {
                    let site = *self.patcher.sites().get(id as usize).ok_or_else(|| format!("unknown exit id {id}"))?;
                    match site.kind {
                        SiteKind::ReadTimer { rt, sysreg } => {
                            if rt != 31 {
                                ctx.x[rt as usize] = if sysreg == SYSREG_CNTFRQ_EL0 { TIMER_HZ } else { tick() };
                            }
                            if std::time::Instant::now() >= deadline {
                                ctx.pc = site.at + 4;
                                NceExit::Preempted
                            } else {
                                target = site.resume;
                                continue;
                            }
                        }
                        SiteKind::Svc(id) => {
                            ctx.pc = site.at + 4;
                            NceExit::Svc(id)
                        }
                    }
                }
                Exit::Fault { signal, pc, addr } => NceExit::Fault { pc, signal, addr },
                Exit::Preempted { .. } => NceExit::Preempted,
            };
            let r = &mut cpu.regs;
            r.x = ctx.x;
            r.sp = ctx.sp;
            r.pc = ctx.pc;
            r.pstate.from_bits(ctx.nzcv as u32);
            r.v = ctx.v;
            r.tpidr_el0 = ctx.tpidr;
            r.tpidrro_el0 = ctx.tpidrro;
            return Ok(result);
        }
    }
}
