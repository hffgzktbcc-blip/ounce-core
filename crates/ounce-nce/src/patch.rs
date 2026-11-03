//! The rewriter: turns a Horizon AArch64 code range into one that can run on the host CPU.
//!
//! Ported from nxnative's `dm/tools/patch_nso.py` (MIT, seedds), keeping its rules and dropping
//! everything game-specific (registered cuts, spin gates, engine fixes). Each rewritten instruction
//! becomes a `B` to a stub in the patch area, and the stub branches back to the next instruction:
//!
//! | Guest instruction | Why | Stub |
//! | --- | --- | --- |
//! | `svc #n` | the host kernel has no Horizon SVCs | exit to the host (`site_gate`) |
//! | `mrs xN, cntpct_el0` / `cntvct_el0` / `cntfrq_el0` | Horizon's 19.2 MHz timer | exit to the host |
//! | `mrs xN, tpidrro_el0` / `tpidr_el0` | the host owns these registers | load from the [`Context`](crate::Context) (`site_sysreg_read`) |
//! | `msr tpidr_el0, xN` | as above | store to the context (`site_tpidr_write`) |
//! | anything reading or writing `x18` | macOS changes `x18` at will | run on a scratch register loaded from and stored back to `Context::x[18]` (`site_plain`, `site_cond`, `site_pcrel`, `site_blr`) |
//!
//! Where nxnative finds its thread slots through `TPIDRRO_EL0` (one key page per module), the
//! runtime here runs one guest thread at a time, so stubs address the single context page directly
//! with `ADRP`. Scratch registers are saved just below `sp` as nxnative does: AAPCS64 code never
//! keeps live data there.
//!
//! Patch area layout: the context page at `ctx` (its own host page, never executable), then at
//! `code_base` the shared exit, the launch slot and the stubs.

use crate::context::off;
use crate::NceError;
use std::collections::HashMap;

/// What a trampoline exit asks the host to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteKind {
    /// `svc #imm`.
    Svc(u16),
    /// `mrs x<rt>, <timer>`: the host writes the value to `x<rt>`.
    ReadTimer { rt: u8, sysreg: u16 },
}

/// A trampoline exit. Its index in [`Patcher::sites`] is the exit id the stub reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    pub kind: SiteKind,
    /// Address of the rewritten guest instruction.
    pub at: u64,
    /// Stub that reloads x16/x17 and continues at `at + 4`.
    pub resume: u64,
}

/// What a [`Patcher::rewrite`] call changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub svc: usize,
    pub timer: usize,
    pub tpidr: usize,
    pub x18: usize,
}

pub const SYSREG_TPIDR_EL0: u16 = 0x5E82;
pub const SYSREG_TPIDRRO_EL0: u16 = 0x5E83;
pub const SYSREG_CNTFRQ_EL0: u16 = 0x5F00;
pub const SYSREG_CNTPCT_EL0: u16 = 0x5F01;
pub const SYSREG_CNTVCT_EL0: u16 = 0x5F02;

/// Room the context needs before `code_base`, enough for any host page size up to 64 KiB.
pub const CONTEXT_AREA: u64 = 0x1_0000;

// ---- Encoders (nxnative `dm/tools/nso_asm.py`) ----

