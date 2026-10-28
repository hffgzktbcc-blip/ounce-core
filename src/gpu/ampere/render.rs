//! Software rendering into guest memory: clears and draws write the bound color and depth targets
//! as raw pitch-linear or block-linear surfaces, exactly where the class state points. The present
//! side (Track A) detiles them; nothing here converts a surface for display.
//!
//! A target is read whole, modified, and written back whole, so the memory must be readable. With
//! a write-only [`GpuMemoryWriter`] (its `read` returns false) the engine keeps step-1 behaviour:
//! records only, nothing rendered.
//!
//! Scope (step 3): triangles, strips, fans and quads; viewport 0, scissor 0, viewport clip 0 and the
//! surface clip; depth test and write; per-target color write masks. Shaders are the `gpu::sass` IR
//! run on the CPU, so only AMPERE_B draws (Ampere SASS) render. Not modelled: blending (written as
//! if off, with a diagnostic), stencil test, culling, clipping against the near plane (triangles
//! with a vertex at `w <= 0` are dropped and counted), multisampling, layered targets.

use super::engine::{Ampere3d, ClearRecord, Diagnostic, DrawRecord, GpuMemoryWriter, Topology};
use super::methods as m;
use super::shade::{self, ShaderInputs};
use super::surface::{self, addr40, ColorFormat, ColorTarget, DepthFormat, DepthTarget, SurfaceLayout};
use crate::gpu::sass::decode::IpaFreq;
use crate::gpu::sass::ir::{self, attr_slot, AttrSlot, Function, Op};
use crate::gpu::sass::sph::{PixelImap, Sph, SphType};
use crate::gpu::sass::{ShaderStage as SassStage, XlateError, AMPERE_SPH_SIZE};
use crate::gpu::tiling::{self, BlockLinear, Layout, Surface};

/// Counters for what the software path actually wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub clears: u32,
    pub draws: u32,
    pub triangles: u32,
    /// Triangles dropped because a vertex had `w <= 0` (no near-plane clipping yet).
    pub dropped_triangles: u32,
    /// Fragments that passed the depth test and were written.
    pub fragments: u64,
}

/// Placement of one surface level in memory, independent of its format.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SurfaceView {
    pub gpu_va: u64,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    size: usize,
    kind: ViewKind,
}

#[derive(Debug, Clone, Copy)]
enum ViewKind {
    Pitch { stride: u32 },
    Block(BlockLinear),
}

/// A tiling format with the given element size; tiling only looks at the size.
fn tiling_format(bpp: u32) -> Option<tiling::Format> {
    Some(match bpp {
        1 => tiling::Format::R8Unorm,
        2 => tiling::Format::R5G6B5Unorm,
        4 => tiling::Format::R8G8B8A8Unorm,
        8 => tiling::Format::R16G16B16A16Float,
        16 => tiling::Format::R32G32B32A32Float,
        _ => return None,
    })
}

impl SurfaceView {
    pub(crate) fn block_linear(gpu_va: u64, width: u32, height: u32, bpp: u32, height_log2: u8, depth_log2: u8) -> Option<Self> {
        let s = Surface::new_2d(width, height, tiling_format(bpp)?, Layout::BlockLinear { height_log2, depth_log2 });
        let size = s.size_bytes().ok()?;
        Some(Self { gpu_va, width, height, bpp, size, kind: ViewKind::Block(s.block_linear()?) })
    }

    pub(crate) fn pitch(gpu_va: u64, width: u32, height: u32, bpp: u32, stride: u32) -> Option<Self> {
        let s = Surface::new_2d(width, height, tiling_format(bpp)?, Layout::Pitch { stride });
        let size = s.size_bytes().ok()?;
        Some(Self { gpu_va, width, height, bpp, size, kind: ViewKind::Pitch { stride } })
    }

    fn from_layout(gpu_va: u64, width: u32, height: u32, bpp: u32, layout: SurfaceLayout) -> Option<Self> {
        match layout {
            // Tiling has no block-width parameter; drivers always program one GOB.
            SurfaceLayout::BlockLinear { block_width_log2: 0, block_height_log2, block_depth_log2 } => {
                Self::block_linear(gpu_va, width, height, bpp, block_height_log2, block_depth_log2)
            }
            SurfaceLayout::BlockLinear { .. } => None,
            SurfaceLayout::Pitch { pitch_bytes } => Self::pitch(gpu_va, width, height, bpp, pitch_bytes),
        }
    }

