//! Vector fixed-point `SCVTF`/`UCVTF`/`FCVTZS`/`FCVTZU` and `FJCVTZS` (#469, #488).
//!
//! Every expected value below is derived by hand from the Arm ARM pseudocode (`FixedToFP`, `FPToFixed`,
//! `FPToFixedJS`) for the stated input, not by running the engines. Words are the ones the fp-workload names
//! (`0x4f39e402` scvtf v2.4s, v0.4s, #7 and so on). No ARM64 hardware was used.
use super::*;
use crate::mmu::{Memory, Permission};

const CODE: u64 = 0x1000;
const IOC: u32 = 1;
const IXC: u32 = 1 << 4;
const IDC: u32 = 1 << 7;
const FZ: u32 = 1 << 24;
const RP: u32 = 1 << 22; // FPCR.RMode = 01, round towards plus infinity
const RM: u32 = 2 << 22; // round towards minus infinity
const RZ: u32 = 3 << 22; // round towards zero

fn machine(words: &[u32]) -> (Cpu, Memory) {
    let mut mmu = Memory::new();
    mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
    for (i, w) in words.iter().enumerate() {
        mmu.write_u32(CODE + 4 * i as u64, *w).unwrap();
    }
    let mut cpu = Cpu::new();
    cpu.regs.pc = CODE;
    (cpu, mmu)
}

/// Runs one instruction with v0 = `src`, v2 = `dirty` (so an unwritten upper half shows) and FPCR = `fpcr`.
fn vec_op(word: u32, src: u128, fpcr: u32) -> (u128, u32) {
    let (mut cpu, mut mmu) = machine(&[word]);
    cpu.regs.v[0] = src;
    cpu.regs.v[2] = u128::MAX;
    cpu.regs.fpcr = fpcr;
    assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)), "{word:#010x}");
    assert_eq!(cpu.regs.pc, CODE + 4);
    (cpu.regs.v[2], cpu.regs.fpsr)
}

fn s4(l: [u32; 4]) -> u128 {
    l.iter().enumerate().fold(0, |a, (i, &x)| a | (x as u128) << (32 * i))
}

fn d2(l: [u64; 2]) -> u128 {
    l[0] as u128 | (l[1] as u128) << 64
}

// scvtf/ucvtf/fcvtzs/fcvtzu v2.<T>, v0.<T>, #fbits
const SCVTF_4S_7: u32 = 0x4f39_e402;
const UCVTF_4S_7: u32 = 0x6f39_e402;
const FCVTZS_4S_7: u32 = 0x4f39_fc02;
const FCVTZU_4S_7: u32 = 0x6f39_fc02;
const SCVTF_2D_20: u32 = 0x4f6c_e402;
const UCVTF_2D_20: u32 = 0x6f6c_e402;
const FCVTZS_2D_20: u32 = 0x4f6c_fc02;
const FCVTZU_2D_20: u32 = 0x6f6c_fc02;
const SCVTF_2S_7: u32 = 0x0f39_e402;
const FCVTZS_2S_7: u32 = 0x0f39_fc02;

#[test]
fn decode_fixed_point_vector_converts() {
    use instructions::SimdOp2::*;
    let want = |op, esize, fbits, q| Instruction::SimdFixedConv {
        rd: 2,
        rn: 0,
        op,
        esize,
        fbits,
        q,
    };
    // immh:immb = 0111001 (57) -> 32-bit lanes, fbits = 64 - 57 = 7; 1101100 (108) -> 64-bit, fbits = 128 - 108 = 20
    assert_eq!(decoder::decode(SCVTF_4S_7), want(Scvtf, 32, 7, true));
    assert_eq!(decoder::decode(UCVTF_4S_7), want(Ucvtf, 32, 7, true));
    assert_eq!(decoder::decode(FCVTZS_4S_7), want(Fcvtzs, 32, 7, true));
    assert_eq!(decoder::decode(FCVTZU_4S_7), want(Fcvtzu, 32, 7, true));
    assert_eq!(decoder::decode(SCVTF_2D_20), want(Scvtf, 64, 20, true));
    assert_eq!(decoder::decode(FCVTZS_2D_20), want(Fcvtzs, 64, 20, true));
    assert_eq!(decoder::decode(SCVTF_2S_7), want(Scvtf, 32, 7, false));
    assert_eq!(decoder::decode(FCVTZS_2S_7), want(Fcvtzs, 32, 7, false));
    // extremes: immh:immb = 0100000 (32) is fbits 32; 0111111 (63) is fbits 1; 1000000 (64) is fbits 64; 1111111 is fbits 1
    assert_eq!(decoder::decode(0x4f20_e402), want(Scvtf, 32, 32, true));
    assert_eq!(decoder::decode(0x4f3f_e402), want(Scvtf, 32, 1, true));
    assert_eq!(decoder::decode(0x4f40_e402), want(Scvtf, 64, 64, true));
    assert_eq!(decoder::decode(0x4f7f_e402), want(Scvtf, 64, 1, true));
    // reserved or not decoded: 64-bit lanes need Q, immh = 0001 is reserved, immh = 001x is half precision
    for w in [
        0x0f6c_e402u32,
        0x0f6c_fc02,
        0x4f09_e402,
        0x4f19_e402,
        0x4f11_e402,
        0x4f19_fc02,
    ] {
        assert!(
            matches!(decoder::decode(w), Instruction::Unimplemented { .. }),
            "{w:#010x} {:?}",
            decoder::decode(w)
        );
    }
}

