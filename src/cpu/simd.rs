#![allow(dead_code)]

//! Lane arithmetic for the AdvSIMD integer instructions, on 128-bit register values.
//! `esize` is the element size in bits (8, 16, 32 or 64). With `q == false` only the low 64 bits are
//! used and the result's upper 64 bits are zero, as the architecture requires.

use super::instructions::{SimdElemOp, SimdFpElementOp, SimdFpOp2, SimdFpOp3, SimdAcrossOp, SimdLongOp, SimdNarrowOp, SimdOp2, SimdOp3, SimdPermuteOp, SimdSatLongOp, SimdShiftExtOp};

#[inline]
/// Mask of one `esize`-bit lane, as a 128-bit value.
pub fn lane_mask(esize: u32) -> u128 {
    mask(esize) as u128
}

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
    three_same_qc(op, d, n, m, esize, q).0
}

/// Register-shift count of lane `i` of `m`: the low byte as a signed number (negative shifts right).
fn reg_shift(m: u128, esize: u32, i: usize) -> i32 {
    lane(m, esize, i) as u8 as i8 as i32
}

/// `S/URSHL`, `S/UQSHL`, `S/UQRSHL` on one lane. `a` is the raw lane. Returns the lane and whether it saturated.
fn shift_lane(a: u64, sh: i32, esize: u32, signed: bool, round: bool, sat: bool) -> (u64, bool) {
    let v: i128 = if signed { sext(a, esize) as i128 } else { a as i128 };
    if sh >= 0 {
        if sat {
            if v == 0 {
                return (0, false);
            }
            if sh as u32 >= esize {
                return saturate(if v < 0 { i128::MIN / 2 } else { i128::MAX / 2 }, esize, signed);
            }
            return saturate(v << sh, esize, signed);
        }
        let r = if sh as u32 >= esize { 0 } else { (v << sh) as u64 & mask(esize) };
        return (r, false);
    }
    let rs = ((-sh) as u32).min(100);
    let adj = if round { v + (1i128 << (rs - 1)) } else { v };
    ((adj >> rs) as u64 & mask(esize), false)
}

/// [`three_same`] plus whether a saturating form saturated (the value `FPSR.QC` must accumulate).
pub fn three_same_qc(op: SimdOp3, d: u128, n: u128, m: u128, esize: u32, q: bool) -> (u128, bool) {
    let mut qc = false;
    let r = three_same_inner(op, d, n, m, esize, q, &mut qc);
    (r, qc)
}

