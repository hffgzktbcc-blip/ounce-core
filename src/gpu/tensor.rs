#![allow(dead_code)]

//! Warp-level matrix multiply-accumulate (tensor core) modelling for the
//! Ampere GA10B GPU (`SM 8.7`) in the Tegra T239 "Drake" SoC.
//!
//! # Where the behaviour in this file comes from
//!
//! NVIDIA does **not** publish a SASS-level encoding for the `HMMA`
//! instruction family, so nothing here claims to decode real hardware
//! instruction bytes. The *semantics* modelled below are taken from public
//! documentation only:
//!
//! * The PTX ISA reference for `mma.sync.aligned.m16n8k8.row.col.f32.f16.f16.f32`
//!   and `mma.sync.aligned.m16n8k16.row.col.f32.f16.f16.f32` — the two shapes
//!   that Ampere exposes as `HMMA.1688` and `HMMA.16816`: FP16 multiplicands,
//!   FP32 accumulator and FP32 destination, issued once per 32-lane warp.
//! * The per-lane matrix fragment layouts documented alongside those PTX
//!   instructions, expressed there in terms of
//!   `groupID = laneid >> 2` and `threadID_in_group = laneid % 4`.
//! * IEEE 754-2008 binary16 for the element format, with round-to-nearest-even
//!   conversion.
//!
//! The numeric opcodes used by [`crate::gpu::shader`] to carry these
//! operations are this project's own synthetic 64-bit test encoding (the same
//! scheme as every other `SassOp`), not Ampere instruction words.
//!
//! # Deliberate limits
//!
//! * Products are evaluated in `f32`. An `f16 * f16` product is exact in
//!   `f32`, so only the accumulation rounds; accumulation runs in increasing
//!   `k` order. Real hardware is free to use a wider internal accumulator and
//!   an unspecified reduction order, so bit-exact agreement with silicon is
//!   not claimed for long `k` reductions.
//! * Only the `.row.col` operand arrangement and the `f32` accumulate variant
//!   are modelled. `f16` accumulate, sparsity, and the integer/`bf16`/`tf32`
//!   shapes are out of scope.

use thiserror::Error;

/// Lanes in one Ampere warp. One `HMMA` is issued per warp.
pub const WARP_LANES: usize = 32;

/// Largest per-lane `A` fragment register count across the shapes modelled here.
pub const MAX_A_REGS: usize = 8;
/// Largest per-lane `B` fragment register count across the shapes modelled here.
pub const MAX_B_REGS: usize = 4;
/// Per-lane `C`/`D` fragment register count (identical for both shapes).
pub const CD_REGS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MmaError {
    #[error("matrix A needs {expected} half-precision elements, got {got}")]
    BadA { expected: usize, got: usize },
    #[error("matrix B needs {expected} half-precision elements, got {got}")]
    BadB { expected: usize, got: usize },
    #[error("accumulator C needs {expected} f32 elements, got {got}")]
    BadC { expected: usize, got: usize },
}

// ---------------------------------------------------------------------------
// IEEE 754-2008 binary16
// ---------------------------------------------------------------------------