#[test]
fn scvtf_vector_fixed_4s_scales_by_2_pow_minus_fbits() {
    // fbits 7: 128 -> 1.0, -128 -> -1.0, 1 -> 2^-7, i32::MAX = 2^31 - 1 -> 2^24 - 2^-7, which rounds to 2^24 (ulp 1)
    let n = s4([128, (-128i32) as u32, 1, 0x7fff_ffff]);
    let (r, fpsr) = vec_op(SCVTF_4S_7, n, 0);
    assert_eq!(r, s4([0x3f80_0000, 0xbf80_0000, 0x3c00_0000, 0x4b80_0000]));
    assert_eq!(fpsr, IXC);
    // i32::MIN / 2^7 = -2^24 exactly, 0 -> +0.0, 387 / 128 = 3.0234375, -1 / 128 = -2^-7
    let n = s4([0x8000_0000, 0, 387, 0xffff_ffff]);
    let (r, fpsr) = vec_op(SCVTF_4S_7, n, 0);
    // 387/128 = 3.0234375 = 1.51171875 * 2: mantissa .51171875 * 2^23 = 131 * 2^15 = 0x418000 -> 0x40000000 | 0x418000
    // -1 / 128 = -2^-7 = 0xbc000000
    assert_eq!(r, s4([0xcb80_0000, 0, 0x4041_8000, 0xbc00_0000]));
    assert_eq!(fpsr, 0);
}

#[test]
fn scvtf_vector_fixed_rounds_under_fpcr_rmode() {
    // 16777217 / 128 = 2^17 + 2^-7 is exactly half an ulp (2^-6) above 2^17: ties-to-even gives 2^17
    let n = s4([0x0100_0001, 0, 0, 0]);
    let low = 0x4800_0000; // 2^17
    let high = 0x4800_0001; // 2^17 + 2^-6
    for (fpcr, want) in [(0, low), (RP, high), (RM, low), (RZ, low)] {
        let (r, fpsr) = vec_op(SCVTF_4S_7, n, fpcr);
        assert_eq!(r, s4([want, 0, 0, 0]), "fpcr {fpcr:#x}");
        assert_eq!(fpsr, IXC, "fpcr {fpcr:#x}");
    }
    // the negative value rounds the other way under RP / RM
    let n = s4([(-0x0100_0001i32) as u32, 0, 0, 0]);
    for (fpcr, want) in [
        (0, 0xc800_0000u32),
        (RP, 0xc800_0000),
        (RM, 0xc800_0001),
        (RZ, 0xc800_0000),
    ] {
        let (r, _) = vec_op(SCVTF_4S_7, n, fpcr);
        assert_eq!(r, s4([want, 0, 0, 0]), "fpcr {fpcr:#x}");
    }
}

#[test]
fn ucvtf_vector_fixed_treats_lanes_as_unsigned() {
    // 0x80000000 / 2^7 = 2^24; 0xffffffff / 2^7 = 2^25 - 2^-7, which rounds to 2^25 (ulp 2); 128 -> 1.0
    let (r, fpsr) = vec_op(UCVTF_4S_7, s4([0x8000_0000, 0xffff_ffff, 128, 0]), 0);
    assert_eq!(r, s4([0x4b80_0000, 0x4c00_0000, 0x3f80_0000, 0]));
    assert_eq!(fpsr, IXC);
    // .2d, fbits 20: 2^20 -> 1.0; u64::MAX / 2^20 = 2^44 - 2^-20 rounds to 2^44 = 0x42b0...
    let (r, fpsr) = vec_op(UCVTF_2D_20, d2([1 << 20, u64::MAX]), 0);
    assert_eq!(r, d2([0x3ff0_0000_0000_0000, 0x42b0_0000_0000_0000]));
    assert_eq!(fpsr, IXC);
}

#[test]
fn scvtf_vector_fixed_2d() {
    // fbits 20: 2^20 -> 1.0, -2^20 -> -1.0
    let (r, fpsr) = vec_op(SCVTF_2D_20, d2([1 << 20, (-(1i64 << 20)) as u64]), 0);
    assert_eq!(r, d2([0x3ff0_0000_0000_0000, 0xbff0_0000_0000_0000]));
    assert_eq!(fpsr, 0);
    // 2^62 / 2^20 = 2^42 = 0x429 << 52; i64::MIN / 2^20 = -2^43 = 0xc2a << 52
    let (r, fpsr) = vec_op(SCVTF_2D_20, d2([1 << 62, i64::MIN as u64]), 0);
    assert_eq!(r, d2([0x4290_0000_0000_0000, 0xc2a0_0000_0000_0000]));
    assert_eq!(fpsr, 0);
    // 2^53 + 1 is not representable; /2^20 keeps the same relative error: (2^53 + 1) / 2^20 = 2^33 + 2^-20, which
    // needs 54 significant bits, so it is half an ulp above 2^33 and ties to even
    let (r, fpsr) = vec_op(SCVTF_2D_20, d2([(1 << 53) + 1, 0]), 0);
    assert_eq!(r, d2([0x4200_0000_0000_0000, 0])); // 2^33 = (33 + 1023) << 52 = 0x420
    assert_eq!(fpsr, IXC);
}