    /// Byte offset of texel `(x, y)` from `gpu_va`.
    pub(crate) fn offset(&self, x: u32, y: u32) -> usize {
        match self.kind {
            ViewKind::Pitch { stride } => y as usize * stride as usize + (x * self.bpp) as usize,
            ViewKind::Block(bl) => bl.offset(x * self.bpp, y, 0),
        }
    }

    pub(crate) fn texel_va(&self, x: u32, y: u32) -> u64 {
        self.gpu_va + self.offset(x, y) as u64
    }

    pub(crate) fn size(&self) -> usize {
        self.size
    }
}

/// A target read into host memory for modification, written back by [`Staged::store`].
struct Staged {
    view: SurfaceView,
    data: Vec<u8>,
}

impl Staged {
    fn load(view: SurfaceView, mem: &mut dyn GpuMemoryWriter) -> Option<Self> {
        let mut data = vec![0u8; view.size()];
        mem.read(view.gpu_va, &mut data).then_some(Staged { view, data })
    }

    fn store(&self, mem: &mut dyn GpuMemoryWriter) {
        mem.write(self.view.gpu_va, &self.data);
    }

    fn texel(&mut self, x: u32, y: u32) -> &mut [u8] {
        let o = self.view.offset(x, y);
        &mut self.data[o..o + self.view.bpp as usize]
    }
}

/// How one channel sits in a packed pixel.
#[derive(Debug, Clone, Copy)]
enum Chan {
    Unorm { shift: u32, bits: u32 },
    F32 { shift: u32 },
    None,
}

/// Channel placement for R, G, B, A. Format names list the most significant field first, so
/// A8B8G8R8 has R in bits 7:0 (byte 0) and R5G6B5 has R in bits 15:11.
fn color_channels(format: ColorFormat) -> Option<[Chan; 4]> {
    use Chan::*;
    let u = |shift, bits| Unorm { shift, bits };
    Some(match format {
        ColorFormat::A8B8G8R8 | ColorFormat::X8B8G8R8 => [u(0, 8), u(8, 8), u(16, 8), u(24, 8)],
        ColorFormat::A8R8G8B8 => [u(16, 8), u(8, 8), u(0, 8), u(24, 8)],
        ColorFormat::A2B10G10R10 => [u(0, 10), u(10, 10), u(20, 10), u(30, 2)],
        ColorFormat::R5G6B5 => [u(11, 5), u(5, 6), u(0, 5), None],
        ColorFormat::R16G16B16A16 => [u(0, 16), u(16, 16), u(32, 16), u(48, 16)],
        ColorFormat::Rf32Gf32Bf32Af32 => [F32 { shift: 0 }, F32 { shift: 32 }, F32 { shift: 64 }, F32 { shift: 96 }],
        ColorFormat::Disabled | ColorFormat::Other(_) => return Option::None,
    })
}

fn unorm(v: f32, bits: u32) -> u128 {
    let max = ((1u64 << bits) - 1) as f32;
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    (v * max).round() as u128
}

/// Writes `color` into `texel` for the channels enabled in `mask`.
fn write_color(format: ColorFormat, texel: &mut [u8], color: [f32; 4], mask: [bool; 4]) -> bool {
    let Some(chans) = color_channels(format) else { return false };
    let mut buf = [0u8; 16];
    buf[..texel.len()].copy_from_slice(texel);
    let mut px = u128::from_le_bytes(buf);
    for i in 0..4 {
        if !mask[i] {
            continue;
        }
        let (shift, bits, v) = match chans[i] {
            Chan::Unorm { shift, bits } => (shift, bits, unorm(color[i], bits)),
            Chan::F32 { shift } => (shift, 32, color[i].to_bits() as u128),
            Chan::None => continue,
        };
        let field = ((1u128 << bits) - 1) << shift;
        px = (px & !field) | ((v << shift) & field);
    }
    texel.copy_from_slice(&px.to_le_bytes()[..texel.len()]);
    true
}

fn depth_bytes(format: DepthFormat) -> Option<u32> {
    Some(match format {
        DepthFormat::Z16 => 2,
        DepthFormat::Z24S8 | DepthFormat::X8Z24 | DepthFormat::S8Z24 | DepthFormat::Zf32 => 4,
        DepthFormat::Zf32X24S8 => 8,
        DepthFormat::Other(_) => return None,
    })
}