fn b(pc: u64, target: u64) -> Result<u32, NceError> {
    let d = target.wrapping_sub(pc) as i64;
    if d % 4 != 0 || !(-(1 << 27)..(1 << 27)).contains(&d) {
        return Err(NceError::Patch(format!("branch {pc:#x} -> {target:#x} out of range")));
    }
    Ok(0x1400_0000 | ((d >> 2) as u32 & 0x03FF_FFFF))
}
fn adrp(rd: u32, pc: u64, target: u64) -> Result<u32, NceError> {
    let d = ((target >> 12) as i64).wrapping_sub((pc >> 12) as i64);
    if !(-(1 << 20)..(1 << 20)).contains(&d) {
        return Err(NceError::Patch(format!("adrp {pc:#x} -> {target:#x} out of range")));
    }
    let d = d as u32;
    Ok(0x9000_0000 | (d & 3) << 29 | ((d >> 2) & 0x7FFFF) << 5 | rd)
}
fn add_imm(rd: u32, rn: u32, imm: u32) -> u32 { 0x9100_0000 | (imm & 0xFFF) << 10 | rn << 5 | rd }
fn ldr_x(rt: u32, rn: u32, off: u32) -> u32 { 0xF940_0000 | (off / 8) << 10 | rn << 5 | rt }
fn str_x(rt: u32, rn: u32, off: u32) -> u32 { 0xF900_0000 | (off / 8) << 10 | rn << 5 | rt }
fn stur_x(rt: u32, rn: u32, off: i32) -> u32 { 0xF800_0000 | (off as u32 & 0x1FF) << 12 | rn << 5 | rt }
fn ldur_x(rt: u32, rn: u32, off: i32) -> u32 { 0xF840_0000 | (off as u32 & 0x1FF) << 12 | rn << 5 | rt }
fn stp_x(t1: u32, t2: u32, rn: u32, off: i32) -> u32 { 0xA900_0000 | ((off / 8) as u32 & 0x7F) << 15 | t2 << 10 | rn << 5 | t1 }
fn ldp_x(t1: u32, t2: u32, rn: u32, off: i32) -> u32 { 0xA940_0000 | ((off / 8) as u32 & 0x7F) << 15 | t2 << 10 | rn << 5 | t1 }
fn stp_q(t1: u32, t2: u32, rn: u32, off: i32) -> u32 { 0xAD00_0000 | ((off / 16) as u32 & 0x7F) << 15 | t2 << 10 | rn << 5 | t1 }
fn movz_x(rd: u32, imm: u32) -> u32 { 0xD280_0000 | (imm & 0xFFFF) << 5 | rd }
fn movk_x(rd: u32, imm: u32, hw: u32) -> u32 { 0xF280_0000 | hw << 21 | (imm & 0xFFFF) << 5 | rd }
fn br(rn: u32) -> u32 { 0xD61F_0000 | rn << 5 }
fn mrs_nzcv(rt: u32) -> u32 { 0xD53B_4200 | rt }
fn mrs_fpcr(rt: u32) -> u32 { 0xD53B_4400 | rt }
fn mrs_fpsr(rt: u32) -> u32 { 0xD53B_4420 | rt }
const BRK_LAUNCH: u32 = 0xD420_0000 | 0x4E43 << 5; // brk #0x4E43 ("NC")

// ---- Which fields of an instruction name general-purpose registers ----

/// A general-purpose register operand: its field position and whether 31 means `sp` there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Field { shift: u8, sp: bool }

/// How an instruction uses general-purpose registers, as far as the rewriter must know.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Use {
    fields: Vec<Field>,
    /// Reads the PC (adr/adrp, literal loads, PC-relative branches): cannot move to a stub as is.
    pc_relative: bool,
    /// `br` / `blr` / `ret` through a register.
    branch_reg: bool,
    /// A load/store with base `sp`: (lowest byte offset accessed from the old `sp`, `sp` writeback delta).
    sp_access: Option<(i64, i64)>,
}

fn f(shift: u8) -> Field { Field { shift, sp: false } }
fn fsp(shift: u8) -> Field { Field { shift, sp: true } }
fn sext(v: u32, bits: u32) -> i64 { ((v as i64) << (64 - bits)) >> (64 - bits) }