fn three_same_inner(op: SimdOp3, d: u128, n: u128, m: u128, esize: u32, q: bool, qc: &mut bool) -> u128 {
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
        let (ia, ib) = (sa as i128, sb as i128);
        let (ua, ub) = (a as i128, b as i128);
        let mut sat1 = |v: i128, signed: bool| {
            let (x, f) = saturate(v, esize, signed);
            *qc |= f;
            x
        };
        let r = match op {
            SimdOp3::Shadd => ((ia + ib) >> 1) as u64,
            SimdOp3::Uhadd => ((ua + ub) >> 1) as u64,
            SimdOp3::Srhadd => ((ia + ib + 1) >> 1) as u64,
            SimdOp3::Urhadd => ((ua + ub + 1) >> 1) as u64,
            SimdOp3::Shsub => ((ia - ib) >> 1) as u64,
            SimdOp3::Uhsub => ((ua - ub) >> 1) as u64,
            SimdOp3::Sqadd => sat1(ia + ib, true),
            SimdOp3::Uqadd => sat1(ua + ub, false),
            SimdOp3::Sqsub => sat1(ia - ib, true),
            SimdOp3::Uqsub => sat1(ua - ub, false),
            SimdOp3::Srshl | SimdOp3::Urshl | SimdOp3::Sqshl | SimdOp3::Uqshl | SimdOp3::Sqrshl | SimdOp3::Uqrshl => {
                let signed = matches!(op, SimdOp3::Srshl | SimdOp3::Sqshl | SimdOp3::Sqrshl);
                let round = matches!(op, SimdOp3::Srshl | SimdOp3::Urshl | SimdOp3::Sqrshl | SimdOp3::Uqrshl);
                let sat = matches!(op, SimdOp3::Sqshl | SimdOp3::Uqshl | SimdOp3::Sqrshl | SimdOp3::Uqrshl);
                let (x, f) = shift_lane(a, reg_shift(m, esize, i), esize, signed, round, sat);
                *qc |= f;
                x
            }
            SimdOp3::Sabd => (ia - ib).unsigned_abs() as u64,
            SimdOp3::Uabd => (ua - ub).unsigned_abs() as u64,
            SimdOp3::Saba => lane(d, esize, i).wrapping_add((ia - ib).unsigned_abs() as u64),
            SimdOp3::Uaba => lane(d, esize, i).wrapping_add((ua - ub).unsigned_abs() as u64),
            SimdOp3::Mla => lane(d, esize, i).wrapping_add(a.wrapping_mul(b)),
            SimdOp3::Mls => lane(d, esize, i).wrapping_sub(a.wrapping_mul(b)),
            SimdOp3::Pmul => {
                let mut r = 0u64;
                for k in 0..8 {
                    if (b >> k) & 1 == 1 {
                        r ^= a << k;
                    }
                }
                r & 0xFF
            }
            SimdOp3::Sqdmulh => sat1((2 * ia * ib) >> esize, true),
            SimdOp3::Sqrdmulh => sat1((2 * ia * ib + (1i128 << (esize - 1))) >> esize, true),
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
            SimdOp3::Sshl | SimdOp3::Ushl => {
                let sh = lane(m, esize, i) as u8 as i8 as i32;
                let signed = op == SimdOp3::Sshl;
                if sh >= 0 {
                    if sh as u32 >= esize { 0 } else { a << sh }
                } else if (-sh) as u32 >= esize {
                    if signed && sa < 0 { all } else { 0 }
                } else if signed {
                    (sa >> (-sh)) as u64
                } else {
                    a >> (-sh)
                }
            }
            _ => unreachable!("bitwise ops return above"),
        };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// Scalar three-same form: lane 0 of `n`/`m` only, result in lane 0 of the destination, the rest zeroed.
pub fn scalar_three_same(op: SimdOp3, d: u128, n: u128, m: u128, esize: u32) -> (u128, bool) {
    let keep = mask(esize) as u128;
    let (r, qc) = three_same_qc(op, d & keep, n & keep, m & keep, esize, true);
    (r & keep, qc)
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
            SimdOp2::Clz => (a.leading_zeros() - (64 - esize)) as u64,
            SimdOp2::Cls => {
                let x = if s < 0 { !a & all } else { a };
                (x.leading_zeros() - (64 - esize)) as u64 - 1
            }
            SimdOp2::Rbit => (a as u8).reverse_bits() as u64,
            SimdOp2::Sqabs | SimdOp2::Sqneg | SimdOp2::Suqadd | SimdOp2::Usqadd => {
                unreachable!("saturating forms go through two_misc_sat")
            }
            SimdOp2::Not | SimdOp2::Rev64 | SimdOp2::Rev32 | SimdOp2::Rev16 => unreachable!(),
        };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// Two-register miscellaneous operations with the saturation flag: also the forms that read `d` (`SUQADD`/`USQADD`).
/// Other ops pass through to [`two_misc`] with no saturation. `scalar` keeps only lane 0 of the result and of the source.
pub fn two_misc_sat(op: SimdOp2, d: u128, n: u128, esize: u32, q: bool) -> (u128, bool) {
    if !matches!(op, SimdOp2::Sqabs | SimdOp2::Sqneg | SimdOp2::Suqadd | SimdOp2::Usqadd) {
        return (two_misc(op, n, esize, q), false);
    }
    let mut out = 0u128;
    let mut qc = false;
    for i in 0..lanes(esize, q) {
        let (a, dv) = (lane(n, esize, i), lane(d, esize, i));
        let (sa, sd) = (sext(a, esize) as i128, sext(dv, esize) as i128);
        let (r, f) = match op {
            SimdOp2::Sqabs => saturate(sa.abs(), esize, true),
            SimdOp2::Sqneg => saturate(-sa, esize, true),
            SimdOp2::Suqadd => saturate(sd + a as i128, esize, true),
            _ => saturate(dv as i128 + sa, esize, false),
        };
        qc |= f;
        set_lane(&mut out, esize, i, r);
    }
    (out, qc)
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

macro_rules! fp_lane_ops {
    ($mod_name:ident, $f:ty, $b:ty, $quiet:expr, $sign:expr) => {
        mod $mod_name {
            use super::{SimdFpOp2, SimdFpOp3};
            const QUIET: $b = $quiet;
            const SIGN: $b = $sign;
            const ALL: $b = <$b>::MAX;

            fn is_snan(x: $f) -> bool {
                x.is_nan() && x.to_bits() & QUIET == 0
            }

            /// ARM `FPMax`/`FPMin`: any NaN propagates (first operand first, quieted); -0 orders below +0.
            fn max_min(a: $f, b: $f, max: bool) -> $b {
                // ARM FPProcessNaNs order: a signalling NaN beats a quiet one, then operand order
                if is_snan(a) {
                    return a.to_bits() | QUIET;
                }
                if is_snan(b) {
                    return b.to_bits() | QUIET;
                }
                if a.is_nan() {
                    return a.to_bits() | QUIET;
                }
                if b.is_nan() {
                    return b.to_bits() | QUIET;
                }
                if a == b {
                    // equal values: only the zeros differ in sign
                    let (ab, bb) = (a.to_bits(), b.to_bits());
                    return if max { ab & bb } else { ab | bb };
                }
                (if (a > b) == max { a } else { b }).to_bits()
            }

            /// ARM `FPMaxNum`/`FPMinNum`: a quiet NaN against a number yields the number.
            fn max_min_num(a: $f, b: $f, max: bool) -> $b {
                let (aq, bq) = (a.is_nan() && !is_snan(a), b.is_nan() && !is_snan(b));
                // ARM swaps a quiet NaN for -inf (max) / +inf (min) when the other side is not a quiet NaN
                if aq && !bq {
                    return if b.is_nan() { b.to_bits() | QUIET } else { b.to_bits() };
                }
                if bq && !aq {
                    return if a.is_nan() { a.to_bits() | QUIET } else { a.to_bits() };
                }
                max_min(a, b, max)
            }

            /// ARM `FPProcessNaNs`: the first signalling NaN (quieted), else the first quiet NaN, else `None`.
            /// Done explicitly because Rust may commute `a * b` and `a + b`, which would pick the other NaN.
            fn nan_in(ops: &[$f]) -> Option<$b> {
                if let Some(x) = ops.iter().find(|x| is_snan(**x)) {
                    return Some(x.to_bits() | QUIET);
                }
                ops.iter().find(|x| x.is_nan()).map(|x| x.to_bits())
            }

            pub fn op3(op: SimdFpOp3, acc: $b, n: $b, m: $b) -> $b {
                let (a, b, d) = (<$f>::from_bits(n), <$f>::from_bits(m), <$f>::from_bits(acc));
                let flag = |c: bool| if c { ALL } else { 0 };
                let nan = match op {
                    SimdFpOp3::Fadd | SimdFpOp3::Faddp | SimdFpOp3::Fsub | SimdFpOp3::Fmul | SimdFpOp3::Fmulx | SimdFpOp3::Fdiv => nan_in(&[a, b]),
                    SimdFpOp3::Fabd => nan_in(&[a, b]).map(|r| r & !SIGN),
                    // FPMulAdd: a quiet-NaN addend with 0 * inf is the default NaN; otherwise addend, then operands
                    SimdFpOp3::Fmla | SimdFpOp3::Fmls => {
                        let a = if op == SimdFpOp3::Fmls { -a } else { a };
                        if d.is_nan() && !is_snan(d) && ((a.is_infinite() && b == 0.0) || (a == 0.0 && b.is_infinite())) {
                            Some(<$f>::NAN.to_bits())
                        } else {
                            nan_in(&[d, a, b])
                        }
                    }
                    _ => None,
                };
                if let Some(r) = nan {
                    return r;
                }
                match op {
                    SimdFpOp3::Fadd | SimdFpOp3::Faddp => (a + b).to_bits(),
                    SimdFpOp3::Fsub => (a - b).to_bits(),
                    SimdFpOp3::Fmul => (a * b).to_bits(),
                    SimdFpOp3::Fmulx => {
                        if (a.is_infinite() && b == 0.0) || (a == 0.0 && b.is_infinite()) {
                            (if a.is_sign_negative() != b.is_sign_negative() { -2.0 } else { 2.0 } as $f).to_bits()
                        } else {
                            (a * b).to_bits()
                        }
                    }
                    SimdFpOp3::Fdiv => (a / b).to_bits(),
                    SimdFpOp3::Fabd => (a - b).abs().to_bits(),
                    SimdFpOp3::Fmla => a.mul_add(b, d).to_bits(),
                    SimdFpOp3::Fmls => (-a).mul_add(b, d).to_bits(),
                    SimdFpOp3::Fmax => max_min(a, b, true),
                    SimdFpOp3::Fmin => max_min(a, b, false),
                    SimdFpOp3::Fmaxnm => max_min_num(a, b, true),
                    SimdFpOp3::Fminnm => max_min_num(a, b, false),
                    SimdFpOp3::Fcmeq => flag(a == b),
                    SimdFpOp3::Fcmge => flag(a >= b),
                    SimdFpOp3::Fcmgt => flag(a > b),
                    SimdFpOp3::Facge => flag(a.abs() >= b.abs()),
                    SimdFpOp3::Facgt => flag(a.abs() > b.abs()),
                }
            }

            pub fn op2(op: SimdFpOp2, n: $b) -> $b {
                let a = <$f>::from_bits(n);
                let flag = |c: bool| if c { ALL } else { 0 };
                match op {
                    SimdFpOp2::Fcmeq0 => flag(a == 0.0),
                    SimdFpOp2::Fcmge0 => flag(a >= 0.0),
                    SimdFpOp2::Fcmgt0 => flag(a > 0.0),
                    SimdFpOp2::Fcmle0 => flag(a <= 0.0),
                    SimdFpOp2::Fcmlt0 => flag(a < 0.0),
                    SimdFpOp2::Fabs => n & !SIGN,
                    SimdFpOp2::Fneg => n ^ SIGN,
                    SimdFpOp2::Fsqrt => a.sqrt().to_bits(),
                    SimdFpOp2::Fcvtn | SimdFpOp2::Fcvtl => unreachable!(),
                }
            }
        }
    };
}
fp_lane_ops!(fp32, f32, u32, 1 << 22, 1 << 31);
fp_lane_ops!(fp64, f64, u64, 1 << 51, 1 << 63);

/// FP three-same on 2S/4S (`double == false`) or 2D lanes. The pairwise `FADDP` reads `n` then `m` pairs.
pub fn fp_three_same(op: SimdFpOp3, d: u128, n: u128, m: u128, double: bool, q: bool) -> u128 {
    let esize = if double { 64 } else { 32 };
    let count = lanes(esize, q);
    let mut out = 0u128;
    for i in 0..count {
        let (a, b) = if op == SimdFpOp3::Faddp {
            // lane i of the concatenation m:n reads elements 2i and 2i+1
            let src = if i < count / 2 { n } else { m };
            let j = (i % (count / 2)) * 2;
            (lane(src, esize, j), lane(src, esize, j + 1))
        } else {
            (lane(n, esize, i), lane(m, esize, i))
        };
        let acc = lane(d, esize, i);
        let r = if double { fp64::op3(op, acc, a, b) } else { fp32::op3(op, acc as u32, a as u32, b as u32) as u64 };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// FP two-register misc. `FCVTN`/`FCVTL` are the double/single forms: `q` picks the upper half
/// (`FCVTN2` keeps the low half of `d`; `FCVTL2` reads the upper two singles).
pub fn fp_two_misc(op: SimdFpOp2, d: u128, n: u128, double: bool, q: bool) -> u128 {
    match op {
        SimdFpOp2::Fcvtn => {
            let lo = (f64::from_bits(lane(n, 64, 0)) as f32).to_bits() as u128;
            let hi = (f64::from_bits(lane(n, 64, 1)) as f32).to_bits() as u128;
            let pair = lo | hi << 32;
            if q { (d & u64::MAX as u128) | pair << 64 } else { pair }
        }
        SimdFpOp2::Fcvtl => {
            let base = if q { 2 } else { 0 };
            let a = f32::from_bits(lane(n, 32, base) as u32) as f64;
            let b = f32::from_bits(lane(n, 32, base + 1) as u32) as f64;
            a.to_bits() as u128 | (b.to_bits() as u128) << 64
        }
        _ => {
            let esize = if double { 64 } else { 32 };
            let mut out = 0u128;
            for i in 0..lanes(esize, q) {
                let a = lane(n, esize, i);
                let r = if double { fp64::op2(op, a) } else { fp32::op2(op, a as u32) as u64 };
                set_lane(&mut out, esize, i, r);
            }
            out
        }
    }
}

/// `FMUL/FMLA/FMLS` by element: every lane of `n` times lane `index` of `m` (fused for FMLA/FMLS).
pub fn fp_by_element(op: SimdFpElementOp, d: u128, n: u128, m: u128, index: usize, double: bool, q: bool) -> u128 {
    let esize = if double { 64 } else { 32 };
    let op3 = match op {
        SimdFpElementOp::Fmul => SimdFpOp3::Fmul,
        SimdFpElementOp::Fmulx => SimdFpOp3::Fmulx,
        SimdFpElementOp::Fmla => SimdFpOp3::Fmla,
        SimdFpElementOp::Fmls => SimdFpOp3::Fmls,
    };
    let mut out = 0u128;
    for i in 0..lanes(esize, q) {
        let (a, b, acc) = (lane(n, esize, i), lane(m, esize, index), lane(d, esize, i));
        let r = if double { fp64::op3(op3, acc, a, b) } else { fp32::op3(op3, acc as u32, a as u32, b as u32) as u64 };
        set_lane(&mut out, esize, i, r);
    }
    out
}

/// Integer by-element and dot-product forms. `m` is the whole `Vm`; `index` picks the element (`0xFF`: vector `SDOT`/`UDOT`,
/// whose second operand is the full vector). Returns the result and whether it saturated (`FPSR.QC`).
pub fn by_element(op: SimdElemOp, d: u128, n: u128, m: u128, index: u8, esize: u32, q: bool, scalar: bool) -> (u128, bool) {
    if let SimdElemOp::Dot(signed) = op {
        let mut out = 0u128;
        for i in 0..lanes(32, q) {
            let group = if index == 0xFF { lane(m, 32, i) } else { lane(m, 32, index as usize) };
            let mut acc = lane(d, 32, i) as u32;
            for k in 0..4 {
                let (a, b) = (lane(n, 8, 4 * i + k), (group >> (8 * k)) & 0xFF);
                let prod = if signed { (a as u8 as i8 as i32).wrapping_mul(b as u8 as i8 as i32) } else { (a as u32).wrapping_mul(b as u32) as i32 };
                acc = acc.wrapping_add(prod as u32);
            }
            set_lane(&mut out, 32, i, acc as u64);
        }
        return (out, false);
    }
    let elem = lane(m, esize, index as usize);
    let keep = mask(esize) as u128;
    let mvec = if scalar { elem as u128 } else { dup(elem, esize, true) };
    match op {
        SimdElemOp::Op3(o3) if scalar => scalar_three_same(o3, d, n, mvec, esize),
        SimdElemOp::Op3(o3) => three_same_qc(o3, d, n, mvec, esize, q),
        SimdElemOp::Long(lo, signed) => (mul_long(lo, d, n, mvec, esize, q, signed), false),
        SimdElemOp::SatLong(so) if scalar => {
            let wide = mask(esize * 2) as u128;
            let (r, f) = sat_doubling_mul_long(so, d & wide, n & keep, mvec, esize, false);
            (r & wide, f)
        }
        SimdElemOp::SatLong(so) => sat_doubling_mul_long(so, d, n, mvec, esize, q),
        SimdElemOp::Dot(_) => unreachable!(),
    }
}

/// `TBL`/`TBX`: `table` holds the 1..=4 source registers, `idx` the index vector, `dst` the old destination.
pub fn table_lookup(table: &[u128], idx: u128, dst: u128, is_tbx: bool, q: bool) -> u128 {
    let count = if q { 16 } else { 8 };
    let mut out: u128 = 0;
    for i in 0..count {
        let k = ((idx >> (8 * i)) & 0xFF) as usize;
        let byte = if k < 16 * table.len() {
            (table[k / 16] >> (8 * (k % 16))) & 0xFF
        } else if is_tbx {
            (dst >> (8 * i)) & 0xFF
        } else {
            0
        };
        out |= byte << (8 * i);
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
    fn register_shifts() {
        // USHL .4S: shifts 1, -1, 32, -32 on 0x80000001 lanes; SSHL arithmetic right keeps sign.
        let n = 0x8000_0001u128 | (0x8000_0001u128 << 32) | (0x8000_0001u128 << 64) | (0x8000_0001u128 << 96);
        let m = 1u128 | (0xFFu128 << 32) | (32u128 << 64) | (0xE0u128 << 96);
        let u = three_same(SimdOp3::Ushl, 0, n, m, 32, true);
        assert_eq!(lane(u, 32, 0), 0x0000_0002);
        assert_eq!(lane(u, 32, 1), 0x4000_0000);
        assert_eq!(lane(u, 32, 2), 0);
        assert_eq!(lane(u, 32, 3), 0);
        let s = three_same(SimdOp3::Sshl, 0, n, m, 32, true);
        assert_eq!(lane(s, 32, 1), 0xC000_0000);
        assert_eq!(lane(s, 32, 3), 0xFFFF_FFFF);
        // 64-bit scalar form: shift by -63 of the minimum is all ones for SSHL.
        assert_eq!(lane(three_same(SimdOp3::Sshl, 0, 1u128 << 63, 0xC1, 64, false), 64, 0), u64::MAX);
    }

    #[test]
    fn table_lookup_tbl_tbx() {
        let t0 = (0..16u128).fold(0, |a, i| a | (i << (8 * i)));
        let t1 = (0..16u128).fold(0, |a, i| a | ((0x10 + i) << (8 * i)));
        // lanes 0..6: indices 15, 0, 16, 31, 32 (out of range), 255
        let idx: u128 = 0xFF_20_1F_10_00_0F;
        let dst = u128::MAX;
        // lanes 6.. have index 0 (in range, table byte 0 is 0)
        let tbl = table_lookup(&[t0, t1], idx, dst, false, true);
        assert_eq!(tbl, 0x00_00_1F_10_00_0F);
        let tbx = table_lookup(&[t0, t1], idx, dst, true, true);
        assert_eq!(tbx, 0xFF_FF_1F_10_00_0F);
        // 16-byte table only: indices 16 and 31 are now out of range
        assert_eq!(table_lookup(&[t0], idx, dst, true, true), 0xFF_FF_FF_FF_00_0F);
        // 64-bit form zeroes the upper half
        let one = table_lookup(&[t0, t1], u128::MAX, dst, true, false);
        assert_eq!(one, 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(table_lookup(&[t0], idx, dst, false, false), 0x0F);
    }

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

#[cfg(test)]
mod fp_vector_tests {
    use super::*;

    fn f4(v: [f32; 4]) -> u128 {
        v.iter().rev().fold(0u128, |a, x| a << 32 | x.to_bits() as u128)
    }
    fn f2d(v: [f64; 2]) -> u128 {
        (v[1].to_bits() as u128) << 64 | v[0].to_bits() as u128
    }
    fn u4(v: u128) -> [u32; 4] {
        [0, 1, 2, 3].map(|i| lane(v, 32, i) as u32)
    }

    #[test]
    fn fp_compares_nan_is_false() {
        let nan = f32::NAN;
        let n = f4([1.0, 2.0, nan, -0.0]);
        let m = f4([1.0, 3.0, 1.0, 0.0]);
        let (z, o) = (0, u32::MAX);
        assert_eq!(u4(fp_three_same(SimdFpOp3::Fcmeq, 0, n, m, false, true)), [o, z, z, o]);
        assert_eq!(u4(fp_three_same(SimdFpOp3::Fcmge, 0, n, m, false, true)), [o, z, z, o]);
        assert_eq!(u4(fp_three_same(SimdFpOp3::Fcmgt, 0, n, m, false, true)), [z, z, z, z]);
        assert_eq!(u4(fp_three_same(SimdFpOp3::Fcmgt, 0, m, n, false, true)), [z, o, z, z]);
        // FACGE/FACGT compare magnitudes: |-0.0| >= |0.0|, not >
        assert_eq!(u4(fp_three_same(SimdFpOp3::Facge, 0, n, m, false, true)), [o, z, z, o]);
        assert_eq!(u4(fp_three_same(SimdFpOp3::Facgt, 0, f4([-3.0, 1.0, 0.0, 0.0]), f4([2.0, -1.0, 0.0, 1.0]), false, true)), [o, z, z, z]);
        // 2D, and 2S leaves the upper half zero
        assert_eq!(fp_three_same(SimdFpOp3::Fcmeq, 0, f2d([1.0, f64::NAN]), f2d([1.0, f64::NAN]), true, true), u64::MAX as u128);
        assert_eq!(fp_three_same(SimdFpOp3::Fcmge, 0, f4([1.0, 1.0, 1.0, 1.0]), f4([1.0, 2.0, 0.0, 0.0]), false, false), 0xFFFF_FFFF);
    }

    #[test]
    fn fp_zero_compares_and_unary() {
        let n = f4([0.0, -0.0, -1.5, f32::NAN]);
        let (z, o) = (0, u32::MAX);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fcmeq0, 0, n, false, true)), [o, o, z, z]);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fcmge0, 0, n, false, true)), [o, o, z, z]);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fcmgt0, 0, n, false, true)), [z, z, z, z]);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fcmle0, 0, n, false, true)), [o, o, o, z]);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fcmlt0, 0, n, false, true)), [z, z, o, z]);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fabs, 0, n, false, true))[..3], [0, 0, 1.5f32.to_bits()]);
        assert_eq!(u4(fp_two_misc(SimdFpOp2::Fneg, 0, n, false, true))[..3], [(-0.0f32).to_bits(), 0, 1.5f32.to_bits()]);
        assert_eq!(fp_two_misc(SimdFpOp2::Fsqrt, 0, f2d([9.0, 2.25]), true, true), f2d([3.0, 1.5]));
        assert_eq!(fp_two_misc(SimdFpOp2::Fsqrt, 0, f4([16.0, 4.0, 1.0, 0.0]), false, false), f4([4.0, 2.0, 0.0, 0.0]));
    }

    #[test]
    fn fp_arith_and_fused() {
        let n = f4([1.5, 2.0, 3.0, 4.0]);
        let m = f4([2.0, 0.5, -1.0, 0.25]);
        assert_eq!(fp_three_same(SimdFpOp3::Fmul, 0, n, m, false, true), f4([3.0, 1.0, -3.0, 1.0]));
        assert_eq!(fp_three_same(SimdFpOp3::Fdiv, 0, n, m, false, true), f4([0.75, 4.0, -3.0, 16.0]));
        assert_eq!(fp_three_same(SimdFpOp3::Fsub, 0, n, m, false, true), f4([-0.5, 1.5, 4.0, 3.75]));
        assert_eq!(fp_three_same(SimdFpOp3::Fabd, 0, n, m, false, true), f4([0.5, 1.5, 4.0, 3.75]));
        let acc = f4([10.0, 10.0, 10.0, 10.0]);
        assert_eq!(fp_three_same(SimdFpOp3::Fmla, acc, n, m, false, true), f4([13.0, 11.0, 7.0, 11.0]));
        assert_eq!(fp_three_same(SimdFpOp3::Fmls, acc, n, m, false, true), f4([7.0, 9.0, 13.0, 9.0]));
        // fused: (1 + 2^-12)^2 - (1 + 2^-11) = 2^-24 exactly; an unfused multiply rounds the 2^-24 away
        let x = 1.0f32 + 2f32.powi(-12);
        let r = fp_three_same(SimdFpOp3::Fmla, f4([-(1.0 + 2f32.powi(-11)); 4]), f4([x; 4]), f4([x; 4]), false, true);
        assert_eq!(u4(r)[0], 2f32.powi(-24).to_bits());
        // FADDP: pairs of n then pairs of m
        assert_eq!(fp_three_same(SimdFpOp3::Faddp, 0, n, m, false, true), f4([3.5, 7.0, 2.5, -0.75]));
        assert_eq!(fp_three_same(SimdFpOp3::Fadd, 0, f2d([1.0, 2.0]), f2d([0.5, 0.25]), true, true), f2d([1.5, 2.25]));
    }

    #[test]
    fn fp_max_min_nan_and_zero() {
        let nan = f32::from_bits(0x7FC0_0001);
        let snan = f32::from_bits(0x7F80_0001);
        let n = f4([nan, 1.0, -0.0, 2.0]);
        let m = f4([1.0, nan, 0.0, 3.0]);
        let max = u4(fp_three_same(SimdFpOp3::Fmax, 0, n, m, false, true));
        assert_eq!(max, [nan.to_bits(), nan.to_bits(), 0.0f32.to_bits(), 3.0f32.to_bits()]);
        let min = u4(fp_three_same(SimdFpOp3::Fmin, 0, n, m, false, true));
        assert_eq!(min, [nan.to_bits(), nan.to_bits(), (-0.0f32).to_bits(), 2.0f32.to_bits()]);
        // MAXNM/MINNM return the number against a quiet NaN
        let maxnm = u4(fp_three_same(SimdFpOp3::Fmaxnm, 0, n, m, false, true));
        assert_eq!(maxnm, [1.0f32.to_bits(), 1.0f32.to_bits(), 0.0f32.to_bits(), 3.0f32.to_bits()]);
        let minnm = u4(fp_three_same(SimdFpOp3::Fminnm, 0, n, m, false, true));
        assert_eq!(minnm, [1.0f32.to_bits(), 1.0f32.to_bits(), (-0.0f32).to_bits(), 2.0f32.to_bits()]);
        // a signalling NaN beats a quiet NaN in the other operand, even in the second position
        let r = fp_three_same(SimdFpOp3::Fmax, 0, f4([nan; 4]), f4([snan; 4]), false, true);
        assert_eq!(u4(r)[0], snan.to_bits() | 1 << 22);
        // a signalling NaN is not dodged by MAXNM: it is quieted and returned
        let r = fp_three_same(SimdFpOp3::Fmaxnm, 0, f4([snan; 4]), f4([1.0; 4]), false, true);
        assert_eq!(u4(r)[0], snan.to_bits() | 1 << 22);
    }

    #[test]
    fn fp_convert_vector() {
        // FCVTZS/FCVTZU truncate toward zero, saturate and map NaN to 0
        let n = f4([-1.9, 2.7, f32::NAN, 3e10]);
        assert_eq!(u4(two_misc(SimdOp2::Fcvtzs, n, 32, true)), [(-1i32) as u32, 2, 0, i32::MAX as u32]);
        assert_eq!(u4(two_misc(SimdOp2::Fcvtzu, n, 32, true)), [0, 2, 0, u32::MAX]);
        // FCVTN/FCVTL double <-> single
        assert_eq!(fp_two_misc(SimdFpOp2::Fcvtn, 0, f2d([1.5, -2.25]), true, false), f4([1.5, -2.25, 0.0, 0.0]));
        let keep = f4([7.0, 8.0, 0.0, 0.0]);
        assert_eq!(fp_two_misc(SimdFpOp2::Fcvtn, keep, f2d([1.5, -2.25]), true, true), f4([7.0, 8.0, 1.5, -2.25]));
        assert_eq!(fp_two_misc(SimdFpOp2::Fcvtl, 0, f4([1.5, -2.25, 3.0, 4.0]), true, false), f2d([1.5, -2.25]));
        assert_eq!(fp_two_misc(SimdFpOp2::Fcvtl, 0, f4([1.5, -2.25, 3.0, 4.0]), true, true), f2d([3.0, 4.0]));
    }
}

