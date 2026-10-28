//! Per-fragment tests after raster: depth bounds, stencil, depth; plus face culling, which shares
//! the facing decision with two-sided stencil.
//!
//! State is open-gpu-doc `clc797.h` (same offsets in `clb197.h`). Compare functions and stencil
//! ops take both the OGL and the D3D encodings the header lists. Order and semantics follow the
//! GL pipeline: depth bounds (stored depth inside `[min, max]`, failing drops the fragment with no
//! stencil update), then stencil, then depth; the stencil op picked is FAIL, ZFAIL or ZPASS, and
//! the result is written through the stencil write mask.
//!
//! Facing: the signed area of the window-space triangle, positive when counter-clockwise with y
//! up, is matched against `OGL_SET_FRONT_FACE`; `SET_WINDOW_ORIGIN.FLIP_Y` inverts it. UNVERIFIED
//! against hardware: GL convention plus the flip, as the header names suggest.

use super::methods as m;

/// `SET_DEPTH_FUNC` / `SET_STENCIL_FUNC` (OGL and D3D encodings) applied to `(new, stored)`.
pub(crate) fn compare<T: PartialOrd>(func: u32, new: T, stored: T) -> bool {
    match func {
        0x200 | 1 => false,
        0x201 | 2 => new < stored,
        0x202 | 3 => new == stored,
        0x203 | 4 => new <= stored,
        0x204 | 5 => new > stored,
        0x205 | 6 => new != stored,
        0x206 | 7 => new >= stored,
        _ => true, // ALWAYS, and values the class does not define.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Keep,
    Zero,
    Replace,
    IncrSat,
    DecrSat,
    Invert,
    Incr,
    Decr,
}

fn op(v: u32) -> Result<Op, &'static str> {
    Ok(match v {
        0x1e00 | 1 => Op::Keep,
        0 | 2 => Op::Zero,
        0x1e01 | 3 => Op::Replace,
        0x1e02 | 4 => Op::IncrSat,
        0x1e03 | 5 => Op::DecrSat,
        0x150a | 6 => Op::Invert,
        0x8507 | 7 => Op::Incr,
        0x8508 | 8 => Op::Decr,
        _ => return Err("stencil op"),
    })
}

/// Stencil state for one face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StencilFace {
    func: u32,
    reference: u8,
    func_mask: u8,
    write_mask: u8,
    fail: Op,
    zfail: Op,
    zpass: Op,
}

impl StencilFace {
    /// `[op_fail, op_zfail, op_zpass, func, func_ref, func_mask, write_mask]` method offsets.
    fn read(regs: &[u32], at: [u32; 7]) -> Result<Self, &'static str> {
        let r = |method: u32| regs[(method / 4) as usize];
        Ok(StencilFace {
            fail: op(r(at[0]))?,
            zfail: op(r(at[1]))?,
            zpass: op(r(at[2]))?,
            func: r(at[3]),
            reference: r(at[4]) as u8,
            func_mask: r(at[5]) as u8,
            write_mask: r(at[6]) as u8,
        })
    }

    /// The stencil test for a stored value.
    pub(crate) fn test(&self, stored: u8) -> bool {
        compare(self.func, self.reference & self.func_mask, stored & self.func_mask)
    }

    /// The value to store after the test outcome: `stencil_pass`, then `depth_pass` when it ran.
    pub(crate) fn update(&self, stored: u8, stencil_pass: bool, depth_pass: bool) -> u8 {
        let op = match (stencil_pass, depth_pass) {
            (false, _) => self.fail,
            (true, false) => self.zfail,
            (true, true) => self.zpass,
        };
        let v = match op {
            Op::Keep => stored,
            Op::Zero => 0,
            Op::Replace => self.reference,
            Op::IncrSat => stored.saturating_add(1),
            Op::DecrSat => stored.saturating_sub(1),
            Op::Invert => !stored,
            Op::Incr => stored.wrapping_add(1),
            Op::Decr => stored.wrapping_sub(1),
        };
        (stored & !self.write_mask) | (v & self.write_mask)
    }
}

/// Stencil state for both faces, `None` when the test is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stencil {
    front: StencilFace,
    back: StencilFace,
}