/// The general-purpose register fields of `w` (A64 encoding index, Arm ARM C4.1).
fn gpr_use(w: u32) -> Use {
    let op0 = (w >> 25) & 0xF;
    let mut u = Use::default();
    match op0 {
        // Data processing, immediate.
        0b1000 | 0b1001 => match (w >> 23) & 7 {
            0b000 | 0b001 => { u.fields = vec![f(0)]; u.pc_relative = true; }
            0b010 => { let s = w >> 29 & 1 == 1; u.fields = vec![Field { shift: 0, sp: !s }, fsp(5)]; }
            0b100 => { let ands = (w >> 29) & 3 == 3; u.fields = vec![Field { shift: 0, sp: !ands }, f(5)]; }
            0b101 => u.fields = vec![f(0)],
            0b110 => u.fields = vec![f(0), f(5)],
            0b111 => u.fields = vec![f(0), f(5), f(16)],
            _ => u.fields = vec![fsp(0), fsp(5)], // add/sub with tags (MTE)
        },
        // Branches, exception generation, system.
        0b1010 | 0b1011 => {
            if w & 0x7C00_0000 == 0x1400_0000 {
                u.pc_relative = true; // b / bl
            } else if w & 0x7E00_0000 == 0x3400_0000 || w & 0x7E00_0000 == 0x3600_0000 {
                u.fields = vec![f(0)]; u.pc_relative = true; // cbz/cbnz, tbz/tbnz
            } else if w & 0xFE00_0000 == 0x5400_0000 {
                u.pc_relative = true; // b.cond
            } else if w & 0xFE00_0000 == 0xD600_0000 {
                u.fields = vec![f(5)]; u.branch_reg = true; // br/blr/ret (and the PAC forms, which use Rm=31 or sp)
            } else if w & 0xFFC0_0000 == 0xD500_0000 {
                // System: hints and barriers name no register (Rt=31); sys/sysl/msr/mrs use Rt.
                if (w >> 19) & 3 != 0 || (w >> 21) & 1 == 1 { u.fields = vec![f(0)]; }
            }
        }
        // Loads and stores.
        0b0100 | 0b0110 | 0b1100 | 0b1110 => {
            let v = w >> 26 & 1 == 1;
            let rt = |u: &mut Use| if !v { u.fields.push(f(0)) };
            match (w >> 27) & 7 {
                0b011 => { rt(&mut u); u.pc_relative = true; } // load literal
                0b101 => {
                    // Pairs: rt, rn, rt2.
                    rt(&mut u);
                    if !v { u.fields.push(f(10)); }
                    u.fields.push(fsp(5));
                    let opc = w >> 30;
                    let scale = if v { 2 + opc } else { 2 + (opc >> 1) };
                    let imm = sext((w >> 15) & 0x7F, 7) << scale;
                    if (w >> 5) & 31 == 31 {
                        u.sp_access = Some(match (w >> 23) & 3 {
                            0b01 => (0, imm),   // post-index
                            0b11 => (imm, imm), // pre-index
                            _ => (imm, 0),
                        });
                    }
                }
                0b111 => {
                    rt(&mut u);
                    u.fields.push(fsp(5));
                    let base_sp = (w >> 5) & 31 == 31;
                    if w >> 24 & 1 == 1 {
                        if base_sp { u.sp_access = Some((0, 0)); } // unsigned offset: never below sp
                    } else if w >> 21 & 1 == 0 {
                        let imm = sext((w >> 12) & 0x1FF, 9);
                        if base_sp {
                            u.sp_access = Some(match (w >> 10) & 3 {
                                0b01 => (0, imm),
                                0b11 => (imm, imm),
                                _ => (imm, 0),
                            });
                        }
                    } else {
                        // Register offset, atomics (Rs), PAC loads: a register at 16 except for PAC.
                        if (w >> 10) & 1 == 0 { u.fields.push(f(16)); }
                        if base_sp { u.sp_access = Some((0, 0)); }
                    }
                }
                0b001 if !v => {
                    // Exclusives and ordered (rs at 16, rt2 at 10, rn, rt).
                    u.fields.extend([f(0), f(10), fsp(5), f(16)]);
                }
                0b001 => {
                    // SIMD structure loads/stores: rn, and rm for the register post-index form.
                    u.fields.push(fsp(5));
                    if w >> 23 & 1 == 1 && (w >> 16) & 31 != 31 { u.fields.push(f(16)); }
                }
                _ => { rt(&mut u); u.fields.push(fsp(5)); }
            }
        }
        // Data processing, register.
        0b0101 | 0b1101 => {
            if w >> 28 & 1 == 0 {
                if w >> 24 & 1 == 1 && w >> 21 & 1 == 1 {
                    let s = w >> 29 & 1 == 1;
                    u.fields = vec![Field { shift: 0, sp: !s }, fsp(5), f(16)]; // add/sub extended
                } else {
                    u.fields = vec![f(0), f(5), f(16)];
                }
            } else {
                match (w >> 21) & 0xF {
                    0b0010 => u.fields = if (w >> 11) & 1 == 0 { vec![f(5), f(16)] } else { vec![f(5)] }, // ccmp
                    0b0110 if w >> 30 & 1 == 1 => u.fields = vec![f(0), f(5)], // 1-source
                    0b1000..=0b1111 => u.fields = vec![f(0), f(5), f(16), f(10)], // 3-source
                    _ => u.fields = vec![f(0), f(5), f(16)],
                }
            }
        }
        // SIMD and floating point: only the forms that move between general and vector registers.
        0b0111 | 0b1111 => {
            if (w >> 24) & 0x5F == 0x1E && w >> 21 & 1 == 1 && (w >> 10) & 0x3F == 0 {
                // FP <-> integer conversion.
                u.fields = match (w >> 16) & 7 { 0b010 | 0b011 | 0b111 => vec![f(5)], _ => vec![f(0)] };
            } else if (w >> 24) & 0x5F == 0x1E && w >> 21 & 1 == 0 {
                // FP <-> fixed point.
                u.fields = match (w >> 16) & 7 { 0b010 | 0b011 => vec![f(5)], 0b000 | 0b001 => vec![f(0)], _ => vec![] };
            } else if w & 0x9FE0_8400 == 0x0E00_0400 {
                // Advanced SIMD copy: dup/ins (general) read Rn, smov/umov write Rd.
                u.fields = match (w >> 11) & 0xF { 0b0001 | 0b0011 => vec![f(5)], 0b0101 | 0b0111 => vec![f(0)], _ => vec![] };
            }
        }
        _ => {}
    }
    u
}

