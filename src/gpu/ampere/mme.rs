//! Macro method expander (MME) for MAXWELL_B: the Fermi-style macro engine deko3d and nouveau use
//! to expand one call into many method writes.
//!
//! Sources: envytools `docs/hw/graph/macro.rst` and Mesa `src/nouveau/mme/mme_fermi*.{h,c}`
//! (instruction encoding and the simulator's semantics). Upload and call methods are open-gpu-doc
//! `clb197.h`: `LOAD_MME_INSTRUCTION_RAM_POINTER` / `_RAM`, `LOAD_MME_START_ADDRESS_RAM_POINTER` /
//! `_RAM`, `CALL_MME_MACRO(j)` (0x3800 + 8j) and `CALL_MME_DATA(j)` (+4).
//!
//! Machine: eight 32-bit registers (`r0` reads as zero), a carry flag, a method address register
//! (`MADDR`: method dword index 11:0, increment 17:12) and a FIFO of call parameters. `r1` starts
//! with the first parameter; `LOAD*` assignments fetch the next one. Each instruction is one word:
//!
//! | bits  | field                                                        |
//! |-------|--------------------------------------------------------------|
//! | 3:0   | op: 0 ALU, 1 ADDI, 2 MERGE, 3 BFE_LSL_IMM, 4 BFE_LSL_REG, 5 STATE, 7 BRANCH |
//! | 6:4   | assignment (branch: 4 = branch if not zero, 5 = no delay slot) |
//! | 7     | exit after the next instruction                              |
//! | 10:8  | destination register                                         |
//! | 13:11 | source A                                                     |
//! | 16:14 | source B                                                     |
//! | 31:14 | signed immediate                                             |
//! | 21:17 | ALU op, or bitfield source bit                               |
//! | 26:22 | bitfield size                                                |
//! | 31:27 | bitfield destination bit                                     |
//!
//! Assignments: 0 `dst = fetch`, 1 `dst = result`, 2 `dst = result; maddr = result`, 3 `dst =
//! fetch; emit result`, 4 `dst = result; emit result`, 5 `dst = fetch; maddr = result`, 6 `dst =
//! result; maddr = result; emit fetch`, 7 `dst = result; maddr = result; emit (result >> 12) &
//! 0x3f`. An emit writes the value to method `MADDR` and advances it by the increment.
//!
//! AMPERE_B's macro engine uses a different (TU104) instruction format and is not modelled.

/// Instruction RAM words. The size is not in the public headers; addresses wrap.
pub const CODE_WORDS: usize = 0x2000;
/// `CALL_MME_MACRO(j)`, j = 0..127.
pub const MACRO_COUNT: usize = 128;
/// Upper bound on instructions per call, so a looping macro cannot hang the channel.
pub const STEP_LIMIT: usize = 1 << 20;

/// What a running macro reads and writes on its engine.
pub trait MmeHost {
    /// Shadow value of method dword `index`.
    fn state(&self, index: u32) -> u32;
    /// A method write from the macro: dword `index`, `data`.
    fn emit(&mut self, index: u32, data: u32);
}

/// Macro RAM and upload pointers for one engine.
#[derive(Debug, Clone)]
pub struct Mme {
    code: Vec<u32>,
    start: [u32; MACRO_COUNT],
    code_ptr: u32,
    start_ptr: u32,
}

impl Default for Mme {
    fn default() -> Self {
        Mme { code: vec![0; CODE_WORDS], start: [0; MACRO_COUNT], code_ptr: 0, start_ptr: 0 }
    }
}

impl Mme {
    /// A stand-in with no instruction RAM, held by the engine while a macro runs. Must not be
    /// loaded or run.
    pub(super) fn detached() -> Self {
        Mme { code: Vec::new(), start: [0; MACRO_COUNT], code_ptr: 0, start_ptr: 0 }
    }

    pub fn set_code_pointer(&mut self, v: u32) {
        self.code_ptr = v;
    }

    /// `LOAD_MME_INSTRUCTION_RAM`: stores a word and advances the pointer.
    pub fn load_code(&mut self, v: u32) {
        self.code[self.code_ptr as usize % CODE_WORDS] = v;
        self.code_ptr = self.code_ptr.wrapping_add(1);
    }

