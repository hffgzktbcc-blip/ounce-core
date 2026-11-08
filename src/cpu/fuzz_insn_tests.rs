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

#[test]
fn integer_two_reg_misc_classes() {
    // suqadd v0.16b: 127 + 1 saturates to 0x7f (QC); -1 + 3 = 2
    let c = run1(0x4e20_3820, &[(0, 0xff7f), (1, 0x0301)]);
    assert_eq!(c.regs.v[0], 0x027f);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // usqadd v2.8h: 5 + (-16) clamps to 0 (QC); 1 + 2 = 3
    let c = run1(0x6e60_3862, &[(2, 5 | 1 << 16), (3, 0xfff0 | 2 << 16)]);
    assert_eq!(c.regs.v[2], 0x0003_0000);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // cls v4.4s: 0x00ffffff has 7 redundant sign bits, 0xffffffff has 31
    let c = run1(0x4ea0_48a4, &[(5, 0x00ff_ffff | 0xffff_ffff << 32)]);
    assert_eq!(c.regs.v[4], 7 | 31 << 32 | 31u128 << 64 | 31u128 << 96); // the zero upper lanes give 31 too
    // clz v6.8b: 0x10 -> 3, 0 -> 8 (the 8b form zeroes the upper half)
    let c = run1(0x2e20_48e6, &[(7, 0x0010)]);
    assert_eq!(c.regs.v[6], 0x0808_0808_0808_0803); // zero lanes give 8
    // rbit v8.16b: 0x01 -> 0x80, 0x0f -> 0xf0
    let c = run1(0x6e60_5928, &[(9, 0x0f01)]);
    assert_eq!(c.regs.v[8], 0xf080);
    // sqabs v10.2d: i64::MIN saturates (QC), -5 -> 5
    let c = run1(0x4ee0_796a, &[(11, 0x8000_0000_0000_0000 | (-5i64 as u64 as u128) << 64)]);
    assert_eq!(c.regs.v[10], 0x7fff_ffff_ffff_ffff | 5u128 << 64);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // sqneg v12.4s: 5 -> -5, no saturation
    let c = run1(0x6ea0_79ac, &[(13, 5)]);
    assert_eq!((c.regs.v[12], c.regs.fpsr & FPSR_QC), (0xffff_fffb, 0));
    // shll v14.8h, v15.8b, #8
    let c = run1(0x2e21_39ee, &[(15, 0x0281)]);
    assert_eq!(c.regs.v[14], 0x0200_8100);
    // sqabs b16, b17: -128 -> 127 (QC), upper bits of v16 cleared
    let c = run1(0x5e20_7a30, &[(16, u128::MAX), (17, 0x80)]);
    assert_eq!(c.regs.v[16], 0x7f);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
}

#[test]
fn by_element_dot_and_fmulx_classes() {
    // mul v0.4s, v1.4s, v2.s[3]
    let c = run1(0x4fa2_8820, &[(1, 1 | 2 << 32 | 3u128 << 64 | 4u128 << 96), (2, 5u128 << 96)]);
    assert_eq!(c.regs.v[0], 5 | 10 << 32 | 15u128 << 64 | 20u128 << 96);
    // mla v3.8h, v4.8h, v5.h[7]: 1 + 2*3
    let c = run1(0x6f75_0883, &[(3, 1), (4, 2), (5, 3u128 << 112)]);
    assert_eq!(c.regs.v[3], 7);
    // sqdmulh v6.4s, v7.4s, v8.s[1]: min * min saturates (QC)
    let c = run1(0x4fa8_c0e6, &[(7, 0x8000_0000), (8, 0x8000_0000u128 << 32)]);
    assert_eq!(c.regs.v[6], 0x7fff_ffff);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // smull v9.4s, v10.4h, v11.h[2]: -2*5, 3*5
    let c = run1(0x0f6b_a149, &[(10, 0xfffe | 3 << 16), (11, 5u128 << 32)]);
    assert_eq!(c.regs.v[9], 0xffff_fff6 | 15 << 32);
    // umull2 v12.2d, v13.4s, v14.s[1]: upper half of v13 only
    let c = run1(0x6fae_a1ac, &[(13, 0x8000_0000u128 << 64 | 2u128 << 96), (14, 0x10u128 << 32)]);
    assert_eq!(c.regs.v[12], 0x8_0000_0000 | 0x20u128 << 64);
    // sqdmull s15, h16, v7.h[1]: 0x8000 * 0x8000 * 2 saturates (QC)
    let c = run1(0x5f57_b20f, &[(16, 0x8000), (7, 0x8000u128 << 16), (15, u128::MAX)]);
    assert_eq!(c.regs.v[15], 0x7fff_ffff);
    assert_ne!(c.regs.fpsr & FPSR_QC, 0);
    // sdot v18.4s, v19.16b, v20.4b[1]: 5 + (1+2+3+4); lane1 = -1
    let c = run1(0x4fb4_e272, &[(18, 5), (19, 0x0101_0101 | 0xffu128 << 32), (20, 0x0403_0201u128 << 32)]);
    assert_eq!(c.regs.v[18], 15 | 0xffff_ffffu128 << 32);
    // udot v21.2s, v22.8b, v23.4b[0]: 255*1, 4 bytes of 1 against 1,2,3,4 = 10
    let c = run1(0x2f97_e2d5, &[(22, 0xff | 0x0101_0101u128 << 32), (23, 0x0403_0201)]);
    assert_eq!(c.regs.v[21], 255 | 10 << 32);
    // sdot v24.4s, v25.16b, v26.16b (vector form)
    let c = run1(0x4e9a_9738, &[(25, 2), (26, 3)]);
    assert_eq!(c.regs.v[24], 6);
    // fmulx v27.4s: 0 * inf = +2.0, -0 * inf = -2.0
    let c = run1(0x4e3d_df9b, &[(28, 0x8000_0000u128 << 32), (29, 0x7f80_0000 | 0x7f80_0000u128 << 32)]);
    assert_eq!(c.regs.v[27], 0x4000_0000 | 0xc000_0000u128 << 32);
    // fmulx v30.2d, v31.2d, v1.d[1]: 1.5 * inf = inf, 0 * inf = 2.0
    let c = run1(0x6fc1_9bfe, &[(31, 0x3ff8_0000_0000_0000), (1, 0x7ff0_0000_0000_0000u128 << 64)]);
    assert_eq!(c.regs.v[30], 0x7ff0_0000_0000_0000 | 0x4000_0000_0000_0000u128 << 64);
}