/// Depth and stencil placement, same naming convention as colors: Z24S8 = Z in 31:8, S in 7:0;
/// S8Z24/X8Z24 = Z in 23:0. UNVERIFIED against hardware: read from the format names only.
fn read_depth(format: DepthFormat, t: &[u8]) -> f32 {
    let w = u32::from_le_bytes([t[0], t[1], *t.get(2).unwrap_or(&0), *t.get(3).unwrap_or(&0)]);
    match format {
        DepthFormat::Z16 => (w & 0xffff) as f32 / 65535.0,
        DepthFormat::Z24S8 => (w >> 8) as f32 / 16_777_215.0,
        DepthFormat::X8Z24 | DepthFormat::S8Z24 => (w & 0xff_ffff) as f32 / 16_777_215.0,
        DepthFormat::Zf32 | DepthFormat::Zf32X24S8 => f32::from_bits(w),
        DepthFormat::Other(_) => 0.0,
    }
}

fn write_depth(format: DepthFormat, t: &mut [u8], depth: Option<f32>, stencil: Option<u8>) {
    let mut w = u32::from_le_bytes([t[0], t[1], *t.get(2).unwrap_or(&0), *t.get(3).unwrap_or(&0)]);
    let z24 = |d: f32| unorm(d, 24) as u32;
    match format {
        DepthFormat::Z16 => {
            if let Some(d) = depth {
                w = unorm(d, 16) as u32;
            }
        }
        DepthFormat::Z24S8 => {
            if let Some(d) = depth {
                w = (w & 0xff) | (z24(d) << 8);
            }
            if let Some(s) = stencil {
                w = (w & !0xff) | s as u32;
            }
        }
        DepthFormat::X8Z24 | DepthFormat::S8Z24 => {
            if let Some(d) = depth {
                w = (w & 0xff00_0000) | z24(d);
            }
            if let (Some(s), DepthFormat::S8Z24) = (stencil, format) {
                w = (w & 0xff_ffff) | ((s as u32) << 24);
            }
        }
        DepthFormat::Zf32 | DepthFormat::Zf32X24S8 => {
            if let Some(d) = depth {
                w = d.to_bits();
            }
            if let (Some(s), DepthFormat::Zf32X24S8) = (stencil, format) {
                t[4] = s;
            }
        }
        DepthFormat::Other(_) => return,
    }
    let n = t.len().min(4);
    t[..n].copy_from_slice(&w.to_le_bytes()[..n]);
}

fn color_view(ct: &ColorTarget) -> Option<SurfaceView> {
    let bpp = ct.format.bytes_per_pixel()?;
    SurfaceView::from_layout(ct.gpu_va, ct.width_texels()?, ct.height, bpp, ct.layout)
}

fn depth_view(zt: &DepthTarget) -> Option<SurfaceView> {
    SurfaceView::from_layout(zt.gpu_va, zt.width, zt.height, depth_bytes(zt.format)?, zt.layout)
}

/// Half-open pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Rect {
    fn new(w: u32, h: u32) -> Self {
        Rect { x0: 0, y0: 0, x1: w, y1: h }
    }

    fn clip(self, x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Rect { x0: self.x0.max(x0), y0: self.y0.max(y0), x1: self.x1.min(x1), y1: self.y1.min(y1) }
    }
}

/// `SET_DEPTH_FUNC` (OGL and D3D encodings) applied to `(new, stored)`.
fn depth_pass(func: u32, new: f32, stored: f32) -> bool {
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

/// A shader loaded from guest memory and lowered to IR.
struct LoadedShader {
    sph: Sph,
    func: Function,
}

const SHADER_READ_START: usize = 0x1000;
const SHADER_READ_MAX: usize = 0x10000;

fn load_shader(stage: SassStage, va: u64, mem: &mut dyn GpuMemoryWriter) -> Result<LoadedShader, &'static str> {
    let mut size = SHADER_READ_START;
    loop {
        let mut bytes = vec![0u8; AMPERE_SPH_SIZE + size];
        if !mem.read(va, &mut bytes) {
            return Err("shader code is not readable");
        }
        let sph = Sph::parse(&bytes).map_err(|_| "bad shader program header")?;
        let ok = match stage {
            SassStage::Vertex => sph.sph_type == SphType::Vtg && sph.shader_type == 1,
            SassStage::Fragment => sph.sph_type == SphType::Ps && sph.shader_type == 5,
        };
        if !ok {
            return Err("shader header is for another stage");
        }
        match ir::lower(stage, &bytes[AMPERE_SPH_SIZE..]) {
            Ok(func) => return Ok(LoadedShader { sph, func }),
            Err(XlateError::MissingExit) if size < SHADER_READ_MAX => size *= 2,
            Err(_) => return Err("shader uses SASS the translator does not support"),
        }
    }
}

/// Constant buffers one stage reads, keyed by bank.
#[derive(Default)]
struct Cbufs(Vec<(u8, Vec<u8>)>);