#[test]
fn scvtf_vector_fixed_two_lane_form_zeroes_upper_half() {
    // .2s: only lanes 0 and 1 are read and written, v2 bits 64..127 become zero
    let (r, _) = vec_op(SCVTF_2S_7, s4([128, (-128i32) as u32, 0x7fff_ffff, 0x7fff_ffff]), 0);
    assert_eq!(r, s4([0x3f80_0000, 0xbf80_0000, 0, 0]));
}

#[test]
fn fcvtzs_vector_fixed_4s_scales_truncates_and_saturates() {
    // fbits 7: 1.5 -> 192, -2.5 -> -320, 1 + 2^-8 -> 128.5 -> 128 (inexact), 2^24 * 2^7 = 2^31 -> i32::MAX (invalid)
    let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([0x3fc0_0000, 0xc020_0000, 0x3f80_8000, 0x4b80_0000]), 0);
    assert_eq!(r, s4([192, (-320i32) as u32, 128, 0x7fff_ffff]));
    assert_eq!(fpsr, IOC | IXC);
    // exact conversions raise nothing: 1.0 -> 128, -1.0 -> -128, -2^24 * 2^7 = -2^31 = i32::MIN is in range
    let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([0x3f80_0000, 0xbf80_0000, 0xcb80_0000, 0]), 0);
    assert_eq!(r, s4([128, (-128i32) as u32, 0x8000_0000, 0]));
    assert_eq!(fpsr, 0);
}

#[test]
fn fcvtzs_vector_fixed_rounds_toward_zero_for_both_signs() {
    // 2^-8 * 2^7 = 0.5 -> 0; -0.5 -> 0 (not -1); -(1 + 2^-8) * 128 = -128.5 -> -128. All inexact, none invalid.
    let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([0x3b80_0000, 0xbb80_0000, 0xbf80_8000, 0]), 0);
    assert_eq!(r, s4([0, 0, (-128i32) as u32, 0]));
    assert_eq!(fpsr, IXC);
    // the FPCR rounding mode does not affect the Z forms
    for fpcr in [RP, RM, RZ] {
        let (r, _) = vec_op(FCVTZS_4S_7, s4([0x3b80_0000, 0xbb80_0000, 0xbf80_8000, 0]), fpcr);
        assert_eq!(r, s4([0, 0, (-128i32) as u32, 0]), "fpcr {fpcr:#x}");
    }
}

#[test]
fn fcvtzs_vector_fixed_saturation_and_nan() {
    // 0xcb800001 = -(2^24 + 2); * 2^7 = -(2^31 + 256) < i32::MIN -> i32::MIN, invalid
    let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([0xcb80_0001, 0, 0, 0]), 0);
    assert_eq!(r, s4([0x8000_0000, 0, 0, 0]));
    assert_eq!(fpsr, IOC);
    // +inf -> i32::MAX, -inf -> i32::MIN, quiet NaN -> 0, signalling NaN -> 0, each invalid
    for (bits, want) in [
        (0x7f80_0000u32, 0x7fff_ffffu32),
        (0xff80_0000, 0x8000_0000),
        (0x7fc0_0000, 0),
        (0x7fa0_0000, 0),
        (0xffc0_0001, 0),
    ] {
        let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([bits, 0, 0, 0]), 0);
        assert_eq!(r, s4([want, 0, 0, 0]), "{bits:#x}");
        assert_eq!(fpsr, IOC, "{bits:#x}");
    }
}

#[test]
fn fcvtzu_vector_fixed_4s() {
    // 1.0 -> 128; (2^25 - 2) * 2^7 = 2^32 - 256 = 0xffffff00 is in range; 2^25 * 2^7 = 2^32 -> u32::MAX (invalid)
    let (r, fpsr) = vec_op(FCVTZU_4S_7, s4([0x3f80_0000, 0x4bff_ffff, 0, 0]), 0);
    assert_eq!(r, s4([128, 0xffff_ff00, 0, 0]));
    assert_eq!(fpsr, 0);
    let (r, fpsr) = vec_op(FCVTZU_4S_7, s4([0x4c00_0000, 0, 0, 0]), 0);
    assert_eq!(r, s4([0xffff_ffff, 0, 0, 0]));
    assert_eq!(fpsr, IOC);
    // -1.0 is out of range for an unsigned result: 0, invalid. -2^-8 * 2^7 = -0.5 truncates to 0: inexact only.
    let (r, fpsr) = vec_op(FCVTZU_4S_7, s4([0xbf80_0000, 0, 0, 0]), 0);
    assert_eq!((r, fpsr), (0, IOC));
    let (r, fpsr) = vec_op(FCVTZU_4S_7, s4([0xbb80_0000, 0, 0, 0]), 0);
    assert_eq!((r, fpsr), (0, IXC));
}

