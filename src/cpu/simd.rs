#![allow(dead_code)]

//! Lane arithmetic for the AdvSIMD integer instructions, on 128-bit register values.
//! `esize` is the element size in bits (8, 16, 32 or 64). With `q == false` only the low 64 bits are
//! used and the result's upper 64 bits are zero, as the architecture requires.

use super::instructions::{SimdAcrossOp, SimdOp2, SimdOp3, SimdPermuteOp};

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
            SimdOp2::Not => unreachable!(),
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