/// Saturates `v` to a `bits`-wide signed or unsigned integer. Returns the stored lane value (low `bits`
/// bits) and whether saturation happened. The bool is exactly what the cumulative saturation flag
/// (`FPSR.QC`) must record; every saturating narrow/shift below goes through this one function, so wiring
/// QC means collecting this bool in `shift_ext` and its caller.
pub fn saturate(v: i128, bits: u32, signed: bool) -> (u64, bool) {
    let (lo, hi) = if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };
    let c = v.clamp(lo, hi);
    (c as u64 & mask(bits), c != v)
}

/// Integer shift-by-immediate forms with accumulate, rounding, saturation or insertion, and the
/// saturating/rounding narrowing shifts (`SQXTN`/`UQXTN`/`SQXTUN` are the zero-shift narrows).
/// For the narrowing ops `esize` is the result lane size (source lanes are `2 * esize`) and `q` selects
/// the `2` form, which keeps the low half of `d`. Otherwise lanes are `esize` wide. Rounding adds
/// `1 << (shift - 1)` before the shift.
pub fn shift_ext(op: SimdShiftExtOp, d: u128, n: u128, esize: u32, shift: u32, q: bool) -> u128 {
    shift_ext_flag(op, d, n, esize, shift, q).0
}

/// Bits of the source register a scalar `shift_ext` form reads: lane 0, which is twice the result width for
/// the narrowing forms. Used so lanes a scalar instruction never reads cannot set `FPSR.QC`.
pub fn scalar_source_mask(op: SimdShiftExtOp, esize: u32) -> u128 {
    use SimdShiftExtOp::*;
    let narrowing = matches!(op, Rshrn | Sqshrn | Uqshrn | Sqshrun | Sqrshrn | Uqrshrn | Sqrshrun);
    let width = if narrowing { esize * 2 } else { esize };
    if width >= 128 { u128::MAX } else { (1u128 << width) - 1 }
}