fn reg(w: u32, fl: Field) -> u32 { (w >> fl.shift) & 31 }

fn uses_x18(w: u32) -> Option<Use> {
    let u = gpr_use(w);
    u.fields.iter().any(|&fl| reg(w, fl) == 18).then_some(u)
}

/// Two scratch registers from x0..x15 that the instruction does not name in any field.
fn pick(w: u32, n: usize) -> Vec<u32> {
    let named: Vec<u32> = [0, 5, 10, 16].iter().map(|s| (w >> s) & 31).collect();
    (0..16).filter(|r| !named.contains(r)).take(n).collect()
}

fn subst(w: u32, u: &Use, s: u32) -> u32 {
    u.fields.iter().filter(|&&fl| reg(w, fl) == 18).fold(w, |nw, fl| (nw & !(31 << fl.shift)) | s << fl.shift)
}

/// A run of stub code: the main area after the context page, or a satellite area placed near code
/// that lies beyond `B` range (±128 MiB) of the main one. Each area starts with its launch slot.
#[derive(Debug, Clone)]
pub struct Area {
    pub base: u64,
    pub capacity: u64,
    pub code: Vec<u32>,
}

impl Area {
    pub fn launch_slot(&self) -> u64 { self.base }
    fn here(&self) -> u64 { self.base + self.code.len() as u64 * 4 }
    fn reaches(&self, at: u64) -> bool {
        let (lo, hi) = (self.base.min(at), (self.base + self.capacity).max(at));
        hi - lo < (1 << 27) - 0x1000
    }
}

/// Builds the patch areas and rewrites code ranges into them.
#[derive(Debug, Clone)]
pub struct Patcher {
    ctx: u64,
    areas: Vec<Area>,
    /// The area stubs are emitted into right now.
    cur: usize,
    common_exit: u64,
    sites: Vec<Site>,
    resume_by_pc: HashMap<u64, u64>,
}

impl Patcher {
    /// A patch area with its context page at `ctx` and `capacity` bytes of stubs at
    /// `ctx + CONTEXT_AREA`. Code beyond ±128 MiB of it needs an [`Patcher::add_area`] first.
    pub fn new(ctx: u64, capacity: u64) -> Result<Self, NceError> {
        let mut p = Self { ctx, areas: Vec::new(), cur: 0, common_exit: 0, sites: Vec::new(), resume_by_pc: HashMap::new() };
        p.add_area(ctx + CONTEXT_AREA, capacity);
        p.emit_common_exit()?;
        Ok(p)
    }

    /// Adds a satellite area of `capacity` bytes at `base` (host-page aligned, mapped by the caller).
    pub fn add_area(&mut self, base: u64, capacity: u64) {
        self.areas.push(Area { base, capacity, code: vec![BRK_LAUNCH, BRK_LAUNCH] });
    }

    pub fn context(&self) -> u64 { self.ctx }
    /// The main area's start.
    pub fn code_base(&self) -> u64 { self.areas[0].base }
    /// The stub areas; each one's `code` is to be written at its `base`.
    pub fn areas(&self) -> &[Area] { &self.areas }
    pub fn sites(&self) -> &[Site] { &self.sites }
    /// True if some area can hold stubs for code at `at`.
    pub fn reaches(&self, at: u64) -> bool { self.areas.iter().any(|a| a.reaches(at)) }
    /// A launch slot within `B` range of `target`: two words the host rewrites with
    /// [`Patcher::launch_words`] to enter guest code at an address no resume stub covers.
    pub fn launch_slot(&self, target: u64) -> Option<u64> {
        self.areas.iter().find(|a| a.reaches(target)).map(Area::launch_slot)
    }
    /// The resume stub for a guest `pc` right after a trampoline site, if there is one.
    pub fn resume_for(&self, pc: u64) -> Option<u64> { self.resume_by_pc.get(&pc).copied() }