impl Stencil {
    pub(crate) fn from_regs(regs: &[u32]) -> Result<Option<Self>, &'static str> {
        if regs[(m::SET_STENCIL_TEST / 4) as usize] & 1 == 0 {
            return Ok(None);
        }
        let front = StencilFace::read(
            regs,
            [
                m::SET_STENCIL_OP_FAIL,
                m::SET_STENCIL_OP_ZFAIL,
                m::SET_STENCIL_OP_ZPASS,
                m::SET_STENCIL_FUNC,
                m::SET_STENCIL_FUNC_REF,
                m::SET_STENCIL_FUNC_MASK,
                m::SET_STENCIL_MASK,
            ],
        )?;
        let back = if regs[(m::SET_TWO_SIDED_STENCIL_TEST / 4) as usize] & 1 == 1 {
            StencilFace::read(
                regs,
                [
                    m::SET_BACK_STENCIL_OP_FAIL,
                    m::SET_BACK_STENCIL_OP_ZFAIL,
                    m::SET_BACK_STENCIL_OP_ZPASS,
                    m::SET_BACK_STENCIL_FUNC,
                    m::SET_BACK_STENCIL_FUNC_REF,
                    m::SET_BACK_STENCIL_FUNC_MASK,
                    m::SET_BACK_STENCIL_MASK,
                ],
            )?
        } else {
            front
        };
        Ok(Some(Stencil { front, back }))
    }

    pub(crate) fn face(&self, front: bool) -> &StencilFace {
        if front {
            &self.front
        } else {
            &self.back
        }
    }
}

/// Facing and culling state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Facing {
    front_ccw: bool,
    cull_front: bool,
    cull_back: bool,
}

impl Facing {
    pub(crate) fn from_regs(regs: &[u32]) -> Self {
        let r = |method: u32| regs[(method / 4) as usize];
        let flip_y = (r(m::SET_WINDOW_ORIGIN) >> 4) & 1 == 1;
        // Anything but CW counts as CCW, the GL default.
        let front_ccw = (r(m::OGL_SET_FRONT_FACE) != 0x900) != flip_y;
        let cull = r(m::OGL_SET_CULL) & 1 == 1;
        let face = r(m::OGL_SET_CULL_FACE);
        Facing {
            front_ccw,
            cull_front: cull && matches!(face, 0x404 | 0x408),
            cull_back: cull && matches!(face, 0x405 | 0x408),
        }
    }

    /// Whether a triangle with window-space signed area `area` (positive counter-clockwise with
    /// y up) is front facing.
    pub(crate) fn is_front(&self, area: f32) -> bool {
        (area > 0.0) == self.front_ccw
    }

    pub(crate) fn culled(&self, front: bool) -> bool {
        if front {
            self.cull_front
        } else {
            self.cull_back
        }
    }
}

/// Pixel depth range of viewport 0 and whether each end clamps (`true`) or clips (`false`):
/// `(lo, hi, clamp_lo, clamp_hi)` from `SET_VIEWPORT_CLIP_CONTROL`. The `MIN_Z_MAX_Z` range is
/// `SET_VIEWPORT_CLIP_MIN_Z/MAX_Z(0)`, ordered so a reversed depth range still works.
pub(crate) fn pixel_z_range(regs: &[u32]) -> (f32, f32, bool, bool) {
    let r = |method: u32| regs[(method / 4) as usize];
    let cc = r(m::SET_VIEWPORT_CLIP_CONTROL);
    let viewport = || {
        let (a, b) = (f32::from_bits(r(m::SET_VIEWPORT_CLIP_MIN_Z)), f32::from_bits(r(m::SET_VIEWPORT_CLIP_MAX_Z)));
        (a.min(b), a.max(b))
    };
    let (lo, hi) = match (cc >> 16) & 3 {
        0 if cc & 1 == 1 => (0.0, 1.0),
        0 | 1 => viewport(),
        2 => (0.0, 1.0),
        _ => (f32::NEG_INFINITY, f32::INFINITY),
    };
    (lo, hi, (cc >> 3) & 1 == 1, (cc >> 4) & 1 == 1)
}

/// Applies the pixel depth range to `z`: `None` when a clipping end drops it.
pub(crate) fn pixel_z(z: f32, range: (f32, f32, bool, bool)) -> Option<f32> {
    let (lo, hi, clamp_lo, clamp_hi) = range;
    if z < lo {
        clamp_lo.then_some(lo)
    } else if z > hi {
        clamp_hi.then_some(hi)
    } else {
        Some(z)
    }
}