/// [`shift_ext`] plus whether any lane saturated: the value `FPSR.QC` must accumulate.
pub fn shift_ext_flag(op: SimdShiftExtOp, d: u128, n: u128, esize: u32, shift: u32, q: bool) -> (u128, bool) {
    use SimdShiftExtOp::*;
    let mut sat = false;
    let round = matches!(op, Srshr | Urshr | Srsra | Ursra | Rshrn | Sqrshrn | Uqrshrn | Sqrshrun);
    let rnd = |v: i128| if round && shift > 0 { v + (1i128 << (shift - 1)) } else { v };
    if matches!(op, Rshrn | Sqshrn | Uqshrn | Sqshrun | Sqrshrn | Uqrshrn | Sqrshrun) {
        let signed_in = matches!(op, Sqshrn | Sqshrun | Sqrshrn | Sqrshrun);
        let mut half = 0u128;
        for i in 0..64 / esize as usize {
            let a = lane(n, esize * 2, i);
            let a = if signed_in { sext(a, esize * 2) as i128 } else { a as i128 };
            let v = rnd(a) >> shift;
            let r = match op {
                Rshrn => v as u64,
                Sqshrn | Sqrshrn => { let (x, f) = saturate(v, esize, true); sat |= f; x }
                _ => { let (x, f) = saturate(v, esize, false); sat |= f; x }
            };
            set_lane(&mut half, esize, i, r);
        }
        return (if q { (d & u64::MAX as u128) | (half << 64) } else { half }, sat);
    }
    let ones = mask(esize) as u128;
    let mut out = 0u128;
    for i in 0..lanes(esize, q) {
        let raw = lane(n, esize, i);
        let sv = sext(raw, esize) as i128;
        let uv = raw as i128;
        let dv = lane(d, esize, i);
        let r = match op {
            Ssra | Srsra => dv.wrapping_add((rnd(sv) >> shift) as u64),
            Usra | Ursra => dv.wrapping_add((rnd(uv) >> shift) as u64),
            Srshr => (rnd(sv) >> shift) as u64,
            Urshr => (rnd(uv) >> shift) as u64,
            Sri => {
                let m = if shift >= esize { 0 } else { ones >> shift };
                ((dv as u128 & !m) | ((uv as u128 >> shift.min(127)) & m)) as u64
            }
            Sli => {
                let m = (ones << shift) & ones;
                ((dv as u128 & !m) | ((uv as u128) << shift & m)) as u64
            }
            Sqshl => { let (x, f) = saturate(sv << shift, esize, true); sat |= f; x }
            Uqshl => { let (x, f) = saturate(uv << shift, esize, false); sat |= f; x }
            Sqshlu => { let (x, f) = saturate(sv << shift, esize, false); sat |= f; x }
            _ => unreachable!(),
        };
        set_lane(&mut out, esize, i, r);
    }
    (out, sat)
}

