//! Guest threads for the Linux runner. One host thread runs every guest thread in turn: a thread runs until it
//! blocks in `futex`, yields, exits, or uses up a fixed slice of instructions, then the next runnable thread in
//! creation order runs. The schedule depends only on retired instructions, so it is the same on every run (and
//! the engines agree on the output of any program without data races).
//!
//! A thread is its `CpuRegisters` (which include `TPIDR_EL0`); memory, files and signal handlers are the
//! process's. Futex waiters are matched on the address alone: private and shared futexes share one key space,
//! which is the same thing in a single process (Linux keeps them apart, so a program that waits with
//! `FUTEX_PRIVATE_FLAG` for the shared `CLONE_CHILD_CLEARTID` wake hangs there; here it is woken).

use super::syscalls::PID;
use crate::cpu::interpreter::Cpu;
use crate::cpu::registers::CpuRegisters;
use crate::mmu::memory::Memory;

pub const SYS_FUTEX: u64 = 98;
pub const SYS_EXIT: u64 = 93;
pub const SYS_SET_TID_ADDRESS: u64 = 96;
pub const SYS_SCHED_YIELD: u64 = 124;
pub const SYS_TKILL: u64 = 130;
pub const SYS_TGKILL: u64 = 131;
pub const SYS_GETTID: u64 = 178;
pub const SYS_CLONE: u64 = 220;
pub const SYS_PRCTL: u64 = 167;
const SYS_NANOSLEEP: u64 = 101;
const SYS_CLOCK_NANOSLEEP: u64 = 115;
const PR_SET_NAME: u64 = 15;
const PR_GET_NAME: u64 = 16;

const CLONE_VM: u64 = 0x100;
const CLONE_FS: u64 = 0x200;
const CLONE_FILES: u64 = 0x400;
const CLONE_SIGHAND: u64 = 0x800;
pub const CLONE_THREAD: u64 = 0x1_0000;
const CLONE_SYSVSEM: u64 = 0x4_0000;
const CLONE_SETTLS: u64 = 0x8_0000;
const CLONE_PARENT_SETTID: u64 = 0x10_0000;
const CLONE_CHILD_CLEARTID: u64 = 0x20_0000;
const CLONE_DETACHED: u64 = 0x40_0000;
const CLONE_CHILD_SETTID: u64 = 0x100_0000;
/// What `pthread_create` passes: a thread in this process sharing everything.
const THREAD_SHARE: u64 = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD | CLONE_SYSVSEM;
const THREAD_OPTIONAL: u64 =
    CLONE_SETTLS | CLONE_PARENT_SETTID | CLONE_CHILD_CLEARTID | CLONE_CHILD_SETTID | CLONE_DETACHED;

const FUTEX_WAIT: u64 = 0;
const FUTEX_WAKE: u64 = 1;
const FUTEX_REQUEUE: u64 = 3;
const FUTEX_CMP_REQUEUE: u64 = 4;
const FUTEX_WAIT_BITSET: u64 = 9;
const FUTEX_WAKE_BITSET: u64 = 10;
/// `FUTEX_PRIVATE_FLAG | FUTEX_CLOCK_REALTIME`: neither changes anything here
const FUTEX_OP_FLAGS: u64 = 128 | 256;

const EAGAIN: i64 = 11;
const EFAULT: i64 = 14;
const EINVAL: i64 = 22;
const ENOSYS: i64 = 38;
const ETIMEDOUT: i64 = 110;

/// Instructions a thread runs before the next runnable one gets the CPU.
pub const QUANTUM: usize = 20_000;

struct Wait {
    addr: u64,
    bitset: u32,
    timed: bool,
}

struct Thread {
    tid: u32,
    /// The thread's registers while it is not running (the running thread's live in `Cpu::regs`).
    regs: Option<Box<CpuRegisters>>,
    clear_child_tid: u64,
    wait: Option<Wait>,
    /// `prctl(PR_SET_NAME)`: at most 15 bytes, NUL-padded as the kernel keeps `comm`
    name: [u8; 16],
}

/// What a thread syscall did.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Not a thread syscall: hand it to `Process::dispatch`.
    NotMine,
    /// Return this value to the calling thread, which keeps running.
    Return(i64),
    /// The calling thread blocked, yielded or exited and another thread now runs.
    Switched,
    /// The last thread exited: the process exits with this status.
    ProcessExit(i32),
    /// No thread can run and none can ever be woken.
    Deadlock(String),
}

pub struct Threads {
    list: Vec<Thread>,
    current: usize,
    next_tid: u32,
    /// The exit status of the thread-group leader, which `wait` reports once every thread is gone.
    leader_status: Option<i32>,
}

impl Default for Threads {
    fn default() -> Self {
        Self::new()
    }
}

impl Threads {
    pub fn new() -> Self {
        Threads {
            list: vec![Thread {
                tid: PID as u32,
                regs: None,
                clear_child_tid: 0,
                wait: None,
                name: [0; 16],
            }],
            current: 0,
            next_tid: PID as u32 + 1,
            leader_status: None,
        }
    }

