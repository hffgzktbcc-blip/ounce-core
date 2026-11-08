#![allow(dead_code)]

//! Lane arithmetic for the AdvSIMD integer instructions, on 128-bit register values.
//! `esize` is the element size in bits (8, 16, 32 or 64). With `q == false` only the low 64 bits are
//! used and the result's upper 64 bits are zero, as the architecture requires.

use super::instructions::{SimdFpElementOp, SimdAcrossOp, SimdLongOp, SimdNarrowOp, SimdOp2, SimdOp3, SimdPermuteOp, SimdSatLongOp};

#[inline]
fn mask(esize: u32) -> u64 {
    if esize == 64 { u64::MAX } else { (1u64 << esize) - 1 }
}

#[inline]
pub fn lane(v: u128, esize: u32, i: usize) -> u64 {
    (v >> (i as u32 * esize)) as u64 & mask(esize)
}

#[inline]
pub fn set_lane(v: &mut u128, esize: u32, i: usize, val: u64) {
    let shift = i as u32 * esize;
    *v &= !((mask(esize) as u128) << shift);
    *v |= ((val & mask(esize)) as u128) << shift;
}

/// Across-lanes reduction; the scalar result lands in lane 0 and the rest of the register is zero.
pub fn across(op: SimdAcrossOp, n: u128, esize: u32, q: bool) -> u128 {
    let lanes = (if q { 128 } else { 64 } / esize) as usize;
    let vals = (0..lanes).map(|i| lane(n, esize, i));
    let signed = |v: u64| sext(v, esize);
    let res = match op {
        SimdAcrossOp::Addv => vals.fold(0u64, |a, v| a.wrapping_add(v)) & mask(esize),
        SimdAcrossOp::Uaddlv => vals.fold(0u64, |a, v| a.wrapping_add(v)) & mask(esize * 2),
        SimdAcrossOp::Saddlv => (vals.fold(0i64, |a, v| a.wrapping_add(signed(v))) as u64) & mask(esize * 2),
        SimdAcrossOp::Umaxv => vals.max().unwrap_or(0),
        SimdAcrossOp::Uminv => vals.min().unwrap_or(0),
        SimdAcrossOp::Smaxv => vals.map(signed).max().unwrap_or(0) as u64 & mask(esize),
        SimdAcrossOp::Sminv => vals.map(signed).min().unwrap_or(0) as u64 & mask(esize),
    };
    res as u128
}

#[inline]
fn sext(val: u64, esize: u32) -> i64 {
    let s = 64 - esize;
    ((val << s) as i64) >> s
}

#[inline]
fn lanes(esize: u32, q: bool) -> usize {
    (if q { 128 } else { 64 }) / esize as usize
}

#[inline]
fn width_mask(q: bool) -> u128 {
    if q { u128::MAX } else { u64::MAX as u128 }
}

/// `DUP Vd.<T>, Rn`: every lane gets the low `esize` bits of `val`.
pub fn dup(val: u64, esize: u32, q: bool) -> u128 {
    let mut out = 0u128;
    for i in 0..lanes(esize, q) {
        set_lane(&mut out, esize, i, val);
    }
    out
}