#[test]
fn fcvtzs_vector_fixed_2d() {
    // fbits 20: 1.0 -> 2^20; -1.5 -> -1572864 = 0xffffffffffe80000
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([0x3ff0_0000_0000_0000, 0xbff8_0000_0000_0000]), 0);
    assert_eq!(r, d2([0x10_0000, 0xffff_ffff_ffe8_0000]));
    assert_eq!(fpsr, 0);
    // 2^43 * 2^20 = 2^63 -> i64::MAX (invalid); -2^43 * 2^20 = i64::MIN exactly (in range)
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([0x42a0_0000_0000_0000, 0]), 0);
    assert_eq!((r, fpsr), (d2([0x7fff_ffff_ffff_ffff, 0]), IOC));
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([0xc2a0_0000_0000_0000, 0]), 0);
    assert_eq!((r, fpsr), (d2([0x8000_0000_0000_0000, 0]), 0));
    // 2^-21 * 2^20 = 0.5 -> 0, inexact; NaN -> 0 invalid; -inf -> i64::MIN invalid
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([0x3ea0_0000_0000_0000, 0]), 0);
    assert_eq!((r, fpsr), (0, IXC));
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([0x7ff8_0000_0000_0000, 0xfff0_0000_0000_0000]), 0);
    assert_eq!((r, fpsr), (d2([0, 0x8000_0000_0000_0000]), IOC));
    // the largest finite double times 2^20 overflows in double arithmetic but is still just out of range: i64::MAX
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([0x7fef_ffff_ffff_ffff, 0]), 0);
    assert_eq!((r, fpsr), (d2([0x7fff_ffff_ffff_ffff, 0]), IOC));
}

#[test]
fn fcvtzu_vector_fixed_2d() {
    // 2^43 * 2^20 = 2^63 is in range for unsigned: 0x8000...; 2^44 * 2^20 = 2^64 -> u64::MAX (invalid)
    let (r, fpsr) = vec_op(FCVTZU_2D_20, d2([0x42a0_0000_0000_0000, 0]), 0);
    assert_eq!((r, fpsr), (d2([0x8000_0000_0000_0000, 0]), 0));
    let (r, fpsr) = vec_op(FCVTZU_2D_20, d2([0x42b0_0000_0000_0000, 0xbff0_0000_0000_0000]), 0);
    assert_eq!((r, fpsr), (d2([u64::MAX, 0]), IOC));
}

#[test]
fn fcvtz_vector_fixed_flushes_denormal_input_under_fz() {
    // the smallest single denormal times 2^7 truncates to 0. Without FZ that is inexact; with FZ the input is flushed
    // to zero first: input-denormal (IDC) only.
    let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([1, 0, 0, 0]), 0);
    assert_eq!((r, fpsr), (0, IXC));
    let (r, fpsr) = vec_op(FCVTZS_4S_7, s4([1, 0, 0, 0]), FZ);
    assert_eq!((r, fpsr), (0, IDC));
    // smallest double denormal, .2d
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([1, 0]), 0);
    assert_eq!((r, fpsr), (0, IXC));
    let (r, fpsr) = vec_op(FCVTZS_2D_20, d2([1, 0]), FZ);
    assert_eq!((r, fpsr), (0, IDC));
}

/// `fjcvtzs w8, d0`
const FJCVTZS: u32 = 0x1e7e_0008;

// scalar scvtf/ucvtf/fcvtzs/fcvtzu s2, s0, #7 and d2, d0, #20
const SCVTF_S_7: u32 = 0x5f39_e402;
const UCVTF_S_7: u32 = 0x7f39_e402;
const FCVTZS_S_7: u32 = 0x5f39_fc02;
const FCVTZU_S_7: u32 = 0x7f39_fc02;
const SCVTF_D_20: u32 = 0x5f6c_e402;
const UCVTF_D_20: u32 = 0x7f6c_e402;
const FCVTZS_D_20: u32 = 0x5f6c_fc02;
const FCVTZU_D_20: u32 = 0x7f6c_fc02;

#[test]
fn decode_fixed_point_scalar_converts() {
    use instructions::SimdOp2::*;
    let want = |op, esize, fbits| Instruction::ScalarFixedConv {
        rd: 2,
        rn: 0,
        op,
        esize,
        fbits,
    };
    assert_eq!(decoder::decode(SCVTF_S_7), want(Scvtf, 32, 7));
    assert_eq!(decoder::decode(UCVTF_S_7), want(Ucvtf, 32, 7));
    assert_eq!(decoder::decode(FCVTZS_S_7), want(Fcvtzs, 32, 7));
    assert_eq!(decoder::decode(FCVTZU_S_7), want(Fcvtzu, 32, 7));
    assert_eq!(decoder::decode(SCVTF_D_20), want(Scvtf, 64, 20));
    assert_eq!(decoder::decode(UCVTF_D_20), want(Ucvtf, 64, 20));
    assert_eq!(decoder::decode(FCVTZS_D_20), want(Fcvtzs, 64, 20));
    assert_eq!(decoder::decode(FCVTZU_D_20), want(Fcvtzu, 64, 20));
    // extremes: immh:immb = 0100000 is fbits 32, 0111111 is fbits 1, 1000000 is fbits 64, 1111111 is fbits 1
    assert_eq!(decoder::decode(0x5f20_e402), want(Scvtf, 32, 32));
    assert_eq!(decoder::decode(0x5f3f_e402), want(Scvtf, 32, 1));
    assert_eq!(decoder::decode(0x5f40_e402), want(Scvtf, 64, 64));
    assert_eq!(decoder::decode(0x5f7f_e402), want(Scvtf, 64, 1));
    // reserved or not decoded: immh = 0001 is reserved, immh = 001x is half precision
    for w in [0x5f09_e402u32, 0x5f19_e402, 0x5f11_e402, 0x5f19_fc02, 0x7f11_fc02] {
        assert!(
            matches!(decoder::decode(w), Instruction::Unimplemented { .. }),
            "{w:#010x} {:?}",
            decoder::decode(w)
        );
    }
    // the neighbouring scalar shifts still decode as shifts (SSHR d2, d0, #3 = 0x5f7d0402)
    assert!(!matches!(
        decoder::decode(0x5f7d_0402),
        Instruction::ScalarFixedConv { .. } | Instruction::Unimplemented { .. }
    ));
}