    /// How many instructions the running thread may execute before the scheduler looks again.
    pub fn slice(&self, budget_left: u64) -> usize {
        let cap = if self.list.len() > 1 { QUANTUM } else { 200_000 };
        budget_left.min(cap as u64) as usize
    }

    pub fn current_tid(&self) -> u32 {
        self.list[self.current].tid
    }

    /// Whether `tid` names a live thread of this process (for `tkill`/`tgkill`).
    pub fn is_live(&self, tid: u64) -> bool {
        self.list.iter().any(|t| u64::from(t.tid) == tid)
    }

    /// Handles the thread syscalls; anything else is `Step::NotMine`.
    pub fn syscall(&mut self, nr: u64, a: [u64; 6], cpu: &mut Cpu, mmu: &mut Memory) -> Step {
        match nr {
            SYS_CLONE if a[0] & CLONE_THREAD != 0 => self.clone_thread(a, cpu, mmu),
            SYS_FUTEX => self.futex(a, cpu, mmu),
            SYS_GETTID => Step::Return(i64::from(self.current_tid())),
            SYS_SET_TID_ADDRESS => {
                self.list[self.current].clear_child_tid = a[0];
                Step::Return(i64::from(self.current_tid()))
            }
            SYS_SCHED_YIELD if self.list.len() > 1 => {
                cpu.regs.write_x(0, 0);
                self.switch_from_current(cpu)
            }
            // while another thread can run, a sleep lets it: the sleeper continues after a turn
            SYS_NANOSLEEP | SYS_CLOCK_NANOSLEEP if self.list.iter().filter(|t| t.wait.is_none()).count() > 1 => {
                cpu.regs.write_x(0, 0);
                self.switch_from_current(cpu)
            }
            SYS_EXIT => self.exit_thread(a[0] as i32, cpu, mmu),
            SYS_PRCTL => self.prctl(a, mmu),
            _ => Step::NotMine,
        }
    }

    /// Preemption at the end of a slice: the next runnable thread, if there is another, takes over.
    pub fn preempt(&mut self, cpu: &mut Cpu) {
        if self.list.iter().filter(|t| t.wait.is_none()).count() > 1 {
            let _ = self.switch_from_current(cpu);
        }
    }

    fn clone_thread(&mut self, a: [u64; 6], cpu: &mut Cpu, mmu: &mut Memory) -> Step {
        let (flags, stack, ptid, tls, ctid) = (a[0], a[1], a[2], a[3], a[4]);
        // only the low byte (the exit signal) may differ from what pthread_create passes
        if flags & THREAD_SHARE != THREAD_SHARE || flags & !(THREAD_SHARE | THREAD_OPTIONAL | 0xFF) != 0 {
            return Step::Return(-EINVAL);
        }
        let tid = self.next_tid;
        if flags & CLONE_PARENT_SETTID != 0 && mmu.write_u32(ptid, tid).is_err() {
            return Step::Return(-EFAULT);
        }
        if flags & CLONE_CHILD_SETTID != 0 && mmu.write_u32(ctid, tid).is_err() {
            return Step::Return(-EFAULT);
        }
        self.next_tid += 1;
        let mut regs = Box::new(cpu.regs.clone());
        regs.write_x(0, 0);
        if stack != 0 {
            regs.sp = stack;
        }
        if flags & CLONE_SETTLS != 0 {
            regs.tpidr_el0 = tls;
        }
        let name = self.list[self.current].name;
        self.list.push(Thread {
            tid,
            regs: Some(regs),
            clear_child_tid: if flags & CLONE_CHILD_CLEARTID != 0 { ctid } else { 0 },
            wait: None,
            name,
        });
        Step::Return(i64::from(tid))
    }

    /// `prctl(PR_SET_NAME / PR_GET_NAME)` on the calling thread's name; other options are not supported (EINVAL).
    fn prctl(&mut self, a: [u64; 6], mmu: &mut Memory) -> Step {
        let name = &mut self.list[self.current].name;
        match a[0] {
            PR_SET_NAME => {
                let mut new = [0u8; 16];
                for (i, b) in new.iter_mut().take(15).enumerate() {
                    match mmu.read_u8(a[1] + i as u64) {
                        Ok(0) => break,
                        Ok(c) => *b = c,
                        Err(_) => return Step::Return(-EFAULT),
                    }
                }
                *name = new;
                Step::Return(0)
            }
            PR_GET_NAME => match mmu.write_bytes(a[1], name) {
                Ok(()) => Step::Return(0),
                Err(_) => Step::Return(-EFAULT),
            },
            _ => Step::Return(-EINVAL),
        }
    }