/// Three-register same-type operations. `d` is the destination's old value (used by `BSL`/`BIT`/`BIF`).
pub fn three_same(op: SimdOp3, d: u128, n: u128, m: u128, esize: u32, q: bool) -> u128 {
    let bitwise = match op {
        SimdOp3::And => Some(n & m),
        SimdOp3::Bic => Some(n & !m),
        SimdOp3::Orr => Some(n | m),
        SimdOp3::Orn => Some(n | !m),
        SimdOp3::Eor => Some(n ^ m),
        SimdOp3::Bsl => Some((d & n) | (!d & m)),
        SimdOp3::Bit => Some((d & !m) | (n & m)),
        SimdOp3::Bif => Some((d & m) | (n & !m)),
        _ => None,
    };
    if let Some(v) = bitwise {
        return v & width_mask(q);
    }

    let count = lanes(esize, q);
    let all = mask(esize);
    let mut out = 0u128;
    let pairwise = matches!(op, SimdOp3::Addp | SimdOp3::Uminp | SimdOp3::Umaxp | SimdOp3::Sminp | SimdOp3::Smaxp);
    for i in 0..count {
        let (a, b) = if pairwise {
            // Lanes of `n` then `m`, taken two at a time.
            let pick = |j: usize| if j < count { lane(n, esize, j) } else { lane(m, esize, j - count) };
            (pick(2 * i), pick(2 * i + 1))
        } else {
            (lane(n, esize, i), lane(m, esize, i))
        };
        let (sa, sb) = (sext(a, esize), sext(b, esize));
        let r = match op {
            SimdOp3::Add | SimdOp3::Addp => a.wrapping_add(b),
            SimdOp3::Sub => a.wrapping_sub(b),
            SimdOp3::Mul => a.wrapping_mul(b),
            SimdOp3::Cmeq => if a == b { all } else { 0 },
            SimdOp3::Cmtst => if a & b != 0 { all } else { 0 },
            SimdOp3::Cmhi => if a > b { all } else { 0 },
            SimdOp3::Cmhs => if a >= b { all } else { 0 },
            SimdOp3::Cmgt => if sa > sb { all } else { 0 },
            SimdOp3::Cmge => if sa >= sb { all } else { 0 },
            SimdOp3::Umax | SimdOp3::Umaxp => a.max(b),
            SimdOp3::Umin | SimdOp3::Uminp => a.min(b),
            SimdOp3::Smax | SimdOp3::Smaxp => sa.max(sb) as u64,
            SimdOp3::Smin | SimdOp3::Sminp => sa.min(sb) as u64,
            _ => unreachable!("bitwise ops return above"),
        };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// Two-register miscellaneous operations.
pub fn two_misc(op: SimdOp2, n: u128, esize: u32, q: bool) -> u128 {
    if op == SimdOp2::Not {
        return !n & width_mask(q);
    }
    if let SimdOp2::Rev64 | SimdOp2::Rev32 | SimdOp2::Rev16 = op {
        let container = match op { SimdOp2::Rev64 => 64, SimdOp2::Rev32 => 32, _ => 16 };
        let per = (container / esize) as usize;
        let mut out = 0u128;
        for i in 0..lanes(esize, q) {
            let (c, j) = (i / per, i % per);
            set_lane(&mut out, esize, c * per + (per - 1 - j), lane(n, esize, i));
        }
        return out;
    }
    let all = mask(esize);
    let mut out = 0u128;
    for i in 0..lanes(esize, q) {
        let a = lane(n, esize, i);
        let s = sext(a, esize);
        let r = match op {
            SimdOp2::Cmeq0 => if a == 0 { all } else { 0 },
            SimdOp2::Cmgt0 => if s > 0 { all } else { 0 },
            SimdOp2::Cmge0 => if s >= 0 { all } else { 0 },
            SimdOp2::Cmle0 => if s <= 0 { all } else { 0 },
            SimdOp2::Cmlt0 => if s < 0 { all } else { 0 },
            SimdOp2::Cnt => a.count_ones() as u64,
            SimdOp2::Abs => s.unsigned_abs(),
            SimdOp2::Neg => (s.wrapping_neg()) as u64,
            SimdOp2::Scvtf if esize == 32 => (s as f32).to_bits() as u64,
            SimdOp2::Scvtf => (s as f64).to_bits(),
            SimdOp2::Ucvtf if esize == 32 => (a as f32).to_bits() as u64,
            SimdOp2::Ucvtf => (a as f64).to_bits(),
            // Rust's float-to-int `as` saturates and maps NaN to 0, as FCVTZS/FCVTZU do
            SimdOp2::Fcvtzs if esize == 32 => f32::from_bits(a as u32) as i32 as u64,
            SimdOp2::Fcvtzs => f64::from_bits(a) as i64 as u64,
            SimdOp2::Fcvtzu if esize == 32 => f32::from_bits(a as u32) as u32 as u64,
            SimdOp2::Fcvtzu => f64::from_bits(a) as u64,
            SimdOp2::Not | SimdOp2::Rev64 | SimdOp2::Rev32 | SimdOp2::Rev16 => unreachable!(),
        };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// `SSHR`/`USHR`/`SHL` by `shift` on `esize` lanes.
pub fn shift_imm(n: u128, esize: u32, shift: u32, q: bool, signed: bool, left: bool) -> u128 {
    let mut out = 0u128;
    for i in 0..lanes(esize, q) {
        let a = lane(n, esize, i);
        let r = if left {
            if shift >= 64 { 0 } else { a << shift }
        } else if signed {
            (sext(a, esize) >> shift.min(63)) as u64
        } else if shift >= 64 {
            0
        } else {
            a >> shift
        };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// `SHRN`/`SHRN2`: shifts each `2 * esize` lane of `n` right and keeps the low `esize` bits.
/// `SHRN` writes the low half and zeroes the rest; `SHRN2` (`q`) writes the high half and keeps `d`'s low half.
pub fn shrn(d: u128, n: u128, esize: u32, shift: u32, q: bool) -> u128 {
    let count = 64 / esize as usize;
    let mut half = 0u128;
    for i in 0..count {
        set_lane(&mut half, esize, i, lane(n, esize * 2, i) >> shift);
    }
    if q { (d & u64::MAX as u128) | (half << 64) } else { half }
}

/// `SSHLL`/`USHLL` (`q`: the `2` forms): widens the low (or, with `q`, high) `esize` lanes of `n` to
/// `2 * esize` lanes and shifts them left.
pub fn shll(n: u128, esize: u32, shift: u32, q: bool, signed: bool) -> u128 {
    let src = if q { n >> 64 } else { n };
    let mut out = 0u128;
    for i in 0..64 / esize as usize {
        let a = lane(src, esize, i);
        let wide = if signed { sext(a, esize) as u64 } else { a };
        set_lane(&mut out, esize * 2, i, wide << shift);
    }
    out
}

/// AdvSIMD three-different (widening) integer ops (`q`: the `2` forms read the high source half).
/// `S/UMULL`, `S/UMLAL`, `S/UMLSL` multiply; `S/UADDL`, `S/USUBL`, `S/UABDL`, `S/UABAL` widen both
/// operands; `S/UADDW`, `S/USUBW` take a full-width `n` and a widened `m`.
pub fn mul_long(op: SimdLongOp, d: u128, n: u128, m: u128, esize: u32, q: bool, signed: bool) -> u128 {
    let half = |v: u128| if q { v >> 64 } else { v };
    let (hn, hm) = (half(n), half(m));
    let wide = esize * 2;
    let ext = |a: u64| -> u64 { if signed { sext(a, esize) as u64 } else { a } };
    let mut out = 0u128;
    for i in 0..64 / esize as usize {
        let b = ext(lane(hm, esize, i));
        let a = if matches!(op, SimdLongOp::Addw | SimdLongOp::Subw) { lane(n, wide, i) } else { ext(lane(hn, esize, i)) };
        let acc = lane(d, wide, i);
        let abd = if signed { (a as i64).wrapping_sub(b as i64).unsigned_abs() } else if a >= b { a - b } else { b - a };
        let r = match op {
            SimdLongOp::Mull => a.wrapping_mul(b),
            SimdLongOp::Mlal => acc.wrapping_add(a.wrapping_mul(b)),
            SimdLongOp::Mlsl => acc.wrapping_sub(a.wrapping_mul(b)),
            SimdLongOp::Addl | SimdLongOp::Addw => a.wrapping_add(b),
            SimdLongOp::Subl | SimdLongOp::Subw => a.wrapping_sub(b),
            SimdLongOp::Abdl => abd,
            SimdLongOp::Abal => acc.wrapping_add(abd),
        };
        set_lane(&mut out, wide, i, r);
    }
    out
}

/// `ADDHN`/`SUBHN`/`RADDHN`/`RSUBHN` (`q`: the `2` forms): each `2 * esize` lane of `n` and `m` is added or
/// subtracted (with a rounding constant for the `R` forms) and the high `esize` bits kept. Without `q` the
/// result fills the low half and the upper half is zero; with `q` it fills the high half and the low half of
/// `d` is preserved.
pub fn narrow_high(op: SimdNarrowOp, d: u128, n: u128, m: u128, esize: u32, q: bool) -> u128 {
    let wide = esize * 2;
    let round = matches!(op, SimdNarrowOp::Raddhn | SimdNarrowOp::Rsubhn);
    let sub = matches!(op, SimdNarrowOp::Subhn | SimdNarrowOp::Rsubhn);
    let mut half = 0u128;
    for i in 0..64 / esize as usize {
        let (a, b) = (lane(n, wide, i) as u128, lane(m, wide, i) as u128);
        let mut r = if sub { a.wrapping_sub(b) } else { a.wrapping_add(b) };
        if round {
            r = r.wrapping_add(1u128 << (esize - 1));
        }
        set_lane(&mut half, esize, i, ((r >> esize) as u64) & mask(esize));
    }
    if q { (d & ((1u128 << 64) - 1)) | (half << 64) } else { half }
}

fn saturate_signed(v: i128, bits: u32) -> u64 {
    saturate_signed_flag(v, bits).0
}

/// Signed saturation to `bits`, also reporting whether the value did not fit (sets FPSR.QC).
fn saturate_signed_flag(v: i128, bits: u32) -> (u64, bool) {
    let (lo, hi) = (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1);
    ((v.clamp(lo, hi) as u64) & mask(bits), v < lo || v > hi)
}

/// `SQDMULL`/`SQDMLAL`/`SQDMLSL` (`q`: the `2` forms): `2 * a * b` saturated to `2 * esize` signed bits,
/// accumulated (also saturating) into `d` for `SQDMLAL`/`SQDMLSL`. Returns the result and whether any lane
/// saturated (in the doubling multiply or the accumulate), which the caller ORs into FPSR.QC.
pub fn sat_doubling_mul_long(op: SimdSatLongOp, d: u128, n: u128, m: u128, esize: u32, q: bool) -> (u128, bool) {
    let half = |v: u128| if q { v >> 64 } else { v };
    let (hn, hm) = (half(n), half(m));
    let wide = esize * 2;
    let mut out = 0u128;
    let mut sat = false;
    for i in 0..64 / esize as usize {
        let prod = 2 * (sext(lane(hn, esize, i), esize) as i128) * (sext(lane(hm, esize, i), esize) as i128);
        let (prod, s1) = saturate_signed_flag(prod, wide); // doubling overflows only for min * min
        let prod = sext(prod, wide) as i128;
        let acc = sext(lane(d, wide, i), wide) as i128;
        let r = match op {
            SimdSatLongOp::Sqdmull => prod,
            SimdSatLongOp::Sqdmlal => acc + prod,
            SimdSatLongOp::Sqdmlsl => acc - prod,
        };
        let (v, s2) = saturate_signed_flag(r, wide);
        sat |= s1 | s2;
        set_lane(&mut out, wide, i, v);
    }
    (out, sat)
}

/// `PMULL` / `PMULL2`: carry-less multiply. `wide`: one 64x64 -> 128-bit product; otherwise eight
/// 8x8 -> 16-bit products. `q` reads the high 64 bits of each source.
pub fn pmull(n: u128, m: u128, wide: bool, q: bool) -> u128 {
    let half = |v: u128| (if q { v >> 64 } else { v }) as u64;
    let (hn, hm) = (half(n), half(m));
    let clmul = |a: u64, b: u64| -> u128 {
        let mut r = 0u128;
        for i in 0..64 {
            if (b >> i) & 1 != 0 {
                r ^= (a as u128) << i;
            }
        }
        r
    };
    if wide {
        return clmul(hn, hm);
    }
    let mut out = 0u128;
    for i in 0..8 {
        let p = clmul((hn >> (8 * i)) & 0xFF, (hm >> (8 * i)) & 0xFF) as u64;
        out |= (p as u128) << (16 * i);
    }
    out
}

/// `FMUL/FMLA/FMLS` by element: every lane of `n` times lane `index` of `m` (fused for FMLA/FMLS).
pub fn fp_by_element(op: SimdFpElementOp, d: u128, n: u128, m: u128, index: usize, double: bool, q: bool) -> u128 {
    let esize = if double { 64 } else { 32 };
    let mut out = 0u128;
    for i in 0..lanes(esize, q) {
        let bits = if double {
            let (a, b, acc) = (f64::from_bits(lane(n, 64, i)), f64::from_bits(lane(m, 64, index)), f64::from_bits(lane(d, 64, i)));
            match op {
                SimdFpElementOp::Fmul => a * b,
                SimdFpElementOp::Fmla => a.mul_add(b, acc),
                SimdFpElementOp::Fmls => (-a).mul_add(b, acc),
            }
            .to_bits()
        } else {
            let (a, b, acc) = (f32::from_bits(lane(n, 32, i) as u32), f32::from_bits(lane(m, 32, index) as u32), f32::from_bits(lane(d, 32, i) as u32));
            match op {
                SimdFpElementOp::Fmul => a * b,
                SimdFpElementOp::Fmla => a.mul_add(b, acc),
                SimdFpElementOp::Fmls => (-a).mul_add(b, acc),
            }
            .to_bits() as u64
        };
        set_lane(&mut out, esize, i, bits);
    }
    out
}

/// `EXT`: the `16 (8 when !q)` bytes starting at byte `index` of `m:n`.
pub fn extract(n: u128, m: u128, index: u32, q: bool) -> u128 {
    let (n, m) = if q { (n, m) } else { (n & u64::MAX as u128, m & u64::MAX as u128) };
    let shift = index * 8;
    if shift == 0 {
        n
    } else if q {
        (n >> shift) | (m << (128 - shift))
    } else {
        ((n | m << 64) >> shift) & u64::MAX as u128
    }
}

/// `UZP1/UZP2/TRN1/TRN2/ZIP1/ZIP2` on `esize` lanes.
pub fn permute(op: SimdPermuteOp, n: u128, m: u128, esize: u32, q: bool) -> u128 {
    let count = lanes(esize, q);
    let half = count / 2;
    let mut out = 0u128;
    for i in 0..count {
        let v = match op {
            // Even (UZP1) or odd (UZP2) lanes of n then m.
            SimdPermuteOp::Uzp1 | SimdPermuteOp::Uzp2 => {
                let j = 2 * i + (op == SimdPermuteOp::Uzp2) as usize;
                if j < count { lane(n, esize, j) } else { lane(m, esize, j - count) }
            }
            // Pairs: n[2k + p], m[2k + p] with p = 0 (TRN1) or 1 (TRN2).
            SimdPermuteOp::Trn1 | SimdPermuteOp::Trn2 => {
                let j = (i & !1) + (op == SimdPermuteOp::Trn2) as usize;
                if i % 2 == 0 { lane(n, esize, j) } else { lane(m, esize, j) }
            }
            // Interleave the low (ZIP1) or high (ZIP2) halves.
            SimdPermuteOp::Zip1 | SimdPermuteOp::Zip2 => {
                let j = i / 2 + if op == SimdPermuteOp::Zip2 { half } else { 0 };
                if i % 2 == 0 { lane(n, esize, j) } else { lane(m, esize, j) }
            }
        };
        set_lane(&mut out, esize, i, v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dup_and_lanes() {
        assert_eq!(dup(0xAB, 8, true), 0xABAB_ABAB_ABAB_ABAB_ABAB_ABAB_ABAB_ABABu128);
        assert_eq!(dup(0x1234_5678, 32, false), 0x1234_5678_1234_5678u128);
        let mut v = 0u128;
        set_lane(&mut v, 16, 7, 0xBEEF);
        assert_eq!(lane(v, 16, 7), 0xBEEF);
        assert_eq!(v >> 112, 0xBEEF);
    }

    #[test]
    fn pairwise_ops() {
        // n lanes 0..16, m lanes 16..32 (bytes).
        let n = (0..16u128).fold(0, |acc, i| acc | (i << (i * 8)));
        let m = (0..16u128).fold(0, |acc, i| acc | ((i + 16) << (i * 8)));
        let r = three_same(SimdOp3::Addp, 0, n, m, 8, true);
        for i in 0..16 {
            assert_eq!(lane(r, 8, i), (4 * i + 1) as u64);
        }
        let r = three_same(SimdOp3::Uminp, 0, n, m, 8, true);
        for i in 0..16 {
            assert_eq!(lane(r, 8, i), (2 * i) as u64);
        }
    }

    #[test]
    fn compares_and_bitwise() {
        let n = 0x00FF_0000_0000_0010_0000_0000_0000_FF00u128;
        let r = two_misc(SimdOp2::Cmeq0, n, 8, true);
        for i in 0..16 {
            assert_eq!(lane(r, 8, i), if lane(n, 8, i) == 0 { 0xFF } else { 0 });
        }
        assert_eq!(two_misc(SimdOp2::Cmeq0, n, 8, false) >> 64, 0);
        assert_eq!(three_same(SimdOp3::And, 0, u128::MAX, 0xF0F0, 8, true), 0xF0F0);
        assert_eq!(three_same(SimdOp3::Orr, 0, 1 << 100, 1, 8, false), 1);
        assert_eq!(three_same(SimdOp3::Bsl, 0xFF00, 0x1234, 0xABCD, 8, true), 0x12CD);
        assert_eq!(two_misc(SimdOp2::Cnt, 0xFF07, 8, false), 0x0803);
        // REV16/REV32/REV64 (vector)
        let v: u128 = 0x0F0E_0D0C_0B0A_0908_0706_0504_0302_0100;
        assert_eq!(two_misc(SimdOp2::Rev16, v, 8, true), 0x0E0F_0C0D_0A0B_0809_0607_0405_0203_0001);
        assert_eq!(two_misc(SimdOp2::Rev32, v, 8, true), 0x0C0D_0E0F_0809_0A0B_0405_0607_0001_0203);
        assert_eq!(two_misc(SimdOp2::Rev64, v, 8, true), 0x0809_0A0B_0C0D_0E0F_0001_0203_0405_0607);
        assert_eq!(two_misc(SimdOp2::Rev64, v, 32, false), 0x0302_0100_0706_0504);
        assert_eq!(three_same(SimdOp3::Smax, 0, 0x80, 0x7F, 8, false), 0x7F);
        assert_eq!(three_same(SimdOp3::Umax, 0, 0x80, 0x7F, 8, false), 0x80);
    }

    #[test]
    fn permutes() {
        let n = 0x0F0E_0D0C_0B0A_0908_0706_0504_0302_0100u128;
        let m = 0x1F1E_1D1C_1B1A_1918_1716_1514_1312_1110u128;
        let bytes = |v: u128| v.to_le_bytes();
        assert_eq!(bytes(permute(SimdPermuteOp::Zip1, n, m, 8, true))[..4], [0x00, 0x10, 0x01, 0x11]);
        assert_eq!(bytes(permute(SimdPermuteOp::Zip2, n, m, 8, true))[..4], [0x08, 0x18, 0x09, 0x19]);
        assert_eq!(bytes(permute(SimdPermuteOp::Uzp1, n, m, 8, true))[6..10], [0x0C, 0x0E, 0x10, 0x12]);
        assert_eq!(bytes(permute(SimdPermuteOp::Uzp2, n, m, 8, true))[..2], [0x01, 0x03]);
        assert_eq!(bytes(permute(SimdPermuteOp::Trn1, n, m, 8, true))[..4], [0x00, 0x10, 0x02, 0x12]);
        assert_eq!(bytes(permute(SimdPermuteOp::Trn2, n, m, 8, true))[..4], [0x01, 0x11, 0x03, 0x13]);
        assert_eq!(permute(SimdPermuteOp::Zip1, n, m, 8, false) >> 64, 0);
    }

    #[test]
    fn shifts() {
        // SHRN v.8b, v.8h, #4: each halfword 0x0AB0 -> byte 0xAB.
        let n = dup(0x0AB0, 16, true);
        assert_eq!(shrn(0, n, 8, 4, false), 0xABAB_ABAB_ABAB_ABABu128);
        assert_eq!(shrn(0x1111, n, 8, 4, true), 0xABAB_ABAB_ABAB_ABAB_0000_0000_0000_1111u128);
        assert_eq!(shift_imm(dup(0x80, 8, false), 8, 7, false, false, false), dup(1, 8, false));
        assert_eq!(shift_imm(dup(0x80, 8, false), 8, 7, false, true, false), dup(0xFF, 8, false));
        assert_eq!(shift_imm(dup(1, 32, true), 32, 31, true, false, true), dup(0x8000_0000, 32, true));
    }
}

#[cfg(test)]
mod mul_long_tests {
    use super::*;

    #[test]
    fn umull_umlal_smull_low_and_high_halves() {
        // n = [0xFFFF_FFFF, 3 | 5, 7], m = [0xFFFF_FFFF, 2 | 11, 13] (32-bit lanes, low lane first)
        let n = (0xFFFF_FFFFu128) | (3u128 << 32) | (5u128 << 64) | (7u128 << 96);
        let m = (0xFFFF_FFFFu128) | (2u128 << 32) | (11u128 << 64) | (13u128 << 96);
        let lo = mul_long(SimdLongOp::Mull, 0, n, m, 32, false, false);
        assert_eq!(lane(lo, 64, 0), 0xFFFF_FFFFu64 * 0xFFFF_FFFFu64); // full 64-bit product, no truncation
        assert_eq!(lane(lo, 64, 1), 6);
        let hi = mul_long(SimdLongOp::Mull, 0, n, m, 32, true, false); // UMULL2 uses the high half
        assert_eq!((lane(hi, 64, 0), lane(hi, 64, 1)), (55, 91));
        let acc = mul_long(SimdLongOp::Mlal, lo, n, m, 32, false, false); // UMLAL doubles the products (wrapping)
        assert_eq!(lane(acc, 64, 1), 12);
        let sub = mul_long(SimdLongOp::Mlsl, acc, n, m, 32, false, false);
        assert_eq!(lane(sub, 64, 1), 6);
        // signed: -1 * -1 = 1, -1 * 2 = -2
        let s = mul_long(SimdLongOp::Mull, 0, n, m, 32, false, true);
        assert_eq!(lane(s, 64, 0), 1);
        assert_eq!(lane(s, 64, 1), 6);
    }

    #[test]
    fn widening_add_sub_abd() {
        let n = (0xFFFF_FFFFu128) | (3u128 << 32);
        let m = 5u128 | (9u128 << 32);
        let addl = mul_long(SimdLongOp::Addl, 0, n, m, 32, false, false);
        assert_eq!((lane(addl, 64, 0), lane(addl, 64, 1)), (0x1_0000_0004, 12)); // no 32-bit wrap
        let wide_n = 0x1_0000_0000u128 | (7u128 << 64); // two 64-bit lanes
        let addw = mul_long(SimdLongOp::Addw, 0, wide_n, m, 32, false, false); // UADDW: wide + widened narrow
        assert_eq!((lane(addw, 64, 0), lane(addw, 64, 1)), (0x1_0000_0005, 16));
        let subl = mul_long(SimdLongOp::Subl, 0, n, m, 32, false, true); // signed: -1 - 5, 3 - 9
        assert_eq!((lane(subl, 64, 0) as i64, lane(subl, 64, 1) as i64), (-6, -6));
        let abdl = mul_long(SimdLongOp::Abdl, 0, n, m, 32, false, false);
        assert_eq!((lane(abdl, 64, 0), lane(abdl, 64, 1)), (0xFFFF_FFFA, 6));
    }
}

#[cfg(test)]
mod narrow_and_saturating_tests {
    use super::*;

    #[test]
    fn narrowing_high_half_rounding_and_second_half_forms() {
        let n = 0x1234u128 | (0x127Fu128 << 16);
        let m = 0x0111u128 | (0x0001u128 << 16);
        let add = narrow_high(SimdNarrowOp::Addhn, 0, n, m, 8, false);
        assert_eq!((lane(add, 8, 0), lane(add, 8, 1)), (0x13, 0x12)); // 0x1345, 0x1280
        let radd = narrow_high(SimdNarrowOp::Raddhn, 0, n, m, 8, false);
        assert_eq!((lane(radd, 8, 0), lane(radd, 8, 1)), (0x13, 0x13)); // 0x1280 + 0x80 rounds up
        let sub = narrow_high(SimdNarrowOp::Subhn, 0, n, m, 8, false);
        assert_eq!(lane(sub, 8, 0), 0x11); // 0x1234 - 0x0111 = 0x1123
        let rsub = narrow_high(SimdNarrowOp::Rsubhn, 0, 0x1100, 0x0080, 8, false);
        assert_eq!(lane(rsub, 8, 0), 0x11); // 0x1080 + 0x80 = 0x1100
        let old = 0xAABB_CCDD_1122_3344u128;
        let add2 = narrow_high(SimdNarrowOp::Addhn, old, n, m, 8, true); // ADDHN2: low half kept, high half written
        assert_eq!(add2 & ((1u128 << 64) - 1), 0xAABB_CCDD_1122_3344);
        assert_eq!(lane(add2 >> 64, 8, 0), 0x13);
        let wrap = narrow_high(SimdNarrowOp::Addhn, 0, 0xFFFF, 0x0001, 8, false); // sum wraps in 16 bits
        assert_eq!(lane(wrap, 8, 0), 0x00);
    }

    #[test]
    fn saturating_doubling_multiply_long() {
        let q15 = |a: i16, b: i16| (a as u16 as u128 | (b as u16 as u128) << 16, 0u128);
        let (n, _) = q15(0x4000, i16::MIN);
        let (m, _) = q15(0x4000, i16::MIN);
        let (r, qc) = sat_doubling_mul_long(SimdSatLongOp::Sqdmull, 0, n, m, 16, false);
        assert!(qc);
        assert_eq!(lane(r, 32, 0), 0x2000_0000); // 2 * 0x4000 * 0x4000
        assert_eq!(lane(r, 32, 1), 0x7FFF_FFFF); // min * min saturates (2^31 does not fit)
        let acc = 0x7FFF_FFF0u128;
        let mla = sat_doubling_mul_long(SimdSatLongOp::Sqdmlal, acc, 3, 5, 16, false).0; // 0x7FFFFFF0 + 30
        assert_eq!(lane(mla, 32, 0), 0x7FFF_FFFF);
        let mls = sat_doubling_mul_long(SimdSatLongOp::Sqdmlsl, 0x8000_0010, 3, 5, 16, false).0; // -2147483632 - 30 saturates low
        assert_eq!(lane(mls, 32, 0), 0x8000_0000);
        let neg = sat_doubling_mul_long(SimdSatLongOp::Sqdmull, 0, 0xFFFF, 3, 16, false).0; // -1 * 3 * 2
        assert_eq!(lane(neg, 32, 0) as u32 as i32, -6);
        let hi = sat_doubling_mul_long(SimdSatLongOp::Sqdmull, 0, 7u128 << 64, 2u128 << 64, 32, true).0; // SQDMULL2 (32-bit)
        assert_eq!(lane(hi, 64, 0), 28);
    }
}

#[cfg(test)]
mod pmull_tests {
    use super::pmull;

    #[test]
    fn pmull_8bit_lanes() {
        assert_eq!(pmull(0x03, 0x03, false, false), 0x05); // (x+1)^2 = x^2+1
        assert_eq!(pmull(0x80, 0x80, false, false), 0x4000);
        assert_eq!(pmull(0xFF, 0xFF, false, false), 0x5555); // (x^8-1)^2 over GF(2)
        // lane 1 independent: 0x02 x 0x07 = 0x0E; lane 0 zero
        assert_eq!(pmull(0x0200, 0x0700, false, false), 0x0E << 16);
    }

    #[test]
    fn pmull2_uses_high_half() {
        let n = (0x03u128 << 64) | 0xFF;
        let m = (0x03u128 << 64) | 0xFF;
        assert_eq!(pmull(n, m, false, true), 0x05);
        assert_eq!(pmull(n, m, false, false), 0x5555);
        assert_eq!(pmull(n, m, true, true), 0x05);
    }

    #[test]
    fn pmull_64bit() {
        // (x^63 + 1)(x + 1) = x^64 + x^63 + x + 1
        assert_eq!(pmull(0x8000_0000_0000_0001, 3, true, false), (1u128 << 64) | 0x8000_0000_0000_0003);
        assert_eq!(pmull(1u128 << 63, 1u128 << 63, true, false), 1u128 << 126);
        let hi = (0x8000_0000_0000_0001u128 << 64) | 7;
        assert_eq!(pmull(hi, 3u128 << 64, true, true), (1u128 << 64) | 0x8000_0000_0000_0003);
    }
}
