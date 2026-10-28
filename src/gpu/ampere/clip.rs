//! Clip-space triangle clipping before the perspective divide.
//!
//! Every mode clips against `w = W_EPSILON`, so nothing behind the eye reaches the divide (the
//! class's default `GEOMETRY_CLIP` is `WZERO_CLIP`), and against an X/Y guard band of
//! `GUARD_BAND` times the viewport (`GEOMETRY_GUARDBAND_SCALE_256`), which keeps window coordinates
//! of vertices made near `w = 0` small enough for f32 edge functions. With `GEOMETRY_CLIP` `FRUSTUM_XYZ_CLIP` or
//! `FRUSTUM_Z_CLIP` the near and far planes are clipped as well: near is `z = -w` or `z = 0` as
//! `SET_VIEWPORT_Z_CLIP.RANGE` says, far is `z = w`. Inside the guard band the raster bounds
//! (viewport clip, scissor, target size) do the X/Y clipping. State from open-gpu-doc `clc797.h`; how the hardware treats the planes
//! beyond their names is UNVERIFIED.
//!
//! Sutherland-Hodgman per plane, interpolating position and varyings linearly in clip space,
//! which is perspective-correct once divided. The output is a convex polygon in the input's
//! winding, rasterized as a fan.

use super::methods as m;

/// Smallest `w` kept; `1/w` stays finite and positive.
pub(crate) const W_EPSILON: f32 = 1.0e-6;
/// Guard band half-extent in clip-space units (`|x|, |y| <= GUARD_BAND * w`).
pub(crate) const GUARD_BAND: f32 = 256.0;

/// A vertex shader result before the divide.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClipVertex {
    pub pos: [f32; 4],
    /// Generic outputs `[location][component]` as floats.
    pub varyings: [[f32; 4]; 32],
}

impl ClipVertex {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        ClipVertex {
            pos: std::array::from_fn(|i| mix(self.pos[i], other.pos[i])),
            varyings: std::array::from_fn(|l| std::array::from_fn(|c| mix(self.varyings[l][c], other.varyings[l][c]))),
        }
    }
}

/// Which planes a draw clips against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZClip {
    /// Only `w = W_EPSILON`.
    Off,
    /// Near `z = -w` (`NEGATIVE_W_TO_POSITIVE_W`) and far `z = w`.
    MinusOne,
    /// Near `z = 0` (`ZERO_TO_POSITIVE_W`) and far `z = w`.
    Zero,
}

impl ZClip {
    pub(crate) fn from_regs(regs: &[u32]) -> Self {
        let r = |method: u32| regs[(method / 4) as usize];
        match (r(m::SET_VIEWPORT_CLIP_CONTROL) >> 11) & 7 {
            3 | 5 if r(m::SET_VIEWPORT_Z_CLIP) & 1 == 1 => ZClip::Zero,
            3 | 5 => ZClip::MinusOne,
            _ => ZClip::Off,
        }
    }

    /// Signed distances to the planes in use; a vertex is inside when all are `>= 0`.
    fn distances(self, p: &[f32; 4]) -> ([f32; 7], usize) {
        let [x, y, z, w] = *p;
        let g = GUARD_BAND * w;
        let base = [w - W_EPSILON, g - x, g + x, g - y, g + y];
        match self {
            ZClip::Off => ([base[0], base[1], base[2], base[3], base[4], 0.0, 0.0], 5),
            ZClip::MinusOne => ([base[0], base[1], base[2], base[3], base[4], z + w, w - z], 7),
            ZClip::Zero => ([base[0], base[1], base[2], base[3], base[4], z, w - z], 7),
        }
    }
}

