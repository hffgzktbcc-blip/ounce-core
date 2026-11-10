//! Instruction trace for the hardware check (`ounce-core diverge`).
//!
//! With `OUNCE_TRACE=<file>` set, the NRO run loop records the register state before and after
//! sampled instructions that only touch registers (integer, FP and SIMD data processing). The
//! `tools/hwcheck` helper replays each record on the host CPU and reports the first record where
//! the host and the emulator disagree.
//!
//! Sampling is by opcode: the first four executions of each distinct opcode are recorded, then
//! every power-of-two occurrence. The first execution of every opcode is therefore always checked.
//!
//! Record layout (little endian, 1584 bytes): `u32 opcode, u32 0, u64 pc, State pre, State post`;
//! `State` is `u64 x[31], u64 sp, u64 v[64] (lo, hi per register), u32 nzcv, fpsr, fpcr, 0` (784 bytes).

use crate::cpu::Cpu;
use std::collections::HashMap;
use std::io::Write;

pub const STATE_BYTES: usize = 784;

/// A snapshot of the registers an instruction in the checked set can read or write.
pub struct State(pub [u8; STATE_BYTES]);

impl State {
    pub fn capture(cpu: &Cpu) -> State {
        let r = &cpu.regs;
        let mut b = [0u8; STATE_BYTES];
        for (i, x) in r.x.iter().enumerate() {
            b[i * 8..i * 8 + 8].copy_from_slice(&x.to_le_bytes());
        }
        b[248..256].copy_from_slice(&r.sp.to_le_bytes());
        for (i, v) in r.v.iter().enumerate() {
            b[256 + i * 16..256 + i * 16 + 16].copy_from_slice(&v.to_le_bytes());
        }
        b[768..772].copy_from_slice(&r.pstate.to_bits().to_le_bytes());
        b[772..776].copy_from_slice(&r.fpsr.to_le_bytes());
        b[776..780].copy_from_slice(&r.fpcr.to_le_bytes());
        State(b)
    }
}

/// Whether `raw` is a register-only data-processing instruction the host can replay in isolation:
/// no memory, branches, system registers, PC-relative addressing, stack pointer operands or
/// pointer authentication (whose keys differ per process).
pub fn eligible(raw: u32) -> bool {
    let op0 = (raw >> 25) & 0xF;
    let data_proc = matches!(op0, 0b1000 | 0b1001 | 0b0101 | 0b1101 | 0b0111 | 0b1111);
    if !data_proc {
        return false;
    }
    let (rd, rn, rm) = (raw & 0x1F, (raw >> 5) & 0x1F, (raw >> 16) & 0x1F);
    if rd == 18 || rn == 18 || rm == 18 {
        return false; // x18 is reserved by the host platform, so the replay cannot set it
    }
    let sp_class = (raw & 0x1F80_0000) == 0x1100_0000 // add/sub immediate
        || (raw & 0x1F80_0000) == 0x1200_0000 // logical immediate
        || (raw & 0x1FE0_0000) == 0x0B20_0000; // add/sub extended register
    if sp_class && (rd == 31 || rn == 31) {
        return false;
    }
    if (raw & 0x1F00_0000) == 0x1000_0000 {
        return false; // ADR / ADRP
    }
    if (raw & 0x5FE0_0000) == 0x5AC0_0000 && (raw >> 16) & 0x1F == 1 {
        return false; // PAC*/AUT*/XPAC*
    }
    true
}

pub struct Tracer {
    out: std::io::BufWriter<std::fs::File>,
    seen: HashMap<u32, u64>,
    pub records: u64,
}

impl Tracer {
    pub fn from_env() -> Option<Tracer> {
        let path = std::env::var("OUNCE_TRACE").ok().filter(|p| !p.is_empty())?;
        let file = std::fs::File::create(&path).map_err(|e| eprintln!("[Trace] cannot create {path}: {e}")).ok()?;
        Some(Tracer { out: std::io::BufWriter::new(file), seen: HashMap::new(), records: 0 })
    }

    /// Call before stepping: returns the pre-state when this execution is to be recorded.
    pub fn pre(&mut self, cpu: &Cpu, raw: u32) -> Option<State> {
        if !eligible(raw) {
            return None;
        }
        let n = self.seen.entry(raw).or_insert(0);
        *n += 1;
        (*n <= 4 || n.is_power_of_two()).then(|| State::capture(cpu))
    }

    /// Call after a step that completed normally.
    pub fn post(&mut self, cpu: &Cpu, raw: u32, pc: u64, pre: State) {
        let post = State::capture(cpu);
        let mut head = [0u8; 16];
        head[..4].copy_from_slice(&raw.to_le_bytes());
        head[8..].copy_from_slice(&pc.to_le_bytes());
        let _ = self.out.write_all(&head).and_then(|_| self.out.write_all(&pre.0)).and_then(|_| self.out.write_all(&post.0));
        self.records += 1;
    }

    pub fn finish(&mut self) {
        let _ = self.out.flush();
        eprintln!("[Trace] {} records written", self.records);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligibility_keeps_alu_and_simd_and_drops_risky_classes() {
        assert!(eligible(0x8B02_0020)); // add x0, x1, x2
        assert!(eligible(0x1E22_0820)); // fmul s0, s1, s2
        assert!(eligible(0x4E22_0C20)); // sqadd v0.16b, v1.16b, v2.16b
        assert!(!eligible(0xF940_0020)); // ldr x0, [x1]
        assert!(!eligible(0xD503_201F)); // nop (system)
        assert!(!eligible(0x9100_03FF)); // add sp, sp, #0
        assert!(!eligible(0x9000_0000)); // adrp x0, ...
        assert!(!eligible(0xDAC1_0020)); // pacia x0, x1
        assert!(!eligible(0x2A12_03FE)); // orr w30, wzr, w18
    }
}