    pub fn set_start_pointer(&mut self, v: u32) {
        self.start_ptr = v;
    }

    /// `LOAD_MME_START_ADDRESS_RAM`: stores a macro's start and advances the pointer.
    pub fn load_start(&mut self, v: u32) {
        self.start[self.start_ptr as usize % MACRO_COUNT] = v;
        self.start_ptr = self.start_ptr.wrapping_add(1);
    }

    /// Runs macro `index` with `params` (the first is the `CALL_MME_MACRO` word).
    pub fn run(&self, index: usize, params: &[u32], host: &mut dyn MmeHost) -> Result<(), &'static str> {
        let mut regs = [0u32; 8];
        regs[1] = params.first().copied().unwrap_or(0);
        // A LOAD past the end reads 0; the run then reports it (hardware would wait for data).
        let mut next_param = 1;
        let fetch = |n: &mut usize| {
            let v = params.get(*n).copied().unwrap_or(0);
            *n += 1;
            v
        };
        let mut carry = false;
        let mut maddr = 0u32;
        let emit = |maddr: &mut u32, data: u32, host: &mut dyn MmeHost| {
            host.emit(*maddr & 0xfff, data);
            let inc = (*maddr >> 12) & 0x3f;
            *maddr = (*maddr & !0xfff) | ((*maddr).wrapping_add(inc) & 0xfff);
        };

        let mut pc = self.start[index % MACRO_COUNT] as usize;
        // Branch taken with a delay slot: jump here after the next instruction.
        let mut delayed: Option<usize> = None;
        // Exit flag seen: stop after the next instruction.
        let mut exiting = false;
        for _ in 0..STEP_LIMIT {
            let insn = self.code[pc % CODE_WORDS];
            let mut next = pc + 1;
            let in_delay_slot = delayed.is_some();
            if let Some(target) = delayed.take() {
                next = target;
            }
            let field = |lo: u32, bits: u32| (insn >> lo) & ((1 << bits) - 1);
            let (op, assign, dst) = (field(0, 4), field(4, 3), field(8, 3) as usize);
            let (a, b) = (regs[field(11, 3) as usize], regs[field(14, 3) as usize]);
            let imm = (insn as i32) >> 14;
            let (src_bit, size, dst_bit) = (field(17, 5), field(22, 5), field(27, 5));
            let mask = if size >= 32 { u32::MAX } else { (1u32 << size) - 1 };
            let is_exit = insn & 0x80 != 0;

            let mut taken = false;
            if op == 7 {
                // BRANCH: assignment bit 0 = branch if not zero, bit 1 = no delay slot.
                if in_delay_slot {
                    return Err("macro branch in a delay slot");
                }
                taken = (a != 0) == (assign & 1 == 1);
                if taken {
                    let target = (pc as i64 + imm as i64).rem_euclid(CODE_WORDS as i64) as usize;
                    if assign & 2 == 2 {
                        next = target;
                    } else {
                        delayed = Some(target);
                    }
                }
            } else {
                let result = match op {
                    0 => {
                        let (r, c) = match field(17, 5) {
                            0 => a.overflowing_add(b),
                            1 => {
                                let s = a as u64 + b as u64 + carry as u64;
                                (s as u32, s >> 32 != 0)
                            }
                            2 => {
                                let (r, borrow) = a.overflowing_sub(b);
                                (r, !borrow)
                            }
                            3 => {
                                let s = (a as u64).wrapping_sub(b as u64).wrapping_sub(!carry as u64);
                                (s as u32, s >> 32 == 0)
                            }
                            8 => (a ^ b, carry),
                            9 => (a | b, carry),
                            10 => (a & b, carry),
                            11 => (a & !b, carry),
                            12 => (!(a & b), carry),
                            _ => return Err("macro ALU op"),
                        };
                        carry = c;
                        r
                    }
                    1 => a.wrapping_add(imm as u32),
                    2 => (a & !(mask << dst_bit)) | (((b >> src_bit) & mask) << dst_bit),
                    3 => (b.checked_shr(a).unwrap_or(0) & mask) << dst_bit,
                    4 => ((b >> src_bit) & mask).checked_shl(a).unwrap_or(0),
                    5 => host.state(a.wrapping_add(imm as u32) & 0xfff),
                    _ => return Err("macro opcode"),
                };
                let value = match assign {
                    0 => fetch(&mut next_param),
                    1 => result,
                    2 => {
                        maddr = result;
                        result
                    }
                    3 => {
                        let v = fetch(&mut next_param);
                        emit(&mut maddr, result, host);
                        v
                    }
                    4 => {
                        emit(&mut maddr, result, host);
                        result
                    }
                    5 => {
                        maddr = result;
                        fetch(&mut next_param)
                    }
                    6 => {
                        maddr = result;
                        let p = fetch(&mut next_param);
                        emit(&mut maddr, p, host);
                        result
                    }
                    _ => {
                        maddr = result;
                        emit(&mut maddr, (result >> 12) & 0x3f, host);
                        result
                    }
                };
                if dst != 0 {
                    regs[dst] = value;
                }
            }

            // A taken branch jumps to target and ignores an EXIT bit on the branch.
            // When a conditional branch with EXIT is NOT taken, execution exits after the delay slot
            // (e.g. `*bnz r2 .loop` in deko3d BindColorWriteMasks or `*bnz r7 .cancelExit`).
            let branch_exit = op == 7 && !taken && is_exit;
            let normal_exit = op != 7 && is_exit;

            if exiting {
                return if next_param > params.len() { Err("macro read past its parameters") } else { Ok(()) };
            }
            exiting = normal_exit || branch_exit;
            pc = next;
        }
        Err("macro step limit")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assembler helpers in the field layout above.
    fn alu(op: u32, assign: u32, dst: u32, a: u32, b: u32) -> u32 {
        (op << 17) | (b << 14) | (a << 11) | (dst << 8) | (assign << 4)
    }
    fn addi(assign: u32, dst: u32, a: u32, imm: i32) -> u32 {
        ((imm as u32) << 14) | (a << 11) | (dst << 8) | (assign << 4) | 1
    }
    fn state(assign: u32, dst: u32, a: u32, imm: i32) -> u32 {
        ((imm as u32) << 14) | (a << 11) | (dst << 8) | (assign << 4) | 5
    }
    fn branch(not_zero: bool, no_delay: bool, a: u32, imm: i32) -> u32 {
        ((imm as u32) << 14) | (a << 11) | ((no_delay as u32) << 5) | ((not_zero as u32) << 4) | 7
    }
    const EXIT: u32 = 0x80;