#[test]
fn scalar_fixed_single_converts_one_lane_and_zeroes_the_rest() {
    // the other lanes of v0 are dirty and v2 is all ones: only lane 0 is converted and the rest of v2 is cleared
    let dirty = |lane0: u32| s4([lane0, 0xdead_beef, 0x1234_5678, 0xffff_ffff]);
    // fbits 7: 128 -> 1.0, -128 -> -1.0
    assert_eq!(vec_op(SCVTF_S_7, dirty(128), 0), (0x3f80_0000, 0));
    assert_eq!(vec_op(SCVTF_S_7, dirty((-128i32) as u32), 0), (0xbf80_0000, 0));
    // unsigned: 0xffffff80 / 128 = 2^25 - 1, a tie that rounds to even, 2^25, inexact
    assert_eq!(vec_op(UCVTF_S_7, dirty(0xffff_ff80), 0), (0x4c00_0000, IXC));
    // 1.5 * 2^7 = 192; 1.0 * 2^7 = 128; truncation toward zero for both signs
    assert_eq!(vec_op(FCVTZS_S_7, dirty(0x3fc0_0000), 0), (192, 0));
    assert_eq!(vec_op(FCVTZS_S_7, dirty(0xbfc0_0000), 0), (0xffff_ff40, 0));
    assert_eq!(vec_op(FCVTZU_S_7, dirty(0x3f80_0000), 0), (128, 0));
    // saturation raises IOC: 2^31 * 2^7 for the signed form, a negative value for the unsigned one
    assert_eq!(vec_op(FCVTZS_S_7, dirty(0x4f00_0000), 0), (0x7fff_ffff, IOC));
    assert_eq!(vec_op(FCVTZU_S_7, dirty(0xbf80_0000), 0), (0, IOC));
}

#[test]
fn scalar_fixed_double_converts_one_lane_and_zeroes_the_rest() {
    let dirty = |lane0: u64| d2([lane0, 0xdead_beef_dead_beef]);
    // fbits 20: 2^20 -> 1.0, -2^20 -> -1.0, 3 * 2^19 -> 1.5
    assert_eq!(vec_op(SCVTF_D_20, dirty(1 << 20), 0), (0x3ff0_0000_0000_0000, 0));
    assert_eq!(
        vec_op(SCVTF_D_20, dirty((-(1i64 << 20)) as u64), 0),
        (0xbff0_0000_0000_0000, 0)
    );
    assert_eq!(vec_op(UCVTF_D_20, dirty(3 << 19), 0), (0x3ff8_0000_0000_0000, 0));
    // 1.5 * 2^20 = 0x180000; saturation: 2^44 * 2^20 = 2^64 overflows the unsigned form
    assert_eq!(vec_op(FCVTZS_D_20, dirty(0x3ff8_0000_0000_0000), 0), (0x18_0000, 0));
    assert_eq!(
        vec_op(FCVTZU_D_20, dirty(0x42b0_0000_0000_0000), 0),
        (u64::MAX as u128, IOC)
    );
    assert_eq!(
        vec_op(FCVTZS_D_20, dirty(0xc3e0_0000_0000_0000), 0),
        (0x8000_0000_0000_0000, IOC)
    );
}

#[test]
fn scalar_fixed_converts_follow_rmode_and_fz() {
    // SCVTF rounds under FPCR.RMode: 1 / 2^7 is exact, 0x7fffffff / 2^7 is not, and RP rounds it up to 2^24 + ... = 2^24
    assert_eq!(vec_op(SCVTF_S_7, s4([0x7fff_ffff, 0, 0, 0]), RP), (0x4b80_0000, IXC));
    // a denormal input under FZ is flushed and raises IDC; FCVTZS of +0 gives 0
    assert_eq!(vec_op(FCVTZS_S_7, s4([1, 0, 0, 0]), FZ), (0, IDC));
}