/// Converts IEEE 754-2008 binary16 bits to `f32`. Exact for every input.
pub fn f16_to_f32(half: u16) -> f32 {
    let sign_neg = (half & 0x8000) != 0;
    let exp = ((half >> 10) & 0x1F) as u32;
    let mant = (half & 0x03FF) as u32;

    if exp == 0 {
        // Zero or subnormal: value = mant * 2^-24, exactly representable.
        let magnitude = (mant as f32) / 16_777_216.0;
        return if sign_neg { -magnitude } else { magnitude };
    }

    let sign_bits = if sign_neg { 0x8000_0000u32 } else { 0 };
    let bits = if exp == 0x1F {
        // Infinity or NaN; the payload is shifted into the f32 mantissa.
        sign_bits | 0x7F80_0000 | (mant << 13)
    } else {
        // Re-bias: half bias 15 -> single bias 127.
        sign_bits | ((exp + 112) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// Converts `f32` to IEEE 754-2008 binary16 bits, rounding to nearest even.
///
/// Overflow saturates to infinity and underflow to a signed zero, matching the
/// IEEE default rounding attribute.
pub fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x007F_FFFF;

    if exp == 0xFF {
        // Infinity passes through; any NaN becomes a quiet NaN.
        return if mant != 0 { sign | 0x7E00 } else { sign | 0x7C00 };
    }

    let unbiased = exp - 127;
    if unbiased > 15 {
        return sign | 0x7C00;
    }

    if unbiased >= -14 {
        // Normal half. 13 of the 23 single-precision mantissa bits are dropped.
        let half_exp = ((unbiased + 15) as u16) << 10;
        let half_mant = (mant >> 13) as u16;
        let mut half = sign | half_exp | half_mant;
        let discarded = mant & 0x1FFF;
        // Carrying out of the mantissa increments the exponent, which is the
        // correct behaviour here, including the step up to infinity.
        if discarded > 0x1000 || (discarded == 0x1000 && (half_mant & 1) != 0) {
            half += 1;
        }
        return half;
    }

    if unbiased < -25 {
        return sign;
    }

    // Subnormal half: restore the implicit bit and shift into place.
    let mant_with_implicit = mant | 0x0080_0000;
    let shift = (-unbiased - 1) as u32; // 14..=24 for this range
    let half_mant = (mant_with_implicit >> shift) as u16;
    let discarded = mant_with_implicit & ((1u32 << shift) - 1);
    let halfway = 1u32 << (shift - 1);
    let mut half = sign | half_mant;
    if discarded > halfway || (discarded == halfway && (half_mant & 1) != 0) {
        half += 1;
    }
    half
}

/// Converts a slice of `f32` to binary16 bits, for building test vectors.
pub fn f32_slice_to_f16(values: &[f32]) -> Vec<u16> {
    values.iter().copied().map(f32_to_f16).collect()
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

/// The two FP16 MMA shapes Ampere exposes as `HMMA.1688` / `HMMA.16816`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HmmaShape {
    /// `m16n8k8`: `A` is 16x8, `B` is 8x8, `C`/`D` are 16x8.
    M16N8K8,
    /// `m16n8k16`: `A` is 16x16, `B` is 16x8, `C`/`D` are 16x8.
    M16N8K16,
}

impl HmmaShape {
    pub const fn m(self) -> usize {
        16
    }

    pub const fn n(self) -> usize {
        8
    }

    pub const fn k(self) -> usize {
        match self {
            HmmaShape::M16N8K8 => 8,
            HmmaShape::M16N8K16 => 16,
        }
    }

    /// `A` fragment registers held by each lane (one binary16 element each).
    pub const fn a_regs_per_lane(self) -> usize {
        match self {
            HmmaShape::M16N8K8 => 4,
            HmmaShape::M16N8K16 => 8,
        }
    }

    /// `B` fragment registers held by each lane (one binary16 element each).
    pub const fn b_regs_per_lane(self) -> usize {
        match self {
            HmmaShape::M16N8K8 => 2,
            HmmaShape::M16N8K16 => 4,
        }
    }

    /// `C`/`D` fragment registers held by each lane (one `f32` each).
    pub const fn cd_regs_per_lane(self) -> usize {
        CD_REGS
    }

    /// Mnemonic as the SASS disassembler spells it.
    pub const fn mnemonic(self) -> &'static str {
        match self {
            HmmaShape::M16N8K8 => "HMMA.1688.F32.F16",
            HmmaShape::M16N8K16 => "HMMA.16816.F32.F16",
        }
    }

    /// The equivalent public PTX instruction this shape models.
    pub const fn ptx_mnemonic(self) -> &'static str {
        match self {
            HmmaShape::M16N8K8 => "mma.sync.aligned.m16n8k8.row.col.f32.f16.f16.f32",
            HmmaShape::M16N8K16 => "mma.sync.aligned.m16n8k16.row.col.f32.f16.f16.f32",
        }
    }
}

// ---------------------------------------------------------------------------
// Fragment layouts (public PTX per-lane mapping)
// ---------------------------------------------------------------------------