/// Depth bounds `[min, max]`, `None` when the test is off.
pub(crate) fn depth_bounds(regs: &[u32]) -> Option<(f32, f32)> {
    let r = |method: u32| regs[(method / 4) as usize];
    (r(m::SET_DEPTH_BOUNDS_TEST) & 1 == 1)
        .then(|| (f32::from_bits(r(m::SET_DEPTH_BOUNDS_MIN)), f32::from_bits(r(m::SET_DEPTH_BOUNDS_MAX))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(fail: u32, zfail: u32, zpass: u32, func: u32, reference: u8, func_mask: u8, write_mask: u8) -> StencilFace {
        StencilFace {
            fail: op(fail).unwrap(),
            zfail: op(zfail).unwrap(),
            zpass: op(zpass).unwrap(),
            func,
            reference,
            func_mask,
            write_mask,
        }
    }

    #[test]
    fn stencil_ops_take_both_encodings_and_respect_the_write_mask() {
        // OGL INCR_WRAP on pass, D3D DECRSAT on zfail, OGL ZERO on fail.
        let f = face(0, 5, 0x8507, 0x207, 7, 0xff, 0x0f);
        assert_eq!(f.update(0x1f, true, true), 0x10); // 0x20 through mask 0x0f over 0x1f
        assert_eq!(f.update(0x30, true, false), 0x3f); // 0x2f through mask 0x0f over 0x30
        assert_eq!(f.update(0xa5, false, false), 0xa0);
        let r = face(1, 1, 0x1e01, 0x207, 0x42, 0xff, 0xff);
        assert_eq!(r.update(0, true, true), 0x42);
        let inv = face(1, 1, 6, 0x207, 0, 0xff, 0xff);
        assert_eq!(inv.update(0x0f, true, true), 0xf0);
    }

    #[test]
    fn stencil_compare_masks_both_sides() {
        // EQUAL with ref 0x13 under mask 0x0f: matches any stored value whose low nibble is 3.
        let f = face(1, 1, 1, 0x202, 0x13, 0x0f, 0xff);
        assert!(f.test(0x03));
        assert!(f.test(0xf3));
        assert!(!f.test(0x12));
        // D3D LESS: ref < stored.
        let l = face(1, 1, 1, 2, 5, 0xff, 0xff);
        assert!(l.test(6));
        assert!(!l.test(5));
    }

    #[test]
    fn unknown_stencil_op_is_rejected() {
        let mut regs = vec![0u32; (m::METHOD_SPACE / 4) as usize];
        regs[(m::SET_STENCIL_TEST / 4) as usize] = 1;
        regs[(m::SET_STENCIL_OP_FAIL / 4) as usize] = 0x1234;
        assert_eq!(Stencil::from_regs(&regs), Err("stencil op"));
    }

    #[test]
    fn pixel_z_range_follows_viewport_clip_control() {
        let mut regs = vec![0u32; (m::METHOD_SPACE / 4) as usize];
        regs[(m::SET_VIEWPORT_CLIP_MIN_Z / 4) as usize] = 0.75f32.to_bits();
        regs[(m::SET_VIEWPORT_CLIP_MAX_Z / 4) as usize] = 0.25f32.to_bits();
        // USE_FIELD with MIN_Z_ZERO_MAX_Z_ONE false: the viewport range, reordered; both ends clip.
        let r = pixel_z_range(&regs);
        assert_eq!(r, (0.25, 0.75, false, false));
        assert_eq!(pixel_z(0.1, r), None);
        assert_eq!(pixel_z(0.5, r), Some(0.5));
        // MIN_Z_ZERO_MAX_Z_ONE, PIXEL_MAX_Z clamp.
        regs[(m::SET_VIEWPORT_CLIP_CONTROL / 4) as usize] = 1 | (1 << 4);
        let r = pixel_z_range(&regs);
        assert_eq!(r, (0.0, 1.0, false, true));
        assert_eq!(pixel_z(1.5, r), Some(1.0));
        assert_eq!(pixel_z(-0.5, r), None);
        // MINUS_INF_PLUS_INF keeps everything.
        regs[(m::SET_VIEWPORT_CLIP_CONTROL / 4) as usize] = 3 << 16;
        assert_eq!(pixel_z(-7.0, pixel_z_range(&regs)), Some(-7.0));
    }

    #[test]
    fn facing_follows_front_face_and_flip_y() {
        let mut regs = vec![0u32; (m::METHOD_SPACE / 4) as usize];
        regs[(m::OGL_SET_FRONT_FACE / 4) as usize] = 0x901;
        assert!(Facing::from_regs(&regs).is_front(1.0));
        regs[(m::OGL_SET_FRONT_FACE / 4) as usize] = 0x900;
        assert!(!Facing::from_regs(&regs).is_front(1.0));
        regs[(m::SET_WINDOW_ORIGIN / 4) as usize] = 1 << 4;
        assert!(Facing::from_regs(&regs).is_front(1.0));
        regs[(m::OGL_SET_CULL / 4) as usize] = 1;
        regs[(m::OGL_SET_CULL_FACE / 4) as usize] = 0x405;
        let f = Facing::from_regs(&regs);
        assert!(f.culled(false) && !f.culled(true));
    }
}