    #[derive(Default)]
    struct Host {
        regs: Vec<u32>,
        writes: Vec<(u32, u32)>,
    }

    impl MmeHost for Host {
        fn state(&self, index: u32) -> u32 {
            self.regs.get(index as usize).copied().unwrap_or(0)
        }
        fn emit(&mut self, index: u32, data: u32) {
            self.writes.push((index, data));
            if let Some(r) = self.regs.get_mut(index as usize) {
                *r = data;
            }
        }
    }

    fn load(code: &[u32]) -> Mme {
        let mut mme = Mme::default();
        mme.set_code_pointer(0);
        code.iter().for_each(|&w| mme.load_code(w));
        mme.set_start_pointer(0);
        mme.load_start(0);
        mme
    }

    #[test]
    fn emits_parameters_to_consecutive_methods() {
        // maddr = r1 (method 0x100, increment 1); emit two fetched parameters.
        let code = [
            alu(0, 2, 0, 1, 0),           // maddr = r1 + r0
            alu(0, 3, 2, 0, 0),           // r2 = fetch; emit r0 + r0 = 0
            alu(0, 4, 0, 2, 0) | EXIT,    // emit r2
            0,                            // delay slot: dst = fetch into r0 (discarded)
        ];
        let mut host = Host::default();
        load(&code).run(0, &[0x100 | (1 << 12), 7, 9], &mut host).unwrap();
        assert_eq!(host.writes, vec![(0x100, 0), (0x101, 7)]);
        // Without the third parameter the delay-slot LOAD reads past the end: reported, writes kept.
        let mut host = Host::default();
        assert_eq!(load(&code).run(0, &[0x100 | (1 << 12), 7], &mut host), Err("macro read past its parameters"));
        assert_eq!(host.writes, vec![(0x100, 0), (0x101, 7)]);
    }