impl Cbufs {
    fn word(&self, bank: u8, offset: u16) -> u32 {
        self.0
            .iter()
            .find(|(b, _)| *b == bank)
            .and_then(|(_, d)| d.get(offset as usize..offset as usize + 4))
            .map_or(0, |w| u32::from_le_bytes(w.try_into().unwrap()))
    }
}

struct VertexIn<'a> {
    attrs: &'a [[u32; 4]; 32],
    cbufs: &'a Cbufs,
}

impl ShaderInputs for VertexIn<'_> {
    fn cbuf(&self, bank: u8, offset: u16) -> u32 {
        self.cbufs.word(bank, offset)
    }
    fn attr(&self, addr: u16) -> u32 {
        match attr_slot(addr) {
            Some(AttrSlot::Generic { loc, comp }) if (loc as usize) < 32 => self.attrs[loc as usize][comp as usize],
            _ => 0,
        }
    }
    fn interp(&self, _: u16, _: IpaFreq) -> f32 {
        0.0
    }
}

/// One shaded vertex in window space.
#[derive(Debug, Clone, Copy)]
struct WinVertex {
    x: f32,
    y: f32,
    z: f32,
    inv_w: f32,
    /// Generic outputs `[location][component]` as floats.
    varyings: [[f32; 4]; 32],
}

struct FragmentIn<'a> {
    v: [&'a WinVertex; 3],
    l: [f32; 3],
    px: u32,
    py: u32,
    z: f32,
    sph: &'a Sph,
    cbufs: &'a Cbufs,
}

impl ShaderInputs for FragmentIn<'_> {
    fn cbuf(&self, bank: u8, offset: u16) -> u32 {
        self.cbufs.word(bank, offset)
    }
    fn attr(&self, _: u16) -> u32 {
        0
    }
    fn interp(&self, addr: u16, freq: IpaFreq) -> f32 {
        let lerp = |f: &dyn Fn(&WinVertex) -> f32| self.l[0] * f(self.v[0]) + self.l[1] * f(self.v[1]) + self.l[2] * f(self.v[2]);
        match attr_slot(addr) {
            Some(AttrSlot::Position(0)) => self.px as f32 + 0.5,
            Some(AttrSlot::Position(1)) => self.py as f32 + 0.5,
            Some(AttrSlot::Position(2)) => self.z,
            Some(AttrSlot::Position(_)) => lerp(&|v| v.inv_w),
            Some(AttrSlot::Generic { loc, comp }) if (loc as usize) < 32 => {
                let (loc, comp) = (loc as usize, comp as usize);
                let mode = self.sph.ps_imap_generic(loc as u32, comp as u32);
                if freq == IpaFreq::Constant || mode == PixelImap::Constant {
                    // Provoking vertex: the first vertex of the triangle (SET_PROVOKING_VERTEX not
                    // modelled).
                    return self.v[0].varyings[loc][comp];
                }
                match mode {
                    PixelImap::ScreenLinear => lerp(&|v| v.varyings[loc][comp]),
                    // Perspective (and unused reads, as in the WGSL emitter): the hardware hands the
                    // shader a/w interpolated in screen space; PassMulW multiplies w back in.
                    _ => lerp(&|v| v.varyings[loc][comp] * v.inv_w),
                }
            }
            _ => 0.0,
        }
    }
}

fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let frac = (h & 0x3ff) as f32;
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => if frac == 0.0 { f32::INFINITY } else { f32::NAN },
        e => (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}