    /// `ldp x16, x17, [sp, #-16]; b target`, for the launch slot at `slot`.
    pub fn launch_words(slot: u64, target: u64) -> Result<[u32; 2], NceError> {
        Ok([ldp_x(16, 17, 31, -16), b(slot + 4, target)?])
    }

    fn here(&self) -> u64 { self.areas[self.cur].here() }
    fn emit(&mut self, w: u32) { self.areas[self.cur].code.push(w); }

    /// `rd` = the context address: `ADRP` when the context is within ±4 GiB, else an inline literal.
    fn ctx_addr(&mut self, rd: u32) -> Result<(), NceError> {
        let pc = self.here();
        match adrp(rd, pc, self.ctx) {
            Ok(w) => self.emit(w),
            Err(_) => {
                self.emit(0x5800_0040 | rd); // ldr xRd, #8
                self.emit(0x1400_0003);      // b #12
                self.emit(self.ctx as u32);
                self.emit((self.ctx >> 32) as u32);
            }
        }
        Ok(())
    }

    /// Store every guest register to the context and return to the host (x16 = exit id, guest
    /// x16/x17 at [sp - 16]).
    fn emit_common_exit(&mut self) -> Result<(), NceError> {
        self.common_exit = self.here();
        self.ctx_addr(17)?;
        self.emit(str_x(16, 17, off::EXIT_ID));
        for r in (0..16).step_by(2) {
            self.emit(stp_x(r, r + 1, 17, (off::X + r * 8) as i32));
        }
        self.emit(ldp_x(0, 1, 31, -16));
        self.emit(stp_x(0, 1, 17, (off::X + 16 * 8) as i32));
        self.emit(str_x(19, 17, off::X + 19 * 8));
        for r in (20..30).step_by(2) {
            self.emit(stp_x(r, r + 1, 17, (off::X + r * 8) as i32));
        }
        self.emit(str_x(30, 17, off::X + 30 * 8));
        self.emit(add_imm(0, 31, 0));
        self.emit(str_x(0, 17, off::SP));
        self.emit(mrs_nzcv(0));
        self.emit(str_x(0, 17, off::NZCV));
        self.emit(mrs_fpcr(0));
        self.emit(str_x(0, 17, off::FPCR));
        self.emit(mrs_fpsr(0));
        self.emit(str_x(0, 17, off::FPSR));
        self.emit(add_imm(1, 17, off::V));
        for q in (0..32).step_by(2) {
            self.emit(stp_q(q, q + 1, 1, q as i32 * 16));
        }
        self.emit(ldr_x(16, 17, off::HOST_RET));
        self.emit(br(16));
        Ok(())
    }

    fn back(&mut self, at: u64) -> Result<(), NceError> {
        let pc = self.here();
        let w = b(pc, at + 4)?;
        self.emit(w);
        Ok(())
    }

    /// `site_gate`: leave guest code with a new exit id.
    fn site_gate(&mut self, at: u64, kind: SiteKind) -> Result<u64, NceError> {
        let start = self.here();
        let id = self.sites.len() as u32;
        if id > 0xFFFF {
            return Err(NceError::Patch("more than 65536 trampoline sites".into()));
        }
        self.emit(stp_x(16, 17, 31, -16));
        self.emit(movz_x(16, id));
        let pc = self.here();
        match b(pc, self.common_exit) {
            Ok(w) => self.emit(w),
            Err(_) => {
                // A satellite area: jump to the shared exit through x17 (saved above).
                self.emit(0x5800_0051); // ldr x17, #8
                self.emit(br(17));
                self.emit(self.common_exit as u32);
                self.emit((self.common_exit >> 32) as u32);
            }
        }
        let resume = self.here();
        self.emit(ldp_x(16, 17, 31, -16));
        self.back(at)?;
        self.sites.push(Site { kind, at, resume });
        self.resume_by_pc.insert(at + 4, resume);
        Ok(start)
    }

    /// `site_sysreg_read`: `mrs xN, tpidr(ro)_el0` -> xN = context field.
    fn site_sysreg_read(&mut self, at: u64, rt: u32, field: u32) -> Result<u64, NceError> {
        let start = self.here();
        if rt != 31 {
            self.ctx_addr(rt)?;
            self.emit(ldr_x(rt, rt, field));
        }
        self.back(at)?;
        Ok(start)
    }