    #[test]
    fn loop_counts_down_with_a_delay_slot() {
        // r2 = r1 (count); loop: r3 += 1; r2 -= 1; branch back while r2 != 0; then emit r3.
        let code = [
            alu(0, 1, 2, 1, 0),                 // r2 = r1
            addi(1, 3, 3, 1),                   // r3 += 1
            addi(1, 2, 2, -1),                  // r2 -= 1
            branch(true, false, 2, -2),         // if r2 != 0 goto 1 (after the delay slot)
            addi(1, 4, 4, 1),                   // delay slot: r4 += 1 (runs every pass)
            addi(2, 0, 0, 0x200),               // maddr = 0x200, increment 0
            alu(0, 4, 0, 3, 0) | EXIT,          // emit r3
            alu(0, 4, 0, 4, 0),                 // delay slot of exit: emit r4 to 0x200 again
        ];
        let mut host = Host::default();
        load(&code).run(0, &[5], &mut host).unwrap();
        assert_eq!(host.writes, vec![(0x200, 5), (0x200, 5)]);
    }

    #[test]
    fn state_reads_and_bitfields() {
        let mut host = Host { regs: vec![0; 0x1000], ..Default::default() };
        host.regs[0x123] = 0xabcd_1234;
        let code = [
            state(1, 2, 0, 0x123),                                    // r2 = state[0x123]
            // MERGE r3 = (r0 & ~(0xff << 8)) | (((r2 >> 16) & 0xff) << 8) = 0xcd00
            (8 << 27) | (8 << 22) | (16 << 17) | (2 << 14) | (3 << 8) | (1 << 4) | 2,
            addi(2, 0, 0, 0x300),                                     // maddr = 0x300
            alu(0, 4, 0, 3, 0) | EXIT,                                // emit r3
            alu(10, 4, 0, 2, 2),                                      // emit r2 & r2
        ];
        load(&code).run(0, &[0], &mut host).unwrap();
        assert_eq!(host.writes, vec![(0x300, 0xcd00), (0x300, 0xabcd_1234)]);
    }

    #[test]
    fn carry_chains_through_addc() {
        // r2 = 0xffffffff + 1 (carry out), r3 = 0 + 0 + carry = 1.
        let code = [
            addi(1, 2, 0, -1),           // r2 = 0xffffffff
            addi(1, 4, 0, 1),            // r4 = 1
            alu(0, 1, 2, 2, 4),          // r2 = r2 + r4, carry = 1
            alu(1, 1, 3, 0, 0),          // r3 = 0 + 0 + carry
            addi(2, 0, 0, 0x10),         // maddr = 0x10
            alu(0, 4, 0, 3, 0) | EXIT,   // emit r3
            0x0000_0011,                 // delay slot: ADDI with every field zero (r0 = 0)
        ];
        let mut host = Host::default();
        load(&code).run(0, &[0], &mut host).unwrap();
        assert_eq!(host.writes, vec![(0x10, 1)]);
    }

    #[test]
    fn branch_delay_slot_ignores_exit_flag() {
        // EXIT on a delayed branch is ignored; run the delay slot and continue at the target.
        let code = [
            branch(false, false, 0, 2) | EXIT, // r0 == 0, branch to instruction 2
            addi(1, 4, 4, 5),                  // delay slot: r4 = 5
            addi(2, 0, 0, 0x200),              // target: maddr = 0x200
            alu(0, 4, 0, 4, 0) | EXIT,          // emit r4
            addi(1, 0, 0, 0),                  // exit delay slot
        ];
        let mut host = Host::default();
        load(&code).run(0, &[0], &mut host).unwrap();
        assert_eq!(host.writes, vec![(0x200, 5)]);
    }

    #[test]
    fn branch_in_a_delay_slot_is_rejected() {
        let code = [branch(false, false, 0, 2), branch(false, true, 0, 0), 0];
        let mut host = Host::default();
        assert_eq!(load(&code).run(0, &[0], &mut host), Err("macro branch in a delay slot"));
    }

    #[test]
    fn runaway_macro_stops_at_the_step_limit() {
        let code = [branch(false, true, 0, 0)]; // branch to itself while r0 == 0, no delay slot
        let mut host = Host::default();
        assert_eq!(load(&code).run(0, &[0], &mut host), Err("macro step limit"));
    }
}
