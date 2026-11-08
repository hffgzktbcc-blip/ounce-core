//! Regression tests for instruction classes added from instruction-fuzzer coverage gaps
//! (`docs/verify/insn-fuzzer`). Encodings were assembled with clang and checked with objdump; expected
//! values are hand-computed from the Arm ARM pseudocode and agree with the Apple Silicon results the
//! fuzzer compared against.

use super::registers::FPSR_QC;
use super::{Cpu, StepResult};
use crate::mmu::{Memory, Permission};

fn run1(word: u32, set: &[(usize, u128)]) -> Cpu {
    let mut mmu = Memory::new();
    let mut cpu = Cpu::new();
    mmu.map_range(0x1000, 0x1000, Permission::READ_WRITE_EXECUTE);
    mmu.write_u32(0x1000, word).unwrap();
    cpu.regs.pc = 0x1000;
    for &(r, v) in set {
        cpu.regs.v[r] = v;
    }
    assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)));
    cpu
}

#[test]
fn integer_three_same_classes() {
    // sqadd v0.16b, v1.16b, v2.16b: lane0 0x7f+1 saturates to 0x7f (QC), lane1 0x10+0x20
    let c = run1(0x4e22_0c20, &[(1, 0x107f), (2, 0x2001)]);
    assert_eq!(c.regs.v[0], 0x307f);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // sqadd without saturation leaves QC clear
    let c = run1(0x4e22_0c20, &[(1, 0x10), (2, 0x20)]);
    assert_eq!((c.regs.v[0], c.regs.fpsr & FPSR_QC), (0x30, 0));
    // uqsub v3.8h, v4.8h, v5.8h: 5-9 -> 0, 0x100-1 = 0xff
    let c = run1(0x6e65_2c83, &[(4, 5 | 0x100 << 16), (5, 9 | 1 << 16)]);
    assert_eq!(c.regs.v[3], 0x00ff_0000);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // sqrdmulh v6.4s: min*min saturates to 0x7fffffff, 0x40000000^2 -> 0x20000000
    let c = run1(0x6ea8_b4e6, &[(7, 0x8000_0000 | 0x4000_0000 << 32), (8, 0x8000_0000 | 0x4000_0000 << 32)]);
    assert_eq!(c.regs.v[6], 0x7fff_ffff | 0x2000_0000 << 32);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // srshl v9.2d: 5 shifted by -1 rounds to 3; 3 shifted left by 4 is 48
    let c = run1(0x4eeb_5549, &[(10, 5 | 3 << 64), (11, 0xff | 4 << 64)]);
    assert_eq!(c.regs.v[9], 3 | 48 << 64);
    // mla v12.4s: 10 + 3*4
    let c = run1(0x4eae_95ac, &[(12, 10), (13, 3), (14, 4)]);
    assert_eq!(c.regs.v[12], 22);
    // pmul v15.8b: carry-less 3*3 = 5
    let c = run1(0x2e31_9e0f, &[(16, 3), (17, 3)]);
    assert_eq!(c.regs.v[15], 5);
    // uhadd v18.16b: (255+255)>>1 = 255, (1+2)>>1 = 1
    let c = run1(0x6e34_0672, &[(19, 0x01ff), (20, 0x02ff)]);
    assert_eq!(c.regs.v[18], 0x01ff);
    // scalar sqadd b0, b1, b2 zeroes the rest of v0
    let c = run1(0x5e22_0c20, &[(0, u128::MAX), (1, 0x7f), (2, 1)]);
    assert_eq!(c.regs.v[0], 0x7f);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
}