    /// `site_tpidr_write`: `msr tpidr_el0, xN` -> context field = xN.
    fn site_tpidr_write(&mut self, at: u64, rt: u32) -> Result<u64, NceError> {
        let start = self.here();
        let t = if rt == 16 { 17 } else { 16 };
        self.emit(stur_x(t, 31, -16));
        self.ctx_addr(t)?;
        self.emit(str_x(rt, t, off::TPIDR));
        self.emit(ldur_x(t, 31, -16));
        self.back(at)?;
        Ok(start)
    }

    /// Emits `xS = guest x18` with `xT` holding the context address (both saved first by the caller).
    fn load18(&mut self, s: u32, t: u32) -> Result<(), NceError> {
        self.ctx_addr(t)?;
        self.emit(ldr_x(s, t, off::X + 18 * 8));
        Ok(())
    }

    /// `site_plain`: run the instruction with x18 replaced by a scratch register, then store it back.
    fn site_plain(&mut self, at: u64, w: u32, u: &Use) -> Result<u64, NceError> {
        let st = pick(w, 2);
        let (s, t) = (st[0], st[1]);
        let (low, dlt) = u.sp_access.unwrap_or((0, 0));
        // Save area below everything the instruction touches relative to the old or new sp.
        let save = ((low.min(dlt).min(0) - 16) & !15) as i32;
        if save < -512 {
            return Err(NceError::Patch(format!("{at:#x}: sp offset {low} too far for the x18 save area")));
        }
        let start = self.here();
        self.emit(stp_x(s, t, 31, save));
        self.load18(s, t)?;
        self.emit(subst(w, u, s));
        self.emit(str_x(s, t, off::X + 18 * 8));
        self.emit(ldp_x(s, t, 31, save - dlt as i32));
        self.back(at)?;
        Ok(start)
    }

    /// `site_sp`: the instruction writes sp from x18 (`mov sp, x18` / `add sp, x18, ...`).
    fn site_sp(&mut self, at: u64, w: u32, u: &Use) -> Result<u64, NceError> {
        let st = pick(w, 2);
        let (s, t) = (st[0], st[1]);
        let start = self.here();
        self.emit(stp_x(s, t, 31, -16));
        self.load18(s, t)?;
        self.emit(add_imm(t, 31, 0)); // t = old sp
        self.emit(subst(w, u, s));
        self.emit(ldp_x(s, t, t, -16));
        self.back(at)?;
        Ok(start)
    }

    /// `site_cond`: `cbz/cbnz/tbz/tbnz x18`.
    fn site_cond(&mut self, at: u64, w: u32) -> Result<u64, NceError> {
        let st = pick(w, 2);
        let (s, t) = (st[0], st[1]);
        let tb = w & 0x7E00_0000 == 0x3600_0000;
        let target = if tb { at.wrapping_add((sext((w >> 5) & 0x3FFF, 14) * 4) as u64) }
            else { at.wrapping_add((sext((w >> 5) & 0x7FFFF, 19) * 4) as u64) };
        let start = self.here();
        self.emit(stp_x(s, t, 31, -16));
        self.load18(s, t)?;
        // Branch over the fall-through exit (3 words ahead) when the condition holds.
        let nw = if tb { (w & 0xFFF8_0000) | 3 << 5 | s } else { (w & 0xFF00_0000) | 3 << 5 | s };
        self.emit(nw);
        self.emit(ldp_x(s, t, 31, -16));
        self.back(at)?;
        self.emit(ldp_x(s, t, 31, -16));
        let pc = self.here();
        self.emit(b(pc, target)?);
        Ok(start)
    }

    /// `site_pcrel`: `adr/adrp x18, label` -> context x18 = the address, computed here.
    fn site_pcrel(&mut self, at: u64, w: u32) -> Result<u64, NceError> {
        let imm = sext(((w >> 5) & 0x7FFFF) << 2 | (w >> 29) & 3, 21);
        let value = if w >> 31 == 1 { (at & !0xFFF).wrapping_add((imm << 12) as u64) } else { at.wrapping_add(imm as u64) };
        let (s, t) = (0, 1);
        let start = self.here();
        self.emit(stp_x(s, t, 31, -16));
        self.ctx_addr(t)?;
        self.emit(movz_x(s, value as u32 & 0xFFFF));
        for hw in 1..4 {
            self.emit(movk_x(s, (value >> (16 * hw)) as u32 & 0xFFFF, hw));
        }
        self.emit(str_x(s, t, off::X + 18 * 8));
        self.emit(ldp_x(s, t, 31, -16));
        self.back(at)?;
        Ok(start)
    }