/// `(row, col)` of each `A` fragment register held by `lane`, in register order.
///
/// `A` is row-major `m x k`.
pub fn lane_a_coords(shape: HmmaShape, lane: usize) -> Vec<(usize, usize)> {
    let group = (lane % WARP_LANES) / 4;
    let tig = (lane % WARP_LANES) % 4;
    let col = tig * 2;
    let mut coords = Vec::with_capacity(shape.a_regs_per_lane());
    // a0, a1, a2, a3
    coords.push((group, col));
    coords.push((group, col + 1));
    coords.push((group + 8, col));
    coords.push((group + 8, col + 1));
    if shape == HmmaShape::M16N8K16 {
        // a4..a7 repeat the pattern in the second half of the k dimension.
        coords.push((group, col + 8));
        coords.push((group, col + 9));
        coords.push((group + 8, col + 8));
        coords.push((group + 8, col + 9));
    }
    coords
}

/// `(row, col)` of each `B` fragment register held by `lane`, in register order.
///
/// `B` is column-major `k x n` (the `col` half of the `.row.col` arrangement).
pub fn lane_b_coords(shape: HmmaShape, lane: usize) -> Vec<(usize, usize)> {
    let group = (lane % WARP_LANES) / 4;
    let tig = (lane % WARP_LANES) % 4;
    let row = tig * 2;
    let mut coords = Vec::with_capacity(shape.b_regs_per_lane());
    coords.push((row, group));
    coords.push((row + 1, group));
    if shape == HmmaShape::M16N8K16 {
        coords.push((row + 8, group));
        coords.push((row + 9, group));
    }
    coords
}

/// `(row, col)` of each `C`/`D` fragment register held by `lane`.
///
/// `C` and `D` are row-major `m x n` `f32`, and the layout is identical for
/// both shapes.
pub fn lane_cd_coords(lane: usize) -> [(usize, usize); CD_REGS] {
    let group = (lane % WARP_LANES) / 4;
    let tig = (lane % WARP_LANES) % 4;
    let col = tig * 2;
    [
        (group, col),
        (group, col + 1),
        (group + 8, col),
        (group + 8, col + 1),
    ]
}

// ---------------------------------------------------------------------------
// The MMA itself
// ---------------------------------------------------------------------------

/// Executes `D = A * B + C` for one warp-wide `HMMA`.
///
/// `a` is row-major `m x k` binary16, `b` is column-major `k x n` binary16
/// (indexed `b[n * k + k_idx]`), and `c` is row-major `m x n` `f32`. The
/// result is row-major `m x n` `f32`.
pub fn mma_f16_f32(
    shape: HmmaShape,
    a: &[u16],
    b: &[u16],
    c: &[f32],
) -> Result<Vec<f32>, MmaError> {
    let (m, n, k) = (shape.m(), shape.n(), shape.k());
    if a.len() != m * k {
        return Err(MmaError::BadA {
            expected: m * k,
            got: a.len(),
        });
    }
    if b.len() != k * n {
        return Err(MmaError::BadB {
            expected: k * n,
            got: b.len(),
        });
    }
    if c.len() != m * n {
        return Err(MmaError::BadC {
            expected: m * n,
            got: c.len(),
        });
    }

    let mut d = vec![0.0f32; m * n];
    for row in 0..m {
        for col in 0..n {
            let mut acc = c[row * n + col];
            for k_idx in 0..k {
                let lhs = f16_to_f32(a[row * k + k_idx]);
                let rhs = f16_to_f32(b[col * k + k_idx]);
                acc += lhs * rhs;
            }
            d[row * n + col] = acc;
        }
    }
    Ok(d)
}

/// A warp's worth of per-lane `HMMA` operands, as the lanes actually hold them.
#[derive(Debug, Clone, PartialEq)]
pub struct WarpFragments {
    pub shape: HmmaShape,
    pub a: [[u16; MAX_A_REGS]; WARP_LANES],
    pub b: [[u16; MAX_B_REGS]; WARP_LANES],
    pub c: [[f32; CD_REGS]; WARP_LANES],
}