/// Runs FJCVTZS with d0 = `bits`, NZCV all set beforehand (so every flag must be written) and the given FPCR.
fn js(bits: u64, fpcr: u32) -> (u32, bool, u32) {
    let (mut cpu, mut mmu) = machine(&[FJCVTZS]);
    cpu.regs.v[0] = (bits as u128) | (0xdead_beef_u128 << 64);
    cpu.regs.write_x(8, u64::MAX);
    cpu.regs.pstate.n = true;
    cpu.regs.pstate.z = false;
    cpu.regs.pstate.c = true;
    cpu.regs.pstate.v = true;
    cpu.regs.fpcr = fpcr;
    assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)));
    assert_eq!(cpu.regs.pc, CODE + 4);
    // N, C and V are always cleared by FJCVTZS; the only flag it can set is Z
    assert!(!cpu.regs.pstate.n && !cpu.regs.pstate.c && !cpu.regs.pstate.v);
    // W8 is written and zero-extended
    assert_eq!(cpu.regs.read_x(8) >> 32, 0);
    (cpu.regs.read_x(8) as u32, cpu.regs.pstate.z, cpu.regs.fpsr)
}

#[test]
fn decode_fjcvtzs() {
    assert_eq!(decoder::decode(FJCVTZS), Instruction::Fjcvtzs { rd: 8, rn: 0 });
    assert_eq!(decoder::decode(0x1e7e_03ff), Instruction::Fjcvtzs { rd: 31, rn: 31 });
    // only sf = 0 with ftype = 01 exists; sf = 1, the single-precision type and the half type are not FJCVTZS
    for w in [0x9e7e_0008u32, 0x1e3e_0008, 0x1e7e_0408] {
        assert!(!matches!(decoder::decode(w), Instruction::Fjcvtzs { .. }), "{w:#010x}");
    }
}

#[test]
fn fjcvtzs_exact_in_range_sets_z() {
    // 1.0 -> 1, -1.0 -> 0xffffffff, +0.0 -> 0, -2^31 -> 0x80000000, 2^31 - 1 -> 0x7fffffff: exact, Z = 1, no flags
    for (bits, want) in [
        (0x3ff0_0000_0000_0000u64, 1u32),
        (0xbff0_0000_0000_0000, 0xffff_ffff),
        (0, 0),
        (0xc1e0_0000_0000_0000, 0x8000_0000),
        (0x41df_ffff_ffc0_0000, 0x7fff_ffff),
    ] {
        assert_eq!(js(bits, 0), (want, true, 0), "{bits:#x}");
    }
}

#[test]
fn fjcvtzs_negative_zero_clears_z_without_flags() {
    assert_eq!(js(0x8000_0000_0000_0000, 0), (0, false, 0));
}

#[test]
fn fjcvtzs_truncates_toward_zero_and_flags_inexact() {
    // 1.5 -> 1, -1.5 -> -1, 0.5 -> 0, -0.5 -> 0 (not -1): Z = 0, IXC
    for (bits, want) in [
        (0x3ff8_0000_0000_0000u64, 1u32),
        (0xbff8_0000_0000_0000, 0xffff_ffff),
        (0x3fe0_0000_0000_0000, 0),
        (0xbfe0_0000_0000_0000, 0),
        // 2147483647.5 -> 0x7fffffff (in range), -2147483648.5 -> 0x80000000 (in range)
        (0x41df_ffff_ffe0_0000, 0x7fff_ffff),
        (0xc1e0_0000_0010_0000, 0x8000_0000),
    ] {
        assert_eq!(js(bits, 0), (want, false, IXC), "{bits:#x}");
    }
    // the smallest denormal truncates to 0, inexact
    assert_eq!(js(1, 0), (0, false, IXC));
}

#[test]
fn fjcvtzs_ignores_fpcr_rounding_mode() {
    for fpcr in [RP, RM, RZ] {
        assert_eq!(js(0x3ff8_0000_0000_0000, fpcr), (1, false, IXC), "{fpcr:#x}");
        assert_eq!(js(0xbff8_0000_0000_0000, fpcr), (0xffff_ffff, false, IXC), "{fpcr:#x}");
    }
}

#[test]
fn fjcvtzs_out_of_range_wraps_modulo_2_pow_32_and_raises_ioc() {
    for (bits, want) in [
        // 2^31 -> 0x80000000; 2^32 -> 0; 2^32 + 1 -> 1
        (0x41e0_0000_0000_0000u64, 0x8000_0000u32),
        (0x41f0_0000_0000_0000, 0),
        (0x41f0_0000_0010_0000, 1),
        // 6442450943 = 0x1_7fff_ffff -> 0x7fffffff; its negation -> 2^32 - 0x7fffffff = 0x80000001
        (0x41f7_ffff_fff0_0000, 0x7fff_ffff),
        (0xc1f7_ffff_fff0_0000, 0x8000_0001),
        // -2147483649 = -(2^31 + 1) -> 2^32 - 2^31 - 1 = 0x7fffffff
        (0xc1e0_0000_0020_0000, 0x7fff_ffff),
        // 2^63 -> 0; 2^63 + 2^11 (the next double) -> 0x800; -2^63 -> 0
        (0x43e0_0000_0000_0000, 0),
        (0x43e0_0000_0000_0001, 0x800),
        (0xc3e0_0000_0000_0000, 0),
        // 3 * 2^39 = 1.5 * 2^40 is a multiple of 2^32 -> 0; 2^84 -> 0
        (0x4278_0000_0000_0000, 0),
        (0x4530_0000_0000_0000, 0),
        // the largest finite double is a multiple of 2^32 -> 0
        (0x7fef_ffff_ffff_ffff, 0),
    ] {
        assert_eq!(js(bits, 0), (want, false, IOC), "{bits:#x}");
    }
}