/// Clips one triangle; fewer than three vertices back means nothing is left.
pub(crate) fn clip_triangle(tri: [&ClipVertex; 3], z: ZClip) -> Vec<ClipVertex> {
    let inside = |v: &ClipVertex| {
        let (d, n) = z.distances(&v.pos);
        d[..n].iter().all(|&d| d >= 0.0)
    };
    let mut poly: Vec<ClipVertex> = tri.iter().map(|v| **v).collect();
    if tri.iter().all(|v| inside(v)) {
        return poly;
    }
    let planes = z.distances(&[0.0; 4]).1;
    for plane in 0..planes {
        let dist = |v: &ClipVertex| z.distances(&v.pos).0[plane];
        let mut out = Vec::with_capacity(poly.len() + 1);
        for (i, cur) in poly.iter().enumerate() {
            let next = &poly[(i + 1) % poly.len()];
            let (dc, dn) = (dist(cur), dist(next));
            if dc >= 0.0 {
                out.push(*cur);
            }
            if (dc >= 0.0) != (dn >= 0.0) {
                out.push(cur.lerp(next, dc / (dc - dn)));
            }
        }
        poly = out;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32, w: f32, tag: f32) -> ClipVertex {
        let mut varyings = [[0.0; 4]; 32];
        varyings[0][0] = tag;
        ClipVertex { pos: [x, y, z, w], varyings }
    }

    #[test]
    fn triangle_inside_is_returned_unchanged() {
        let t = [v(0.0, 0.0, 0.0, 1.0, 1.0), v(1.0, 0.0, 0.0, 1.0, 2.0), v(0.0, 1.0, 0.0, 1.0, 3.0)];
        let out = clip_triangle([&t[0], &t[1], &t[2]], ZClip::MinusOne);
        assert_eq!(out.len(), 3);
        assert_eq!(out.iter().map(|v| v.varyings[0][0]).collect::<Vec<_>>(), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn vertex_behind_the_eye_is_clipped_to_w_and_the_guard_band() {
        // Vertex 2 has w = -1: the w = eps plane cuts edges 1-2 and 2-0, then the guard band
        // trims the slivers that run off towards infinity.
        let t = [v(0.0, 0.0, 0.0, 1.0, 0.0), v(1.0, 0.0, 0.0, 1.0, 0.0), v(0.0, 1.0, 0.0, -1.0, 4.0)];
        let out = clip_triangle([&t[0], &t[1], &t[2]], ZClip::Off);
        assert!(out.len() >= 3);
        for v in &out {
            let tol = 1e-3;
            assert!(v.pos[3] > 0.0);
            assert!(v.pos[0].abs() <= GUARD_BAND * v.pos[3] + tol && v.pos[1].abs() <= GUARD_BAND * v.pos[3] + tol);
            // The varying stays the same linear function of clip position: 4 * (1 - w) / 2.
            assert!((v.varyings[0][0] - 2.0 * (1.0 - v.pos[3])).abs() < 1e-3);
        }
        assert_eq!(out[0].pos, t[0].pos);
        assert_eq!(out[1].pos, t[1].pos);
    }

    #[test]
    fn near_plane_follows_the_z_range() {
        // z/w = -0.5 everywhere: inside -w..w, outside 0..w.
        let t = [v(0.0, 0.0, -0.5, 1.0, 0.0), v(1.0, 0.0, -0.5, 1.0, 0.0), v(0.0, 1.0, -0.5, 1.0, 0.0)];
        assert_eq!(clip_triangle([&t[0], &t[1], &t[2]], ZClip::MinusOne).len(), 3);
        assert!(clip_triangle([&t[0], &t[1], &t[2]], ZClip::Zero).is_empty());
        assert_eq!(clip_triangle([&t[0], &t[1], &t[2]], ZClip::Off).len(), 3);
    }

    #[test]
    fn z_clip_mode_comes_from_viewport_clip_control() {
        let mut regs = vec![0u32; (m::METHOD_SPACE / 4) as usize];
        assert_eq!(ZClip::from_regs(&regs), ZClip::Off);
        regs[(m::SET_VIEWPORT_CLIP_CONTROL / 4) as usize] = 3 << 11;
        assert_eq!(ZClip::from_regs(&regs), ZClip::MinusOne);
        regs[(m::SET_VIEWPORT_Z_CLIP / 4) as usize] = 1;
        assert_eq!(ZClip::from_regs(&regs), ZClip::Zero);
        regs[(m::SET_VIEWPORT_CLIP_CONTROL / 4) as usize] = 2 << 11;
        assert_eq!(ZClip::from_regs(&regs), ZClip::Off);
    }
}