/// `SADDLP`/`UADDLP` (and with `acc`, `SADALP`/`UADALP`): each pair of adjacent `esize` lanes of `n` is
/// widened and added into one `2 * esize` lane; `acc` adds the existing lane of `d`.
pub fn pairwise_long(d: u128, n: u128, esize: u32, q: bool, signed: bool, acc: bool) -> u128 {
    let mut out = 0u128;
    for i in 0..lanes(esize, q) / 2 {
        let ext = |v: u64| if signed { sext(v, esize) as i128 } else { v as i128 };
        let mut sum = ext(lane(n, esize, 2 * i)) + ext(lane(n, esize, 2 * i + 1));
        if acc {
            sum += lane(d, esize * 2, i) as i128;
        }
        set_lane(&mut out, esize * 2, i, sum as u64);
    }
    out
}

#[cfg(test)]
mod fuzz_nan_order {
    use super::*;

    fn v4(l: [u32; 4]) -> u128 {
        l.iter().enumerate().fold(0, |a, (i, x)| a | (*x as u128) << (32 * i))
    }

    /// Found by the instruction fuzzer (`fmul v6.4s, v11.4s, v29.s[3]`, checked against Apple Silicon):
    /// FMUL by element let the compiler pick which NaN operand propagates. ARM propagates the first
    /// operand's NaN, a signalling NaN beating a quiet one.
    #[test]
    fn fmul_by_element_propagates_first_operand_nan() {
        let n = v4([0xffff_ffff, 0x7fc0_0001, 0x3f80_0000, 0x7fc0_0001]);
        let m = v4([0x7fff_ffff, 0x7f80_0001, 0x7fff_ffff, 0x7fff_ffff]);
        let r = fp_by_element(SimdFpElementOp::Fmul, 0, n, m, 0, false, true);
        let lanes: Vec<u32> = (0..4).map(|i| (r >> (32 * i)) as u32).collect();
        // element 0 of m is a quiet NaN: both quiet -> n's; only m a NaN (lane 2) -> m's
        assert_eq!(lanes, [0xffff_ffff, 0x7fc0_0001, 0x7fff_ffff, 0x7fc0_0001]);
        // a signalling NaN in the element beats a quiet NaN in n
        let m = v4([0x7f80_0001, 0, 0, 0]);
        let r = fp_by_element(SimdFpElementOp::Fmul, 0, n, m, 0, false, true);
        assert_eq!(r as u32, 0x7fc0_0001);
    }

    #[test]
    fn fmla_by_element_propagates_addend_nan_first() {
        let d = v4([0x7fc0_0002, 0, 0, 0]);
        let n = v4([0x7fc0_0001, 0x3f80_0000, 0, 0]);
        let m = v4([0x7fc0_0003, 0, 0, 0]);
        let r = fp_by_element(SimdFpElementOp::Fmla, d, n, m, 0, false, true);
        assert_eq!(r as u32, 0x7fc0_0002);
    }
}