#[test]
fn fjcvtzs_infinity_and_nan_give_zero_with_ioc() {
    for bits in [
        0x7ff0_0000_0000_0000u64,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0xfff8_0000_0000_0001,
    ] {
        assert_eq!(js(bits, 0), (0, false, IOC), "{bits:#x}");
    }
}

#[test]
fn fjcvtzs_flush_to_zero_flushes_denormal_input() {
    // A denormal flushed to zero by FZ gives result 0, IDC and Z = 0, for either sign: the pseudocode tests the unpacked
    // input, so the flushed value is not an exact zero. Measured on Apple M-series hardware (FJCVTZS at FPCR.FZ = 1:
    // 0000000000000001, 8000000000000001 and 000fffffffffffff all give NZCV Z = 0, FPSR = 0x80). qemu-aarch64 gives
    // Z = 1 for the negative one, which is wrong.
    assert_eq!(js(1, FZ), (0, false, IDC));
    assert_eq!(js(0x000f_ffff_ffff_ffff, FZ), (0, false, IDC));
    assert_eq!(js(0x8000_0000_0000_0001, FZ), (0, false, IDC));
}

#[test]
fn fjcvtzs_zero_register_destination_is_discarded() {
    let (mut cpu, mut mmu) = machine(&[0x1e7e_001f]); // fjcvtzs wzr, d0
    cpu.regs.v[0] = 0x3ff8_0000_0000_0000;
    assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)));
    assert_eq!(cpu.regs.read_x(31), 0);
    assert!(!cpu.regs.pstate.z);
}

/// The interpreter and the Tier-2/Tier-3 engines must agree, registers, flags and FPSR included.
#[test]
fn new_converts_agree_between_interpreter_and_tier3() {
    let words = [
        0x4f39_e402, // scvtf v2.4s, v0.4s, #7
        0x6f6c_e403, // ucvtf v3.2d, v0.2d, #20
        0x4f39_fc04, // fcvtzs v4.4s, v0.4s, #7
        0x6f6c_fc05, // fcvtzu v5.2d, v0.2d, #20
        0x5f39_e406, // scvtf s6, s0, #7
        0x7f6c_e407, // ucvtf d7, d0, #20
        0x5f39_fc09, // fcvtzs s9, s0, #7
        0x7f6c_fc0a, // fcvtzu d10, d0, #20
        0x1e7e_0008, // fjcvtzs w8, d0
        0xd400_0001, // svc #0
    ];
    for (v0, fpcr) in [
        (0x4b80_0001_c020_0000_3fc0_0000_7fa0_0000u128, 0u32),
        (0x7ff8_0000_0000_0000_0000_0000_0000_0001, FZ),
        (0x41f7_ffff_fff0_0000_c1e0_0000_0010_0000, RP),
    ] {
        let run_with = |tier3: bool| {
            let (mut cpu, mut mmu) = machine(&words);
            cpu.regs.v[0] = v0;
            cpu.regs.fpcr = fpcr;
            if tier3 {
                let mut e = Tier3JitEngine::new();
                let (_, exit) = e.run_slice(&mut cpu, &mut mmu, 100);
                assert!(matches!(exit, BlockExit::Svc { .. }), "{exit:?}");
            } else {
                for _ in 0..words.len() - 1 {
                    assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)));
                }
            }
            (
                cpu.regs.v,
                cpu.regs.x[8],
                cpu.regs.pstate.to_bits() >> 28,
                cpu.regs.fpsr,
            )
        };
        let reference = run_with(false);
        assert_eq!(run_with(true), reference, "v0 {v0:#x} fpcr {fpcr:#x}");
    }
}

/// Seeded xorshift64*, so a failure reproduces from the printed inputs.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// A float bit pattern that is often special or near a rounding/saturation edge.
    fn float_bits(&mut self, double: bool) -> u64 {
        let r = self.next();
        let (ebits, mbits) = if double { (11, 52) } else { (8, 23) };
        let sign = (r >> 63) << (ebits + mbits);
        let frac = self.next() & ((1u64 << mbits) - 1);
        let max_e = (1u64 << ebits) - 1;
        let bias = max_e >> 1;
        let e = match (r >> 8) % 8 {
            0 => 0,                          // zero and denormals
            1 => max_e,                      // infinities and NaNs
            2 => bias + (r >> 16) % 70 + 10, // magnitudes 2^10 .. 2^79: saturation edges
            _ => bias - 5 + (r >> 16) % 80,  // the bulk: 2^-5 .. 2^74
        };
        sign | e.min(max_e) << mbits | frac
    }
}