impl WarpFragments {
    /// Distributes full matrices into per-lane fragments using the documented
    /// PTX layout. Registers a lane does not own for this shape stay zero.
    pub fn distribute(
        shape: HmmaShape,
        a: &[u16],
        b: &[u16],
        c: &[f32],
    ) -> Result<Self, MmaError> {
        let (m, n, k) = (shape.m(), shape.n(), shape.k());
        if a.len() != m * k {
            return Err(MmaError::BadA {
                expected: m * k,
                got: a.len(),
            });
        }
        if b.len() != k * n {
            return Err(MmaError::BadB {
                expected: k * n,
                got: b.len(),
            });
        }
        if c.len() != m * n {
            return Err(MmaError::BadC {
                expected: m * n,
                got: c.len(),
            });
        }

        let mut frags = Self {
            shape,
            a: [[0u16; MAX_A_REGS]; WARP_LANES],
            b: [[0u16; MAX_B_REGS]; WARP_LANES],
            c: [[0.0f32; CD_REGS]; WARP_LANES],
        };
        for lane in 0..WARP_LANES {
            for (reg, (row, col)) in lane_a_coords(shape, lane).into_iter().enumerate() {
                frags.a[lane][reg] = a[row * k + col];
            }
            for (reg, (row, col)) in lane_b_coords(shape, lane).into_iter().enumerate() {
                // B is column-major k x n.
                frags.b[lane][reg] = b[col * k + row];
            }
            for (reg, (row, col)) in lane_cd_coords(lane).into_iter().enumerate() {
                frags.c[lane][reg] = c[row * n + col];
            }
        }
        Ok(frags)
    }

    /// Reassembles full matrices from the fragments, then executes the MMA and
    /// scatters `D` back into per-lane registers.
    pub fn execute(&self) -> Result<[[f32; CD_REGS]; WARP_LANES], MmaError> {
        let (m, n, k) = (self.shape.m(), self.shape.n(), self.shape.k());
        let mut a = vec![0u16; m * k];
        let mut b = vec![0u16; k * n];
        let mut c = vec![0.0f32; m * n];

        for lane in 0..WARP_LANES {
            for (reg, (row, col)) in lane_a_coords(self.shape, lane).into_iter().enumerate() {
                a[row * k + col] = self.a[lane][reg];
            }
            for (reg, (row, col)) in lane_b_coords(self.shape, lane).into_iter().enumerate() {
                b[col * k + row] = self.b[lane][reg];
            }
            for (reg, (row, col)) in lane_cd_coords(lane).into_iter().enumerate() {
                c[row * n + col] = self.c[lane][reg];
            }
        }

        let d = mma_f16_f32(self.shape, &a, &b, &c)?;

        let mut d_frags = [[0.0f32; CD_REGS]; WARP_LANES];
        for lane in 0..WARP_LANES {
            for (reg, (row, col)) in lane_cd_coords(lane).into_iter().enumerate() {
                d_frags[lane][reg] = d[row * n + col];
            }
        }
        Ok(d_frags)
    }
}

/// Collects a row-major `m x n` `D` matrix from per-lane fragments.
pub fn collect_d(shape: HmmaShape, d_frags: &[[f32; CD_REGS]; WARP_LANES]) -> Vec<f32> {
    let n = shape.n();
    let mut d = vec![0.0f32; shape.m() * n];
    for lane in 0..WARP_LANES {
        for (reg, (row, col)) in lane_cd_coords(lane).into_iter().enumerate() {
            d[row * n + col] = d_frags[lane][reg];
        }
    }
    d
}

/// A software tensor core unit: issues `HMMA` operations and counts them, so
/// the verification harness can report tensor core throughput.
#[derive(Debug, Clone, Default)]
pub struct TensorCoreUnit {
    pub issued_ops: u64,
    /// Multiply-accumulate lane operations retired (`m * n * k` per issue).
    pub retired_macs: u64,
}

impl TensorCoreUnit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn issue(
        &mut self,
        shape: HmmaShape,
        a: &[u16],
        b: &[u16],
        c: &[f32],
    ) -> Result<Vec<f32>, MmaError> {
        let d = mma_f16_f32(shape, a, b, c)?;
        self.issued_ops += 1;
        self.retired_macs += (shape.m() * shape.n() * shape.k()) as u64;
        Ok(d)
    }

    /// Issues one warp-wide `HMMA` from per-lane fragments.
    pub fn issue_warp(
        &mut self,
        frags: &WarpFragments,
    ) -> Result<[[f32; CD_REGS]; WARP_LANES], MmaError> {
        let d = frags.execute()?;
        self.issued_ops += 1;
        self.retired_macs += (frags.shape.m() * frags.shape.n() * frags.shape.k()) as u64;
        Ok(d)
    }
}