    fn futex(&mut self, a: [u64; 6], cpu: &mut Cpu, mmu: &mut Memory) -> Step {
        let (addr, op, val, timeout, addr2, val3) = (a[0], a[1] & !FUTEX_OP_FLAGS, a[2] as u32, a[3], a[4], a[5]);
        match op {
            FUTEX_WAIT | FUTEX_WAIT_BITSET => {
                let bitset = if op == FUTEX_WAIT { u32::MAX } else { val3 as u32 };
                if bitset == 0 {
                    return Step::Return(-EINVAL);
                }
                match mmu.read_u32(addr) {
                    Err(_) => Step::Return(-EFAULT),
                    Ok(v) if v != val => Step::Return(-EAGAIN),
                    Ok(_) => {
                        self.list[self.current].wait = Some(Wait {
                            addr,
                            bitset,
                            timed: timeout != 0,
                        });
                        // the value a waker leaves behind; a timeout replaces it with -ETIMEDOUT
                        cpu.regs.write_x(0, 0);
                        self.switch_from_current(cpu)
                    }
                }
            }
            FUTEX_WAKE | FUTEX_WAKE_BITSET => {
                let bitset = if op == FUTEX_WAKE { u32::MAX } else { val3 as u32 };
                if bitset == 0 {
                    return Step::Return(-EINVAL);
                }
                Step::Return(self.wake(addr, val, bitset) as i64)
            }
            FUTEX_REQUEUE | FUTEX_CMP_REQUEUE => {
                if op == FUTEX_CMP_REQUEUE {
                    match mmu.read_u32(addr) {
                        Err(_) => return Step::Return(-EFAULT),
                        Ok(v) if v != val3 as u32 => return Step::Return(-EAGAIN),
                        Ok(_) => {}
                    }
                }
                // the timeout argument carries the requeue limit
                let woken = self.wake(addr, val, u32::MAX);
                let mut moved = 0u32;
                for t in self.list.iter_mut() {
                    if moved == timeout as u32 {
                        break;
                    }
                    if let Some(w) = t.wait.as_mut().filter(|w| w.addr == addr) {
                        w.addr = addr2;
                        moved += 1;
                    }
                }
                Step::Return(i64::from(woken + if op == FUTEX_CMP_REQUEUE { moved } else { 0 }))
            }
            _ => Step::Return(-ENOSYS),
        }
    }

    /// Wakes up to `count` threads waiting on `addr` whose bitset overlaps `bitset`, in creation order.
    fn wake(&mut self, addr: u64, count: u32, bitset: u32) -> u32 {
        let mut woken = 0;
        for t in self.list.iter_mut() {
            if woken == count {
                break;
            }
            if t.wait
                .as_ref()
                .is_some_and(|w| w.addr == addr && w.bitset & bitset != 0)
            {
                t.wait = None;
                woken += 1;
            }
        }
        woken
    }

    fn exit_thread(&mut self, status: i32, cpu: &mut Cpu, mmu: &mut Memory) -> Step {
        let me = &self.list[self.current];
        if me.tid == PID as u32 {
            self.leader_status = Some(status);
        }
        // CLONE_CHILD_CLEARTID: zero the word and wake one waiter, as the kernel does on thread exit
        let ctid = me.clear_child_tid;
        if ctid != 0 && mmu.write_u32(ctid, 0).is_ok() {
            self.wake(ctid, 1, u32::MAX);
        }
        if self.list.len() == 1 {
            return Step::ProcessExit(self.leader_status.unwrap_or(status));
        }
        self.list.remove(self.current);
        if self.current == self.list.len() {
            self.current = 0;
        }
        // the next thread in order now sits at `current`; resume it (or report that nothing can run)
        self.resume(self.current, cpu)
    }

    /// Saves the running thread and switches to the next runnable one after it.
    fn switch_from_current(&mut self, cpu: &mut Cpu) -> Step {
        self.list[self.current].regs = Some(Box::new(cpu.regs.clone()));
        let start = (self.current + 1) % self.list.len();
        self.resume(start, cpu)
    }

    /// Runs the first runnable thread at or after `start` (wrapping). When every thread waits, the timed waits
    /// expire (there is nothing else to wait for); with no timed wait left the process is deadlocked.
    fn resume(&mut self, start: usize, cpu: &mut Cpu) -> Step {
        let n = self.list.len();
        let pick = (0..n).map(|k| (start + k) % n).find(|&i| self.list[i].wait.is_none());
        let pick = match pick {
            Some(i) => i,
            None => {
                let mut expired = None;
                for (i, t) in self.list.iter_mut().enumerate() {
                    if t.wait.as_ref().is_some_and(|w| w.timed) {
                        t.wait = None;
                        if let Some(r) = t.regs.as_mut() {
                            r.write_x(0, (-ETIMEDOUT) as u64);
                        }
                        expired.get_or_insert(i);
                    }
                }
                match expired {
                    Some(i) => i,
                    None => {
                        let waits: Vec<String> = self
                            .list
                            .iter()
                            .map(|t| format!("tid {} on {:#x}", t.tid, t.wait.as_ref().map_or(0, |w| w.addr)))
                            .collect();
                        return Step::Deadlock(format!(
                            "deadlock: every guest thread waits in futex and none can wake it ({})",
                            waits.join(", ")
                        ));
                    }
                }
            }
        };
        self.current = pick;
        if let Some(regs) = self.list[pick].regs.take() {
            cpu.regs = *regs;
        }
        // a thread switch clears the local exclusive monitor, so an interrupted LDXR/STXR pair retries
        cpu.exclusive = None;
        Step::Switched
    }
}