/// Decodes one vertex attribute from `bytes` per `COMPONENT_BIT_WIDTHS` / `NUMERICAL_TYPE`, as the
/// raw words the shader sees (floats as bits, integers as integers). `None` = unsupported format.
fn decode_attribute(widths: u32, numerical: u32, bytes: &[u8]) -> Option<[u32; 4]> {
    // (component count, bytes per component)
    let (n, size) = match widths {
        0x01 => (4, 4),
        0x02 => (3, 4),
        0x04 => (2, 4),
        0x12 => (1, 4),
        0x03 => (4, 2),
        0x05 => (3, 2),
        0x0f => (2, 2),
        0x1b => (1, 2),
        0x0a | 0x2f => (4, 1), // R8_G8_B8_A8, A8B8G8R8: R in byte 0 either way.
        0x13 => (3, 1),
        0x18 => (2, 1),
        0x1d => (1, 1),
        _ => return None,
    };
    if bytes.len() < n * size {
        return None;
    }
    let int_type = matches!(numerical, 3 | 4);
    let one = if int_type { 1 } else { 1.0f32.to_bits() };
    let mut out = [0, 0, 0, one];
    for (i, slot) in out.iter_mut().enumerate().take(n) {
        let raw = &bytes[i * size..(i + 1) * size];
        let u = match size {
            4 => u32::from_le_bytes(raw.try_into().unwrap()),
            2 => u16::from_le_bytes(raw.try_into().unwrap()) as u32,
            _ => raw[0] as u32,
        };
        let bits = size as u32 * 8;
        let signed = ((u << (32 - bits)) as i32) >> (32 - bits);
        let max = ((1u64 << bits) - 1) as f32;
        *slot = match (numerical, size) {
            (7, 4) | (3, 4) | (4, 4) => u,
            (7, 2) => half_to_f32(u as u16).to_bits(),
            (2, _) => (u as f32 / max).to_bits(),
            (1, _) => (signed as f32 / (max / 2.0).floor()).max(-1.0).to_bits(),
            (3, _) => signed as u32,
            (4, _) => u,
            (5, _) => (u as f32).to_bits(),
            (6, _) => (signed as f32).to_bits(),
            _ => return None,
        };
    }
    Some(out)
}

/// Splits a vertex range into triangles (indices into the shaded vertex list).
fn triangles(topology: Topology, count: usize) -> Option<Vec<[usize; 3]>> {
    let mut t = Vec::new();
    match topology {
        Topology::Triangles => (0..count / 3).for_each(|i| t.push([3 * i, 3 * i + 1, 3 * i + 2])),
        Topology::TriangleStrip => (0..count.saturating_sub(2))
            .for_each(|i| t.push(if i % 2 == 0 { [i, i + 1, i + 2] } else { [i + 1, i, i + 2] })),
        Topology::TriangleFan | Topology::Polygon => (1..count.saturating_sub(1)).for_each(|i| t.push([0, i, i + 1])),
        Topology::Quads => (0..count / 4).for_each(|q| {
            t.push([4 * q, 4 * q + 1, 4 * q + 2]);
            t.push([4 * q, 4 * q + 2, 4 * q + 3]);
        }),
        _ => return None,
    }
    Some(t)
}

fn edge(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

/// Top-left fill rule for an edge of a triangle with positive area in this orientation.
fn top_left(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.1 == b.1 && b.0 < a.0) || b.1 < a.1
}

/// Color targets in `SET_CT_SELECT` order, keeping the select position even for disabled slots,
/// since a pixel shader's output `t` goes to position `t`.
fn selected_targets(regs: &[u32]) -> Vec<Option<ColorTarget>> {
    let select = regs[m::SET_CT_SELECT as usize / 4];
    let count = (select & 0xf).min(m::COLOR_TARGET_COUNT as u32);
    (0..count)
        .map(|n| surface::color_target(regs, ((select >> (4 + 3 * n)) & 7) as usize))
        .map(|ct| (ct.format != ColorFormat::Disabled).then_some(ct))
        .collect()
}

fn write_mask(regs: &[u32], slot: u8) -> [bool; 4] {
    let w = regs[(m::SET_CT_WRITE + 4 * slot as u32) as usize / 4];
    [0, 4, 8, 12].map(|b| (w >> b) & 1 == 1)
}

impl Ampere3d {
    fn reg_at(&self, method: u32) -> u32 {
        self.regs[method as usize / 4]
    }

    fn skip(&mut self, reason: &'static str) {
        self.diagnostics.push(Diagnostic::RenderSkipped { reason });
    }