/// Builds a `k x n` column-major identity matrix in binary16, padded with
/// zeros when `k != n`. Useful as a documented MMA test vector: with `B = I`,
/// `D` must equal the leading `m x n` block of `A` plus `C`.
pub fn identity_b(shape: HmmaShape) -> Vec<u16> {
    let (n, k) = (shape.n(), shape.k());
    let mut b = vec![0u16; k * n];
    for col in 0..n {
        if col < k {
            b[col * k + col] = 0x3C00; // 1.0h
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary16_conversion_reference_vectors() {
        // Documented IEEE 754-2008 binary16 bit patterns.
        assert_eq!(f32_to_f16(0.0), 0x0000);
        assert_eq!(f32_to_f16(-0.0), 0x8000);
        assert_eq!(f32_to_f16(1.0), 0x3C00);
        assert_eq!(f32_to_f16(-2.0), 0xC000);
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(2.0), 0x4000);
        assert_eq!(f32_to_f16(65504.0), 0x7BFF); // largest finite half
        assert_eq!(f32_to_f16(65536.0), 0x7C00); // overflow -> +inf
        assert_eq!(f32_to_f16(6.103515625e-5), 0x0400); // smallest normal
        assert_eq!(f32_to_f16(5.960464477539063e-8), 0x0001); // smallest subnormal

        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0x7BFF), 65504.0);
        assert_eq!(f16_to_f32(0x0400), 6.103515625e-5);
        assert_eq!(f16_to_f32(0x0001), 5.960464477539063e-8);
        assert!(f16_to_f32(0x7C00).is_infinite());
        assert!(f16_to_f32(0xFE00).is_nan());

        // Round to nearest even on the tie between 1.0h and the next half.
        // 1.0 + 2^-11 is exactly halfway; the even neighbour is 1.0 (0x3C00).
        assert_eq!(f32_to_f16(1.0 + 0.000_488_281_25), 0x3C00);
        // 1.0 + 3 * 2^-12 is three quarters of the way to the next half and
        // rounds up to 0x3C01.
        assert_eq!(f32_to_f16(1.0 + 0.000_732_421_875), 0x3C01);
        // 1.0 + 3 * 2^-11 is halfway between 0x3C01 and 0x3C02; the even
        // neighbour is 0x3C02.
        assert_eq!(f32_to_f16(1.0 + 0.001_464_843_75), 0x3C02);

        // Every representable half must survive a round trip.
        for bits in 0..=u16::MAX {
            let exp = (bits >> 10) & 0x1F;
            if exp == 0x1F {
                continue; // inf / NaN payloads are not bit-preserved
            }
            assert_eq!(f32_to_f16(f16_to_f32(bits)), bits, "bits {bits:#06X}");
        }
    }

    #[test]
    fn test_fragment_layouts_cover_every_element_once() {
        for shape in [HmmaShape::M16N8K8, HmmaShape::M16N8K16] {
            let (m, n, k) = (shape.m(), shape.n(), shape.k());

            let mut a_hits = vec![0u32; m * k];
            let mut b_hits = vec![0u32; k * n];
            let mut cd_hits = vec![0u32; m * n];
            for lane in 0..WARP_LANES {
                let a = lane_a_coords(shape, lane);
                let b = lane_b_coords(shape, lane);
                assert_eq!(a.len(), shape.a_regs_per_lane());
                assert_eq!(b.len(), shape.b_regs_per_lane());
                for (row, col) in a {
                    a_hits[row * k + col] += 1;
                }
                for (row, col) in b {
                    b_hits[col * k + row] += 1;
                }
                for (row, col) in lane_cd_coords(lane) {
                    cd_hits[row * n + col] += 1;
                }
            }
            assert!(a_hits.iter().all(|&h| h == 1), "A layout for {shape:?}");
            assert!(b_hits.iter().all(|&h| h == 1), "B layout for {shape:?}");
            assert!(cd_hits.iter().all(|&h| h == 1), "C/D layout for {shape:?}");
        }
    }

    #[test]
    fn test_mma_identity_and_ones_reference_vectors() {
        for shape in [HmmaShape::M16N8K8, HmmaShape::M16N8K16] {
            let (m, n, k) = (shape.m(), shape.n(), shape.k());

            // B = I: D must be the leading m x n block of A, plus C.
            let a: Vec<u16> = (0..m * k)
                .map(|i| f32_to_f16(((i % 7) as f32) - 3.0))
                .collect();
            let c: Vec<f32> = (0..m * n).map(|i| (i % 5) as f32 * 0.25).collect();
            let d = mma_f16_f32(shape, &a, &identity_b(shape), &c).unwrap();
            for row in 0..m {
                for col in 0..n {
                    let expected = f16_to_f32(a[row * k + col]) + c[row * n + col];
                    assert_eq!(d[row * n + col], expected, "{shape:?} ({row},{col})");
                }
            }

            // A and B all ones, C zero: every D element is exactly k.
            let ones_a = vec![0x3C00u16; m * k];
            let ones_b = vec![0x3C00u16; k * n];
            let zero_c = vec![0.0f32; m * n];
            let d = mma_f16_f32(shape, &ones_a, &ones_b, &zero_c).unwrap();
            assert!(d.iter().all(|&v| v == k as f32), "{shape:?} ones");
        }
    }

    #[test]
    fn test_mma_accumulates_in_f32_beyond_half_precision() {
        // 2048.0 is the last integer with an exact binary16 neighbour at 1.0
        // spacing; 2048 + 1 is not representable in binary16 but is exact in
        // the f32 accumulator, which is the point of the .f32 accumulate form.
        let shape = HmmaShape::M16N8K8;
        let (m, n, k) = (shape.m(), shape.n(), shape.k());
        let mut a = vec![0u16; m * k];
        let mut b = vec![0u16; k * n];
        for row in 0..m {
            a[row * k] = f32_to_f16(2048.0);
        }
        for col in 0..n {
            b[col * k] = 0x3C00; // 1.0h
        }
        let c = vec![1.0f32; m * n];
        let d = mma_f16_f32(shape, &a, &b, &c).unwrap();
        assert!(d.iter().all(|&v| v == 2049.0));
        // The same value forced through binary16 would round back to 2048.
        assert_eq!(f16_to_f32(f32_to_f16(2049.0)), 2048.0);
    }

    #[test]
    fn test_warp_fragment_roundtrip_matches_reference() {
        for shape in [HmmaShape::M16N8K8, HmmaShape::M16N8K16] {
            let (m, n, k) = (shape.m(), shape.n(), shape.k());
            let a: Vec<u16> = (0..m * k)
                .map(|i| f32_to_f16((i as f32) * 0.125 - 4.0))
                .collect();
            let b: Vec<u16> = (0..k * n)
                .map(|i| f32_to_f16(1.0 - (i as f32) * 0.0625))
                .collect();
            let c: Vec<f32> = (0..m * n).map(|i| (i as f32) * 0.5).collect();

            let reference = mma_f16_f32(shape, &a, &b, &c).unwrap();

            let frags = WarpFragments::distribute(shape, &a, &b, &c).unwrap();
            let mut unit = TensorCoreUnit::new();
            let d_frags = unit.issue_warp(&frags).unwrap();
            let collected = collect_d(shape, &d_frags);

            assert_eq!(collected, reference, "{shape:?} warp path");
            assert_eq!(unit.issued_ops, 1);
            assert_eq!(unit.retired_macs, (m * n * k) as u64);
        }
    }

    #[test]
    fn test_mma_rejects_wrong_operand_sizes() {
        let shape = HmmaShape::M16N8K16;
        let err = mma_f16_f32(shape, &[0u16; 8], &[0u16; 128], &[0.0f32; 128]).unwrap_err();
        assert_eq!(
            err,
            MmaError::BadA {
                expected: 256,
                got: 8
            }
        );
    }
}