/// Runs `word` once (with x1 = `x`, v0 = `v`, FPCR = `fpcr`) and returns the machine.
fn scalar_op(word: u32, x: u64, v: u64, fpcr: u32) -> Cpu {
    let (mut cpu, mut mmu) = machine(&[word]);
    cpu.regs.write_x(1, x);
    cpu.regs.v[0] = v as u128;
    cpu.regs.fpcr = fpcr;
    cpu.step(&mut mmu).unwrap();
    cpu
}

/// The new vector forms are the scalar general-register fixed-point converts applied per lane, so every lane must
/// match the existing scalar instruction (which the fp-workload compares against hardware) for every `fbits`:
/// result bits and FPSR, under FPCR default, FZ and RMode.
#[test]
fn vector_fixed_lanes_match_scalar_fixed_converts() {
    let mut rng = Rng(0x2026_1118_0469_0488);
    for double in [false, true] {
        let esize: u32 = if double { 64 } else { 32 };
        let mask = if double { u64::MAX } else { u32::MAX as u64 };
        for fbits in 1..=esize {
            let imm = 2 * esize - fbits;
            // vector word with Rn = v0, Rd = v2; scalar word with Rn = x1/w1 or d0/s0 and Rd = x1/w1 or v1
            let vector = |u: u32, opc: u32| 0x4f00_0400 | u << 29 | imm << 16 | opc << 11 | 2;
            let scalar = |opcode: u32, rn: u32, rd: u32| {
                let base = if double { 0x9e40_0000u32 } else { 0x1e00_0000 };
                base | (if opcode < 2 { 3 } else { 0 }) << 19 | opcode << 16 | (64 - fbits) << 10 | rn << 5 | rd
            };
            for _ in 0..40 {
                let fpcr = [0, FZ, RP, RM | FZ][(rng.next() % 4) as usize];
                // float -> fixed: fcvtzs (opcode 0) and fcvtzu (opcode 1), lane 0 and lane 1
                for (u, opcode) in [(0u32, 0u32), (1, 1)] {
                    let f = [rng.float_bits(double), rng.float_bits(double)];
                    let src = if double {
                        d2(f)
                    } else {
                        s4([f[0] as u32, f[1] as u32, 0, 0])
                    };
                    let (v, fpsr) = vec_op(vector(u, 0b11111), src, fpcr);
                    let mut scalar_flags = 0;
                    for (i, &bits) in f.iter().enumerate() {
                        let cpu = scalar_op(scalar(opcode, 0, 1), 0, bits, fpcr);
                        let lane = (v >> (esize as usize * i)) as u64 & mask;
                        assert_eq!(
                            lane,
                            cpu.regs.read_x(1) & mask,
                            "fcvtz u={u} lane {i} {bits:#x} #{fbits} fpcr {fpcr:#x}"
                        );
                        scalar_flags |= cpu.regs.fpsr;
                    }
                    assert_eq!(fpsr, scalar_flags, "fcvtz flags u={u} {f:x?} #{fbits} fpcr {fpcr:#x}");
                }
                // fixed -> float: scvtf (opcode 2) and ucvtf (opcode 3)
                for (u, opcode) in [(0u32, 2u32), (1, 3)] {
                    let x = [rng.next() & mask, rng.next() & mask];
                    let src = if double {
                        d2(x)
                    } else {
                        s4([x[0] as u32, x[1] as u32, 0, 0])
                    };
                    let (v, fpsr) = vec_op(vector(u, 0b11100), src, fpcr);
                    let mut scalar_flags = 0;
                    for (i, &bits) in x.iter().enumerate() {
                        let cpu = scalar_op(scalar(opcode, 1, 1), bits, 0, fpcr);
                        let lane = (v >> (esize as usize * i)) as u64 & mask;
                        assert_eq!(
                            lane,
                            cpu.regs.v[1] as u64 & mask,
                            "cvtf u={u} lane {i} {bits:#x} #{fbits} fpcr {fpcr:#x}"
                        );
                        scalar_flags |= cpu.regs.fpsr;
                    }
                    assert_eq!(fpsr, scalar_flags, "cvtf flags u={u} {x:x?} #{fbits} fpcr {fpcr:#x}");
                }
            }
        }
    }
}

/// FJCVTZS against Rust's saturating cast for every input whose truncation fits an i64, where the modulo-2^32 result is
/// just the low 32 bits of that integer.
#[test]
fn fjcvtzs_matches_i64_truncation_modulo_2_pow_32() {
    let mut rng = Rng(0x0fcc_7255);
    let mut checked = 0;
    for _ in 0..20_000 {
        let bits = rng.float_bits(true);
        let v = f64::from_bits(bits);
        if v.is_nan() || v.is_infinite() || v.abs() >= 9.2e18 {
            continue;
        }
        checked += 1;
        let t = v.trunc();
        let in_range = (-2147483648.0..=2147483647.0).contains(&t);
        let exact = t == v && !(v == 0.0 && v.is_sign_negative());
        let (got, z, fpsr) = js(bits, 0);
        assert_eq!(got, (t as i64) as u32, "{bits:#x}");
        assert_eq!(z, in_range && exact, "{bits:#x}");
        let flags = if !in_range {
            IOC
        } else if t != v {
            IXC
        } else {
            0
        };
        assert_eq!(fpsr, flags, "{bits:#x}");
    }
    assert!(checked > 10_000);
}