    /// `site_blr`: `blr x18` -> x16 = guest x18, x30 = the original return address, `br x16`.
    /// x16/x17 are intra-procedure-call scratch registers at a call, so they need no saving.
    fn site_blr(&mut self, at: u64) -> Result<u64, NceError> {
        let start = self.here();
        self.load18(16, 17)?;
        let ret = at + 4;
        self.emit(movz_x(30, ret as u32 & 0xFFFF));
        for hw in 1..4 {
            self.emit(movk_x(30, (ret >> (16 * hw)) as u32 & 0xFFFF, hw));
        }
        self.emit(br(16));
        Ok(start)
    }

    /// Rewrites the code words of `[start, start + 4 * words.len())` in place, adding stubs.
    pub fn rewrite(&mut self, start: u64, words: &mut [u32]) -> Result<Stats, NceError> {
        let mut stats = Stats::default();
        for (i, word) in words.iter_mut().enumerate() {
            let at = start + i as u64 * 4;
            let w = *word;
            self.cur = self.areas.iter().position(|a| a.reaches(at))
                .ok_or_else(|| NceError::Patch(format!("no patch area within branch range of {at:#x}")))?;
            let stub = if w & 0xFFE0_001F == 0xD400_0001 {
                stats.svc += 1;
                Some(self.site_gate(at, SiteKind::Svc((w >> 5) as u16))?)
            } else if w & 0xFFF0_0000 == 0xD530_0000 {
                let sysreg = ((w >> 5) & 0x7FFF) as u16;
                let rt = w & 31;
                match sysreg {
                    SYSREG_TPIDR_EL0 => { stats.tpidr += 1; Some(self.site_sysreg_read(at, rt, off::TPIDR)?) }
                    SYSREG_TPIDRRO_EL0 => { stats.tpidr += 1; Some(self.site_sysreg_read(at, rt, off::TPIDRRO)?) }
                    SYSREG_CNTFRQ_EL0 | SYSREG_CNTPCT_EL0 | SYSREG_CNTVCT_EL0 => {
                        stats.timer += 1;
                        Some(self.site_gate(at, SiteKind::ReadTimer { rt: rt as u8, sysreg })?)
                    }
                    _ if rt == 18 => {
                        return Err(NceError::Patch(format!("{at:#x}: mrs x18, sysreg {sysreg:#x}")));
                    }
                    _ => None,
                }
            } else if w & 0xFFF0_0000 == 0xD510_0000 && ((w >> 5) & 0x7FFF) as u16 == SYSREG_TPIDR_EL0 {
                stats.tpidr += 1;
                Some(self.site_tpidr_write(at, w & 31)?)
            } else if let Some(u) = uses_x18(w) {
                stats.x18 += 1;
                Some(if u.branch_reg {
                    if w & 0xFFFF_FC1F == 0xD63F_0000 {
                        self.site_blr(at)?
                    } else {
                        return Err(NceError::Patch(format!("{at:#x}: br/ret through x18 ({w:#010x})")));
                    }
                } else if u.pc_relative {
                    if w & 0x1F00_0000 == 0x1000_0000 {
                        self.site_pcrel(at, w)?
                    } else if w & 0x7C00_0000 == 0x3400_0000 {
                        self.site_cond(at, w)?
                    } else {
                        return Err(NceError::Patch(format!("{at:#x}: PC-relative x18 form {w:#010x}")));
                    }
                } else if u.fields.first().is_some_and(|fl| fl.sp && fl.shift == 0 && reg(w, *fl) == 31) {
                    self.site_sp(at, w, &u)?
                } else {
                    self.site_plain(at, w, &u)?
                })
            } else {
                None
            };
            if let Some(stub) = stub {
                *word = b(at, stub)?;
            }
        }
        for a in &self.areas {
            if a.code.len() as u64 * 4 > a.capacity {
                return Err(NceError::Patch(format!("patch area at {:#x} needs {:#x} bytes, has {:#x}", a.base, a.code.len() * 4, a.capacity)));
            }
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoders_match_the_architecture() {
        assert_eq!(stp_x(16, 17, 31, -16), 0xA93F_47F0); // stp x16, x17, [sp, #-16]
        assert_eq!(ldp_x(16, 17, 31, -16), 0xA97F_47F0); // ldp x16, x17, [sp, #-16]
        assert_eq!(ldr_x(16, 17, 0x158), 0xF940_AE30); // ldr x16, [x17, #0x158]
        assert_eq!(str_x(30, 17, 240), 0xF900_7A3E); // str x30, [x17, #240]
        assert_eq!(stur_x(16, 31, -16), 0xF81F_03F0); // stur x16, [sp, #-16]
        assert_eq!(ldur_x(16, 31, -16), 0xF85F_03F0); // ldur x16, [sp, #-16]
        assert_eq!(stp_q(0, 1, 1, 0), 0xAD00_0420); // stp q0, q1, [x1]
        assert_eq!(add_imm(0, 31, 0), 0x9100_03E0); // mov x0, sp
        assert_eq!(movz_x(16, 5), 0xD280_00B0);
        assert_eq!(br(16), 0xD61F_0200);
        assert_eq!(mrs_nzcv(0), 0xD53B_4200);
        assert_eq!(b(0x1000, 0x1008).unwrap(), 0x1400_0002);
        assert_eq!(b(0x1008, 0x1000).unwrap(), 0x17FF_FFFE);
        assert_eq!(adrp(17, 0x1_0000_0FFC, 0x1_0000_3000).unwrap(), 0xF000_0011); // adrp x17, +3 pages
        assert!(b(0, 1 << 28).is_err());
    }

    #[test]
    fn classifies_x18_users() {
        for w in [
            0xA9014FF2u32, // stp x18, x19, [sp, #16]
            0xAA1203E0,    // mov x0, x18
            0xF2A00012,    // movk x18, #0, lsl #16
            0x8B120020,    // add x0, x1, x18
            0xF9400252,    // ldr x18, [x18]
            0x34000012,    // cbz w18, .
            0x910003F2,    // mov x18, sp
            0x9E670240,    // fmov d0, x18
            0x4E080E40,    // dup v0.2d, x18
        ] {
            assert!(uses_x18(w).is_some(), "{w:#010x}");
        }
        for w in [
            0x1E721A40u32, // fdiv s0, s18, s18 (vector register 18)
            0x4EB21E40,    // orr v0.16b, v18.16b, v18.16b
            0x3DC00032,    // ldr q18, [x1]
            0xD503201F,    // nop
            0xA9417BFD,    // ldp x29, x30, [sp, #16]
        ] {
            assert!(uses_x18(w).is_none(), "{w:#010x}");
        }
    }

    #[test]
    fn rewrites_svc_tls_and_x18_sites() {
        let ctx = 0x10_7C00_0000u64;
        let mut p = Patcher::new(ctx, 0x10_0000).unwrap();
        assert!(!p.reaches(0x7F_0000_0000));
        let code = 0x10_8000_0000u64;
        let mut words = [
            0xD4000E01u32, // svc #0x70
            0xD53BD060,    // mrs x0, tpidrro_el0
            0xD51BD041,    // msr tpidr_el0, x1
            0xAA1203E0,    // mov x0, x18
            0xD503201F,    // nop
            0xD53BE023,    // mrs x3, cntpct_el0
        ];
        let stats = p.rewrite(code, &mut words).unwrap();
        assert_eq!(stats, Stats { svc: 1, timer: 1, tpidr: 2, x18: 1 });
        assert_eq!(words[4], 0xD503201F);
        assert!(words.iter().enumerate().filter(|&(i, _)| i != 4).all(|(_, w)| w & 0xFC00_0000 == 0x1400_0000));
        assert_eq!(p.sites()[0].kind, SiteKind::Svc(0x70));
        assert_eq!(p.sites()[1].kind, SiteKind::ReadTimer { rt: 3, sysreg: SYSREG_CNTPCT_EL0 });
        assert_eq!(p.resume_for(code + 4), Some(p.sites()[0].resume));
        // An x18 branch target that cannot be expressed is refused, not mistranslated.
        assert!(p.rewrite(code, &mut [0xD61F0240]).is_err()); // br x18
        // Far code gets a satellite area; its gate reaches the shared exit through a literal.
        let far = 0x17_0000_0000u64;
        assert!(p.rewrite(far, &mut [0xD4000141]).is_err());
        p.add_area(far - 0x10_0000, 0x1000);
        let mut w = [0xD4000141u32, 0xD53BD060];
        p.rewrite(far, &mut w).unwrap();
        assert_eq!(p.areas()[1].code[2..4], [0xA93F47F0, 0xD2800000 | 2 << 5 | 16]);
        assert_eq!(p.areas()[1].code[4], 0x5800_0051);
    }
}
