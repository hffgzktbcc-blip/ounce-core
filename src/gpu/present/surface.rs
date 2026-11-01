//! Turns a guest surface (block linear or pitch, as the Ampere engine and a future display
//! service describe it) into a host-ready [`Frame`]. This is the consumer half of the frame
//! descriptor agreed in `docs/design/m5-deko3d-triangle.md`; it reuses `gpu::tiling` rather than
//! carrying a second detiler.

use super::Frame;
use crate::gpu::tiling::{self, Format, Surface, TilingError};
use crate::mmu::Memory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceError {
    /// Only 8-bit RGBA / BGRA surfaces can be shown directly.
    UnsupportedFormat(Format),
    Tiling(TilingError),
    /// The surface bytes are not fully mapped in guest memory.
    Unmapped { addr: u64, len: usize },
    /// Detiled bytes do not cover `width * height` RGBA8 pixels.
    FrameSizeMismatch { expected: Option<usize>, got: usize },
}

impl From<TilingError> for SurfaceError {
    fn from(e: TilingError) -> Self {
        Self::Tiling(e)
    }
}

/// Reads `surface` at guest address `addr` and returns it as a packed RGBA8 frame (alpha forced
/// opaque, since a swapchain image's alpha is not meant to blend with the desktop).
pub fn frame_from_surface(mmu: &Memory, addr: u64, surface: &Surface) -> Result<Frame, SurfaceError> {
    let swap_rb = match surface.format {
        Format::R8G8B8A8Unorm | Format::R8G8B8A8Srgb => false,
        Format::B8G8R8A8Unorm | Format::B8G8R8A8Srgb => true,
        other => return Err(SurfaceError::UnsupportedFormat(other)),
    };
    let size = surface.size_bytes()?;
    let mut raw = vec![0u8; size];
    mmu.read_bytes(addr, &mut raw).map_err(|_| SurfaceError::Unmapped { addr, len: size })?;
    let mut packed = tiling::detile(surface, &raw)?;
    for px in packed.chunks_exact_mut(4) {
        if swap_rb {
            px.swap(0, 2);
        }
        px[3] = 0xFF;
    }
    let (width, height) = (surface.width as usize, surface.height as usize);
    let expected = width.checked_mul(height).and_then(|pixels| pixels.checked_mul(4));
    if expected != Some(packed.len()) {
        return Err(SurfaceError::FrameSizeMismatch { expected, got: packed.len() });
    }
    Ok(Frame { width, height, rgba: packed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::tiling::Layout;
    use crate::mmu::Permission;

    fn surface(format: Format, layout: Layout) -> Surface {
        Surface { width: 70, height: 45, depth: 1, format, layout }
    }

    fn pattern(w: usize, h: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[x as u8, y as u8, (x ^ y) as u8, 0x11]);
            }
        }
        v
    }

    fn load(mmu: &mut Memory, addr: u64, bytes: &[u8]) {
        mmu.map_range(addr, bytes.len(), Permission::READ_WRITE);
        mmu.write_bytes(addr, bytes).unwrap();
    }

    #[test]
    fn block_linear_rgba_round_trips_to_frame() {
        let s = surface(Format::R8G8B8A8Unorm, Layout::BlockLinear { height_log2: 2, depth_log2: 0 });
        let linear = pattern(70, 45);
        let tiled = tiling::tile(&s, &linear).unwrap();
        let mut mmu = Memory::new();
        load(&mut mmu, 0x4000_0000, &tiled);
        let f = frame_from_surface(&mmu, 0x4000_0000, &s).unwrap();
        assert_eq!((f.width, f.height), (70, 45));
        assert_eq!(f.pixel(3, 5), Some((3, 5, 6, 0xFF)));
        assert_eq!(f.pixel(69, 44), Some((69, 44, 69 ^ 44, 0xFF)));
    }

    #[test]
    fn pitch_bgra_swaps_red_and_blue() {
        let s = surface(Format::B8G8R8A8Unorm, Layout::Pitch { stride: 70 * 4 });
        let mut mmu = Memory::new();
        load(&mut mmu, 0x4000_0000, &pattern(70, 45));
        let f = frame_from_surface(&mmu, 0x4000_0000, &s).unwrap();
        assert_eq!(f.pixel(3, 5), Some((6, 5, 3, 0xFF)));
    }

    #[test]
    fn rejects_unsupported_format_and_unmapped_memory() {
        let mmu = Memory::new();
        let s = surface(Format::R16Float, Layout::Pitch { stride: 140 });
        assert_eq!(frame_from_surface(&mmu, 0, &s), Err(SurfaceError::UnsupportedFormat(Format::R16Float)));
        let s = surface(Format::R8G8B8A8Unorm, Layout::Pitch { stride: 280 });
        assert!(matches!(frame_from_surface(&mmu, 0x5000_0000, &s), Err(SurfaceError::Unmapped { .. })));
    }
}