    /// Writes a `CLEAR_SURFACE` into the targets it names.
    pub(super) fn render_clear(&mut self, rec: &ClearRecord, mem: &mut dyn GpuMemoryWriter) {
        let ctrl = self.reg_at(m::SET_CLEAR_SURFACE_CONTROL);
        let region = |w: u32, h: u32| {
            let mut r = Rect::new(w, h);
            if (ctrl >> 4) & 1 == 1 {
                let (hz, vt) = (self.reg_at(m::SET_CLEAR_RECT_HORIZONTAL), self.reg_at(m::SET_CLEAR_RECT_VERTICAL));
                r = r.clip(hz & 0xffff, vt & 0xffff, hz >> 16, vt >> 16);
            }
            if (ctrl >> 8) & 1 == 1 && self.reg_at(m::SET_SCISSOR_ENABLE) & 1 == 1 {
                let (hz, vt) = (self.reg_at(m::SET_SCISSOR_HORIZONTAL), self.reg_at(m::SET_SCISSOR_VERTICAL));
                r = r.clip(hz & 0xffff, vt & 0xffff, hz >> 16, vt >> 16);
            }
            if (ctrl >> 12) & 1 == 1 {
                let (hz, vt) = (self.reg_at(m::SET_VIEWPORT_CLIP_HORIZONTAL), self.reg_at(m::SET_VIEWPORT_CLIP_VERTICAL));
                let (x, y) = (hz & 0xffff, vt & 0xffff);
                r = r.clip(x, y, x + (hz >> 16), y + (vt >> 16));
            }
            r
        };
        let mut wrote = false;
        if let Some(ct) = rec.color_target.filter(|ct| ct.format != ColorFormat::Disabled) {
            if rec.layer != 0 || ct.layer != 0 {
                return self.skip("layered clear");
            }
            let Some(view) = color_view(&ct) else { return self.skip("color target format or layout") };
            let Some(mut t) = Staged::load(view, mem) else { return };
            let r = region(view.width, view.height);
            // Float and unorm targets take the clear value as floats.
            let color = rec.color_f32();
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    write_color(ct.format, t.texel(x, y), color, rec.color_mask);
                }
            }
            t.store(mem);
            wrote = true;
        }
        if let Some(zt) = rec.depth_target {
            let Some(view) = depth_view(&zt) else { return self.skip("depth target format or layout") };
            let Some(mut t) = Staged::load(view, mem) else { return };
            let r = region(view.width, view.height);
            let depth = rec.clear_depth.then_some(rec.depth);
            let stencil = rec.clear_stencil.then_some(rec.stencil);
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    write_depth(zt.format, t.texel(x, y), depth, stencil);
                }
            }
            t.store(mem);
            wrote = true;
        }
        if wrote {
            self.stats.clears += 1;
        }
    }

    fn stage_cbufs(&self, group: usize, f: &Function, mem: &mut dyn GpuMemoryWriter) -> Cbufs {
        let mut out = Cbufs::default();
        for op in &f.values {
            if let Op::Cbuf { bank, .. } = *op {
                if out.0.iter().any(|(b, _)| *b == bank) {
                    continue;
                }
                let Some(bind) = self.cbufs[group].get(bank as usize).copied().flatten() else { continue };
                let mut data = vec![0u8; bind.size as usize];
                if mem.read(bind.gpu_va, &mut data) {
                    out.0.push((bank, data));
                }
            }
        }
        out
    }

    /// Runs one draw: vertex fetch, vertex shader, raster, pixel shader, depth, color write.
    pub(super) fn render_draw(&mut self, rec: &DrawRecord, mem: &mut dyn GpuMemoryWriter) {
        // Probe: write-only memory keeps step-1 behaviour silently.
        if !mem.read(0, &mut []) {
            return;
        }
        if rec.class != m::AMPERE_B {
            return self.skip("MAXWELL_B shaders are Maxwell SASS, which gpu::sass does not decode");
        }
        if rec.instance != 0 {
            return self.skip("instanced draw");
        }
        let Some(tris) = triangles(rec.topology, rec.vertex_count as usize) else {
            return self.skip("topology other than triangles, strips, fans and quads");
        };
        let find = |kinds: &[super::engine::ShaderType]| rec.shaders.iter().find(|s| kinds.contains(&s.kind)).copied();
        use super::engine::ShaderType as T;
        let (Some(vs), Some(ps)) = (find(&[T::Vertex, T::VertexCullBeforeFetch]), find(&[T::Pixel])) else {
            return self.skip("draw without both a vertex and a pixel shader");
        };
        let vs = match load_shader(SassStage::Vertex, vs.program_va, mem) {
            Ok(s) => s,
            Err(reason) => return self.skip(reason),
        };
        let ps = match load_shader(SassStage::Fragment, ps.program_va, mem) {
            Ok(s) => s,
            Err(reason) => return self.skip(reason),
        };
        if ps.sph.ps_omap_depth() || ps.sph.ps_omap_sample_mask() {
            return self.skip("pixel shader writes depth or sample mask");
        }
        // Groups: vertex 0, pixel 4 (BIND_GROUP_CONSTANT_BUFFER(j) is per stage).
        let vs_cb = self.stage_cbufs(0, &vs.func, mem);
        let ps_cb = self.stage_cbufs(4, &ps.func, mem);

        // Vertex fetch + shading.
        let mut stream_cache: Vec<(u8, Vec<u8>)> = Vec::new();
        let vp = rec.viewport0;
        let mut verts = Vec::with_capacity(rec.vertex_count as usize);
        for i in 0..rec.vertex_count {
            let index = rec.first_vertex + i;
            let mut attrs = [[0u32, 0, 0, 1.0f32.to_bits()]; 32];
            for a in &rec.vertex_attributes {
                let Some(stream) = rec.vertex_streams.iter().find(|s| s.index == a.stream) else { continue };
                if !stream_cache.iter().any(|(s, _)| *s == stream.index) {
                    let mut data = vec![0u8; stream.size.min(1 << 24) as usize];
                    if !mem.read(stream.gpu_va, &mut data) {
                        return self.skip("vertex stream is not readable");
                    }
                    stream_cache.push((stream.index, data));
                }
                let data = &stream_cache.iter().find(|(s, _)| *s == stream.index).unwrap().1;
                let at = index as usize * stream.stride as usize + a.offset as usize;
                let Some(bytes) = data.get(at..) else { return self.skip("vertex fetch past the stream size") };
                let Some(mut words) = decode_attribute(a.component_bit_widths, a.numerical_type, bytes) else {
                    return self.skip("vertex attribute format");
                };
                if a.swap_r_and_b {
                    words.swap(0, 2);
                }
                attrs[a.index as usize] = words;
            }
            let values = shade::run(&vs.func, &VertexIn { attrs: &attrs, cbufs: &vs_cb });
            let mut pos = [0.0f32, 0.0, 0.0, 1.0];
            let mut varyings = [[0.0f32; 4]; 32];
            for (addr, bits) in shade::vertex_outputs(&vs.func, &values) {
                match attr_slot(addr) {
                    Some(AttrSlot::Position(c)) => pos[c as usize] = f32::from_bits(bits),
                    Some(AttrSlot::Generic { loc, comp }) if (loc as usize) < 32 => {
                        varyings[loc as usize][comp as usize] = f32::from_bits(bits)
                    }
                    _ => {}
                }
            }
            let w = pos[3];
            verts.push((
                w,
                WinVertex {
                    x: vp.scale[0] * pos[0] / w + vp.offset[0],
                    y: vp.scale[1] * pos[1] / w + vp.offset[1],
                    z: vp.scale[2] * pos[2] / w + vp.offset[2],
                    inv_w: 1.0 / w,
                    varyings,
                },
            ));
        }

        // Targets.
        let targets = selected_targets(&self.regs);
        let mut staged: Vec<Option<(ColorTarget, Staged)>> = Vec::new();
        for ct in &targets {
            match ct {
                Some(ct) if ct.layer != 0 => return self.skip("layered render target"),
                Some(ct) => {
                    let Some(view) = color_view(ct) else { return self.skip("color target format or layout") };
                    let Some(t) = Staged::load(view, mem) else { return self.skip("color target is not readable") };
                    if self.reg_at(m::SET_BLEND + 4 * ct.index as u32) & 1 == 1 {
                        self.diagnostics.push(Diagnostic::BlendNotModelled { target: ct.index });
                    }
                    staged.push(Some((*ct, t)));
                }
                None => staged.push(None),
            }
        }
        let depth_test = self.reg_at(m::SET_DEPTH_TEST) & 1 == 1;
        let depth_write = self.reg_at(m::SET_DEPTH_WRITE) & 1 == 1;
        let depth_func = self.reg_at(m::SET_DEPTH_FUNC);
        let mut zbuf = match rec.depth_target {
            Some(zt) if depth_test || depth_write => {
                let Some(view) = depth_view(&zt) else { return self.skip("depth target format or layout") };
                let Some(t) = Staged::load(view, mem) else { return self.skip("depth target is not readable") };
                Some((zt, t))
            }
            _ => None,
        };

        // Raster bounds: the smallest target, viewport clip 0, scissor 0, surface clip.
        let mut bounds = Rect::new(u32::MAX, u32::MAX);
        for (_, t) in staged.iter().flatten() {
            bounds = bounds.clip(0, 0, t.view.width, t.view.height);
        }
        if let Some((_, t)) = &zbuf {
            bounds = bounds.clip(0, 0, t.view.width, t.view.height);
        }
        let [cx, cy, cw, ch] = vp.clip;
        if cw > 0 && ch > 0 {
            bounds = bounds.clip(cx, cy, cx + cw, cy + ch);
        }
        if rec.scissor0.enabled {
            let s = rec.scissor0;
            bounds = bounds.clip(s.xmin, s.ymin, s.xmax, s.ymax);
        }
        let (sh, sv) = (self.reg_at(m::SET_SURFACE_CLIP_HORIZONTAL), self.reg_at(m::SET_SURFACE_CLIP_VERTICAL));
        if sh >> 16 > 0 && sv >> 16 > 0 {
            let (x, y) = (sh & 0xffff, sv & 0xffff);
            bounds = bounds.clip(x, y, x + (sh >> 16), y + (sv >> 16));
        }
        if bounds.x1 == u32::MAX || bounds.y1 == u32::MAX {
            return self.skip("draw with no bound target");
        }

        // Pixel shader outputs: target t takes the next popcount(mask) registers.
        let outputs: Vec<(usize, u8, usize)> = {
            let mut reg = 0;
            (0..8u32)
                .filter_map(|t| {
                    let mask = ps.sph.ps_omap_target(t);
                    (mask != 0).then(|| {
                        let first = reg;
                        reg += mask.count_ones() as usize;
                        (t as usize, mask, first)
                    })
                })
                .collect()
        };

        for tri in tris {
            let [a, b, c] = tri.map(|i| &verts[i]);
            if a.0 <= 0.0 || b.0 <= 0.0 || c.0 <= 0.0 {
                self.stats.dropped_triangles += 1;
                continue;
            }
            let (v0, v1, v2) = (&a.1, &b.1, &c.1);
            let p = [(v0.x, v0.y), (v1.x, v1.y), (v2.x, v2.y)];
            let signed = edge(p[0], p[1], p[2]);
            if signed == 0.0 || !signed.is_finite() {
                continue;
            }
            // Either winding rasterizes (no culling); edge values are flipped to be positive inside.
            let sign = signed.signum();
            let area = signed.abs();
            self.stats.triangles += 1;
            let min_x = p.iter().map(|q| q.0).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
            let min_y = p.iter().map(|q| q.1).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
            let max_x = p.iter().map(|q| q.0).fold(f32::NEG_INFINITY, f32::max).ceil().max(0.0) as u32;
            let max_y = p.iter().map(|q| q.1).fold(f32::NEG_INFINITY, f32::max).ceil().max(0.0) as u32;
            let r = bounds.clip(min_x, min_y, max_x.saturating_add(1), max_y.saturating_add(1));
            let edges = [(1, 2), (2, 0), (0, 1)];
            let tl = edges.map(|(i, j)| if sign > 0.0 { top_left(p[i], p[j]) } else { top_left(p[j], p[i]) });
            for py in r.y0..r.y1 {
                for px in r.x0..r.x1 {
                    let c = (px as f32 + 0.5, py as f32 + 0.5);
                    let e = edges.map(|(i, j)| sign * edge(p[i], p[j], c));
                    if !(0..3).all(|i| e[i] > 0.0 || (e[i] == 0.0 && tl[i])) {
                        continue;
                    }
                    let l = e.map(|v| v / area);
                    let z = l[0] * v0.z + l[1] * v1.z + l[2] * v2.z;
                    if let Some((zt, t)) = zbuf.as_mut() {
                        let texel = t.texel(px, py);
                        if depth_test && !depth_pass(depth_func, z, read_depth(zt.format, texel)) {
                            continue;
                        }
                        if depth_write {
                            write_depth(zt.format, texel, Some(z.clamp(0.0, 1.0)), None);
                        }
                    }
                    let input = FragmentIn { v: [v0, v1, v2], l, px, py, z, sph: &ps.sph, cbufs: &ps_cb };
                    let values = shade::run(&ps.func, &input);
                    for &(t, mask, first) in &outputs {
                        let Some(Some((ct, st))) = staged.get_mut(t) else { continue };
                        let mut color = [0.0f32; 4];
                        let mut reg = first;
                        for (ch, slot) in color.iter_mut().enumerate() {
                            if mask & (1 << ch) != 0 {
                                *slot = shade::exit_reg(&ps.func, &values, reg);
                                reg += 1;
                            }
                        }
                        let wm = write_mask(&self.regs, ct.index);
                        write_color(ct.format, st.texel(px, py), color, wm);
                    }
                    self.stats.fragments += 1;
                }
            }
        }
        for (_, t) in staged.iter().flatten() {
            t.store(mem);
        }
        if let Some((_, t)) = &zbuf {
            t.store(mem);
        }
        self.stats.draws += 1;
    }
}

/// Address helper shared with the engine for `SET_CONSTANT_BUFFER_SELECTOR_B/C`.
pub(crate) fn selector_va(regs: &[u32]) -> u64 {
    addr40(regs[m::SET_CONSTANT_BUFFER_SELECTOR_B as usize / 4], regs[m::SET_CONSTANT_BUFFER_SELECTOR_C as usize / 4])
}
