//! Step 3: clears and draws land in guest memory as raw block-linear / pitch surfaces, texture
//! headers and samplers decode from their pools, host and 3D semaphores block and resume.
//!
//! Shaders are synthetic Ampere SASS built with the same field positions as `gpu::sass`'s own test
//! encoder; the surfaces are read back through `gpu::tiling` the way the present side will.

use super::engine::*;
use super::methods as m;
use super::pushbuf::{immd_header, inc_header, non_inc_header};
use super::render::RenderStats;
use crate::gpu::sass::sph::{PixelImap, SphBuilder};
use crate::gpu::sass::{AMPERE_SPH_SIZE, MAXWELL_SPH_SIZE};
use crate::gpu::tiling::{self, Format, Layout, Surface};

const SC: u8 = 0;
const RT_VA: u64 = 0x01_0000_0000;
const ZT_VA: u64 = 0x01_0010_0000;
const VB_VA: u64 = 0x01_0020_0000;
const VS_VA: u64 = 0x01_0030_0000;
const PS_VA: u64 = 0x01_0030_4000;
const CB_VA: u64 = 0x01_0040_0000;
const TEX_VA: u64 = 0x01_0050_0000;
const TIC_VA: u64 = 0x01_0060_0000;
const TSC_VA: u64 = 0x01_0060_1000;
const SEM_VA: u64 = 0x01_0070_0000;

/// 256x256 RGBA8, 16-GOB-tall blocks: exactly 256 KiB.
const W: u32 = 256;
const RT_BYTES: usize = (W * W * 4) as usize;

/// Sparse GPU memory: mapped regions only, reads of unmapped bytes fail.
#[derive(Default)]
struct Mem(Vec<(u64, Vec<u8>)>);

impl Mem {
    fn map(&mut self, va: u64, size: usize) {
        self.0.push((va, vec![0; size]));
    }

    fn region(&mut self, va: u64, len: usize) -> Option<&mut [u8]> {
        self.0.iter_mut().find_map(|(base, data)| {
            let off = va.checked_sub(*base)? as usize;
            data.get_mut(off..off.checked_add(len)?)
        })
    }

    fn put(&mut self, va: u64, bytes: &[u8]) {
        self.region(va, bytes.len()).expect("mapped").copy_from_slice(bytes);
    }

    fn get(&mut self, va: u64, len: usize) -> Vec<u8> {
        self.region(va, len).expect("mapped").to_vec()
    }

    fn u32_at(&mut self, va: u64) -> u32 {
        u32::from_le_bytes(self.get(va, 4).try_into().unwrap())
    }

    fn standard() -> Self {
        let mut mem = Mem::default();
        mem.map(RT_VA, RT_BYTES);
        mem.map(ZT_VA, RT_BYTES);
        mem.map(VB_VA, 0x1000);
        mem.map(VS_VA, 0x10000);
        mem.map(CB_VA, 0x1000);
        mem.map(TEX_VA, 0x1000);
        mem.map(TIC_VA, 0x2000);
        mem.map(SEM_VA, 0x100);
        mem
    }
}

impl GpuMemoryWriter for Mem {
    fn write(&mut self, gpu_va: u64, bytes: &[u8]) {
        if let Some(dst) = self.region(gpu_va, bytes.len()) {
            dst.copy_from_slice(bytes);
        }
    }

    fn read(&mut self, gpu_va: u64, buf: &mut [u8]) -> bool {
        if buf.is_empty() {
            return true;
        }
        match self.region(gpu_va, buf.len()) {
            Some(src) => {
                buf.copy_from_slice(src);
                true
            }
            None => false,
        }
    }
}

/// Encoder for the few SASS words the tests need; field positions as in `gpu::sass::decode`.
mod asm {
    use crate::gpu::sass::decode::RZ;

    fn op(full: u16) -> u128 {
        full as u128 | (7 << 12)
    }
    fn rd(r: u8) -> u128 {
        (r as u128) << 16
    }
    pub fn exit() -> u128 {
        op(0x94d)
    }
    pub fn mov_cb(d: u8, bank: u8, off: u16) -> u128 {
        op(0xa02) | rd(d) | ((off as u128) << 38) | ((bank as u128) << 54) | (0xf << 72)
    }
    pub fn mufu_rcp(d: u8, s: u8) -> u128 {
        op(0x308) | rd(d) | ((s as u128) << 32) | (4 << 74)
    }
    pub fn ald(d: u8, addr: u16, comps: u8) -> u128 {
        op(0x321) | rd(d) | ((RZ as u128) << 24) | ((RZ as u128) << 32) | ((addr as u128) << 40) | (((comps - 1) as u128) << 74)
    }
    pub fn ast(s: u8, addr: u16, comps: u8) -> u128 {
        op(0x322)
            | ((RZ as u128) << 24)
            | ((s as u128) << 32)
            | ((addr as u128) << 40)
            | ((RZ as u128) << 64)
            | (((comps - 1) as u128) << 74)
    }
    /// `freq`: 0 PASS, 1 PassMulW, 2 CONSTANT.
    pub fn ipa(d: u8, addr: u16, freq: u8, mul: u8) -> u128 {
        op(0x326) | rd(d) | ((RZ as u128) << 24) | ((mul as u128) << 32) | (((addr >> 2) as u128) << 64) | ((freq as u128) << 78)
    }
    pub fn mov_imm(d: u8, imm: u32) -> u128 {
        op(0x802) | rd(d) | ((imm as u128) << 32) | (0xf << 72)
    }
    /// 2D `TEX` (`TLD` when `fetch`), RGBA into `d`, `d + 1`, `d2`, `d2 + 1`, coordinates `ra`,
    /// `ra + 1`, handle at `c[bank][4 * idx]`; `lod`: 0 auto, 1 LZ.
    pub fn tex(fetch: bool, d: u8, d2: u8, ra: u8, bank: u8, idx: u16, lod: u8) -> u128 {
        op(if fetch { 0xb66 } else { 0xb60 })
            | rd(d)
            | ((ra as u128) << 24)
            | ((RZ as u128) << 32)
            | ((idx as u128) << 40)
            | ((bank as u128) << 54)
            | (3 << 60)
            | ((d2 as u128) << 64)
            | (0xf << 72)
            | ((lod as u128) << 87)
    }
}

fn program(header: &SphBuilder, code: &[u128]) -> Vec<u8> {
    let mut b = header.bytes(AMPERE_SPH_SIZE);
    for w in code {
        b.extend_from_slice(&w.to_le_bytes());
    }
    b
}

/// Passthrough: position from attribute 0, generic 0 from attribute 1.
fn vertex_shader() -> Vec<u8> {
    let mut h = SphBuilder::vertex();
    for a in (0x80..0xa0).step_by(4) {
        h = h.imap(a);
    }
    for a in (0x70..0x90).step_by(4) {
        h = h.omap(a);
    }
    program(&h, &[asm::ald(0, 0x80, 4), asm::ald(4, 0x90, 4), asm::ast(0, 0x70, 4), asm::ast(4, 0x80, 4), asm::exit()])
}

/// Perspective-correct varying 0 to target 0: `IPA w; MUFU.RCP; IPA.MUL` x4.
fn varying_pixel_shader() -> Vec<u8> {
    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..4 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    let mut code = vec![asm::ipa(8, 0x7c, 0, 0), asm::mufu_rcp(9, 8)];
    code.extend((0..4).map(|c| asm::ipa(c as u8, 0x80 + 4 * c, 1, 9)));
    code.push(asm::exit());
    program(&h, &code)
}

/// Target 0 color from `c[1][0..16]`.
fn cbuf_pixel_shader() -> Vec<u8> {
    let h = SphBuilder::pixel().ps_target(0, 0xf);
    program(&h, &[asm::mov_cb(0, 1, 0), asm::mov_cb(1, 1, 4), asm::mov_cb(2, 1, 8), asm::mov_cb(3, 1, 12), asm::exit()])
}

fn hi(va: u64) -> u32 {
    (va >> 32) as u32
}

fn lo(va: u64) -> u32 {
    va as u32
}

fn f(v: f32) -> u32 {
    v.to_bits()
}

/// Class, RT 0 (A8B8G8R8, block linear 16 GOBs or pitch 1024 B), viewport 0 covering the target.
fn setup(pb: &mut Vec<u32>, class: u32, pitch: bool) {
    pb.extend([inc_header(SC, m::SET_OBJECT, 1), class]);
    let (width_field, memory) = if pitch { (W * 4, 1 << 12) } else { (W, 4 << 4) };
    pb.extend([inc_header(SC, m::SET_COLOR_TARGET_A, 9), hi(RT_VA), lo(RT_VA), width_field, W, 0xd5, memory, 1, 0, 0]);
    pb.push(immd_header(SC, m::SET_CT_SELECT, 1));
    pb.extend([inc_header(SC, m::SET_VIEWPORT_SCALE_X, 6), f(128.0), f(128.0), f(1.0), f(128.0), f(128.0), f(0.0)]);
    pb.extend([inc_header(SC, m::SET_VIEWPORT_CLIP_HORIZONTAL, 2), W << 16, W << 16]);
}

fn clear_color(pb: &mut Vec<u32>, rgba: [f32; 4]) {
    pb.push(inc_header(SC, m::SET_COLOR_CLEAR_VALUE, 4));
    pb.extend(rgba.map(f));
    pb.push(immd_header(SC, m::CLEAR_SURFACE, 0x3c));
}

/// Vertex stream 0 with `verts` (vec4 position, vec4 color), attributes 0 and 1, and both shaders.
fn geometry(pb: &mut Vec<u32>, mem: &mut Mem, verts: &[[f32; 8]], ps: &[u8]) {
    let bytes: Vec<u8> = verts.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    mem.put(VB_VA, &bytes);
    mem.put(VS_VA, &vertex_shader());
    mem.put(PS_VA, ps);
    pb.extend([inc_header(SC, m::SET_VERTEX_STREAM_A_FORMAT, 4), (1 << 12) | 32, hi(VB_VA), lo(VB_VA), 0]);
    pb.extend([inc_header(SC, m::SET_VERTEX_STREAM_SIZE_A, 2), 0, bytes.len() as u32]);
    let attr = |offset: u32| (7 << 27) | (0x01 << 21) | (offset << 7);
    pb.extend([inc_header(SC, m::SET_VERTEX_ATTRIBUTE_A, 2), attr(0), attr(16)]);
    for (pipe, kind, va) in [(1u32, 1u32, VS_VA), (5, 5, PS_VA)] {
        let base = pipe * m::PIPELINE_STRIDE;
        pb.extend([inc_header(SC, m::SET_PIPELINE_SHADER + base, 1), (kind << 4) | 1]);
        pb.extend([inc_header(SC, m::SET_PIPELINE_PROGRAM_ADDRESS_A + base, 2), hi(va), lo(va)]);
    }
}

/// `DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST`: start, count, topology.
fn draw(pb: &mut Vec<u32>, start: u32, count: u32, topology: u32) {
    pb.extend([inc_header(SC, m::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, 1), start | (count << 16) | (topology << 28)]);
}

fn vtx(x: f32, y: f32, z: f32, rgba: [f32; 4]) -> [f32; 8] {
    [x, y, z, 1.0, rgba[0], rgba[1], rgba[2], rgba[3]]
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn rt_surface() -> Surface {
    Surface::new_2d(W, W, Format::R8G8B8A8Unorm, Layout::BlockLinear { height_log2: 4, depth_log2: 0 })
}

/// Detiles the block-linear render target the way the present side does.
fn rt_pixels(mem: &mut Mem) -> Vec<u8> {
    let raw = mem.get(RT_VA, RT_BYTES);
    tiling::detile(&rt_surface(), &raw).expect("detile")
}

fn px(linear: &[u8], x: u32, y: u32) -> [u8; 4] {
    let o = ((y * W + x) * 4) as usize;
    linear[o..o + 4].try_into().unwrap()
}

fn run(pb: &[u32], mem: &mut Mem) -> Ampere3d {
    let mut gpu = Ampere3d::new();
    gpu.submit(pb, mem).expect("pushbuffer decodes");
    gpu
}

#[test]
fn clear_fills_block_linear_target_in_guest_memory() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, [0.0, 0.0, 0.25, 1.0]);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    assert_eq!(gpu.render_stats().clears, 1);
    let linear = rt_pixels(&mut mem);
    assert!(linear.chunks(4).all(|p| p == [0, 0, 64, 255]));
}

#[test]
fn clear_respects_scissor_when_asked() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, true);
    pb.extend([inc_header(SC, m::SET_SCISSOR_ENABLE, 3), 1, (20 << 16) | 10, (8 << 16) | 4]);
    pb.push(immd_header(SC, m::SET_CLEAR_SURFACE_CONTROL, 1 << 8));
    clear_color(&mut pb, [1.0, 1.0, 1.0, 1.0]);
    run(&pb, &mut mem);
    // Pitch linear: row stride 1024 bytes.
    let at = |mem: &mut Mem, x: u64, y: u64| mem.u32_at(RT_VA + y * 1024 + x * 4);
    assert_eq!(at(&mut mem, 10, 4), 0xffff_ffff);
    assert_eq!(at(&mut mem, 19, 7), 0xffff_ffff);
    assert_eq!(at(&mut mem, 9, 4), 0);
    assert_eq!(at(&mut mem, 20, 7), 0);
    assert_eq!(at(&mut mem, 10, 8), 0);
}

#[test]
fn triangle_draw_produces_raw_block_linear_surface() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, [0.0, 0.0, 0.0, 1.0]);
    // Upper-left half of the target: window (0,0), (256,0), (0,256).
    geometry(&mut pb, &mut mem, &[vtx(-1.0, -1.0, 0.5, RED), vtx(1.0, -1.0, 0.5, RED), vtx(-1.0, 1.0, 0.5, RED)], &varying_pixel_shader());
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    // 255 * 256 / 2: the hypotenuse is a right edge, so pixels centred on it stay out (top-left rule).
    assert_eq!(gpu.render_stats(), RenderStats { clears: 1, draws: 1, triangles: 1, dropped_triangles: 0, fragments: 32640 });

    let linear = rt_pixels(&mut mem);
    assert_eq!(px(&linear, 10, 10), [255, 0, 0, 255]);
    assert_eq!(px(&linear, 200, 40), [255, 0, 0, 255]);
    assert_eq!(px(&linear, 250, 250), [0, 0, 0, 255]);
    assert_eq!(px(&linear, 129, 128), [0, 0, 0, 255]);
    // The bytes in memory are block linear, not pitch: inside a GOB, row 1 starts 16 bytes in
    // (sector swizzle), not 1024 bytes in as a pitch surface would put it.
    let raw = mem.get(RT_VA, RT_BYTES);
    let off = rt_surface().byte_offset(0, 1, 0);
    assert_eq!(off, 16);
    assert_eq!(&raw[off..off + 4], &[255, 0, 0, 255]);
}

#[test]
fn varyings_interpolate_across_the_triangle() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    geometry(&mut pb, &mut mem, &[vtx(-1.0, -1.0, 0.0, RED), vtx(1.0, -1.0, 0.0, GREEN), vtx(-1.0, 1.0, 0.0, BLUE)], &varying_pixel_shader());
    draw(&mut pb, 0, 3, 4);
    run(&pb, &mut mem);
    let linear = rt_pixels(&mut mem);
    let near = |p: [u8; 4], want: [u8; 4]| p.iter().zip(want).all(|(&a, b)| (a as i32 - b as i32).abs() <= 2);
    assert!(near(px(&linear, 0, 0), [255, 0, 0, 255]), "{:?}", px(&linear, 0, 0));
    assert!(near(px(&linear, 254, 0), [0, 255, 0, 255]), "{:?}", px(&linear, 254, 0));
    assert!(near(px(&linear, 0, 254), [0, 0, 255, 255]), "{:?}", px(&linear, 0, 254));
    // Pixel (64, 64) has center (64.5, 64.5): barycentrics 0.496 / 0.252 / 0.252.
    assert!(near(px(&linear, 64, 64), [127, 64, 64, 255]), "{:?}", px(&linear, 64, 64));
}

#[test]
fn depth_test_keeps_the_nearest_quad() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    // Zf32 depth target, block linear 16 GOBs, cleared to 1.0.
    pb.extend([inc_header(SC, m::SET_ZT_A, 5), hi(ZT_VA), lo(ZT_VA), 0x0a, 4 << 4, 0]);
    pb.extend([inc_header(SC, m::SET_ZT_SIZE_A, 3), W, W, 1]);
    pb.push(immd_header(SC, m::SET_ZT_SELECT, 1));
    pb.extend([inc_header(SC, m::SET_Z_CLEAR_VALUE, 1), f(1.0)]);
    pb.push(immd_header(SC, m::CLEAR_SURFACE, 0x1));
    pb.push(immd_header(SC, m::SET_DEPTH_TEST, 1));
    pb.push(immd_header(SC, m::SET_DEPTH_WRITE, 1));
    pb.extend([inc_header(SC, m::SET_DEPTH_FUNC, 1), 0x201]); // OGL_LESS
    let quad = |z: f32, c: [f32; 4]| [vtx(-1.0, -1.0, z, c), vtx(1.0, -1.0, z, c), vtx(1.0, 1.0, z, c), vtx(-1.0, 1.0, z, c)];
    let verts: Vec<[f32; 8]> = [quad(0.5, GREEN), quad(0.8, RED), quad(0.2, BLUE), quad(0.3, RED)].concat();
    geometry(&mut pb, &mut mem, &verts, &varying_pixel_shader());
    for i in 0..4 {
        draw(&mut pb, 4 * i, 4, 7); // QUADS
    }
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    let linear = rt_pixels(&mut mem);
    assert_eq!(px(&linear, 0, 0), [0, 0, 255, 255]);
    assert_eq!(px(&linear, 255, 255), [0, 0, 255, 255]);
    assert_eq!(px(&linear, 128, 77), [0, 0, 255, 255]);
    let depth = tiling::detile(
        &Surface::new_2d(W, W, Format::Z32Float, Layout::BlockLinear { height_log2: 4, depth_log2: 0 }),
        &mem.get(ZT_VA, RT_BYTES),
    )
    .unwrap();
    assert_eq!(f32::from_le_bytes(depth[0..4].try_into().unwrap()), 0.2);
}

#[test]
fn pixel_shader_reads_constant_buffer_loaded_inline() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    geometry(&mut pb, &mut mem, &[vtx(-1.0, -1.0, 0.0, RED), vtx(1.0, -1.0, 0.0, RED), vtx(-1.0, 1.0, 0.0, RED)], &cbuf_pixel_shader());
    pb.extend([inc_header(SC, m::SET_CONSTANT_BUFFER_SELECTOR_A, 3), 256, hi(CB_VA), lo(CB_VA)]);
    pb.extend([inc_header(SC, m::LOAD_CONSTANT_BUFFER_OFFSET, 1), 0]);
    pb.push(non_inc_header(SC, m::LOAD_CONSTANT_BUFFER, 4));
    pb.extend([0.25f32, 0.5, 0.75, 1.0].map(f));
    // Pixel group (4), slot 1, valid.
    pb.extend([inc_header(SC, m::BIND_GROUP_CONSTANT_BUFFER + 4 * m::BIND_GROUP_STRIDE, 1), (1 << 4) | 1]);
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    assert_eq!(gpu.constant_buffer(4, 1), Some(ConstantBufferBinding { gpu_va: CB_VA, size: 256 }));
    assert_eq!(gpu.reg(m::LOAD_CONSTANT_BUFFER_OFFSET), 16);
    assert_eq!(mem.get(CB_VA, 4), 0.25f32.to_le_bytes());
    assert_eq!(px(&rt_pixels(&mut mem), 5, 5), [64, 128, 191, 255]);
}

#[test]
fn color_write_mask_limits_channels() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, [0.0, 0.0, 0.0, 0.0]);
    pb.extend([inc_header(SC, m::SET_CT_WRITE, 1), 0x0101]); // R and B only
    geometry(&mut pb, &mut mem, &[vtx(-1.0, -1.0, 0.0, [1.0; 4]), vtx(1.0, -1.0, 0.0, [1.0; 4]), vtx(-1.0, 1.0, 0.0, [1.0; 4])], &varying_pixel_shader());
    draw(&mut pb, 0, 3, 4);
    run(&pb, &mut mem);
    assert_eq!(px(&rt_pixels(&mut mem), 3, 3), [255, 0, 255, 0]);
}

/// Maxwell (64-bit) encoder for the same programs; field positions as in `gpu::sass::maxwell`.
mod mx {
    use crate::gpu::sass::decode::RZ;

    const PT: u64 = 7 << 16;

    fn op(top: u64) -> u64 {
        (top << 48) | PT
    }
    pub fn exit() -> u64 {
        op(0xe300) | 0xf
    }
    pub fn nop() -> u64 {
        op(0x50b0) | 0xf00
    }
    fn amem(op_: u64, r: u8, addr: u16, comps: u8) -> u64 {
        op(op_) | (((comps - 1) as u64) << 47) | ((RZ as u64) << 39) | ((addr as u64) << 20) | ((RZ as u64) << 8) | r as u64
    }
    pub fn ald(d: u8, addr: u16, comps: u8) -> u64 {
        amem(0xefd8, d, addr, comps)
    }
    pub fn ast(s: u8, addr: u16, comps: u8) -> u64 {
        amem(0xeff0, s, addr, comps)
    }
    /// `freq`: 0 PASS, 1 multiply, 2 CONSTANT.
    pub fn ipa(d: u8, addr: u16, freq: u8, mul: u8) -> u64 {
        op(0xe000) | ((freq as u64) << 54) | (7 << 47) | ((RZ as u64) << 39) | ((addr as u64) << 28) | ((mul as u64) << 20) | ((RZ as u64) << 8) | d as u64
    }
    pub fn mufu_rcp(d: u8, s: u8) -> u64 {
        op(0x5080) | (4 << 20) | ((s as u64) << 8) | d as u64
    }
    /// `TEXS` 2D RGBA: R `d`, G `d + 1`, B `d2`, A `d2 + 1`, coordinates (`ra`, `rb`), bound
    /// handle `idx`.
    pub fn texs(d: u8, d2: u8, ra: u8, rb: u8, idx: u16) -> u64 {
        (0xd8 << 56) | (1 << 53) | (4 << 50) | ((idx as u64) << 36) | ((d2 as u64) << 28) | ((rb as u64) << 20) | PT | ((ra as u64) << 8) | d as u64
    }
}

/// 0x50-byte header, then 32-byte bundles (control word + three instructions).
fn program_mx(header: &SphBuilder, code: &[u64]) -> Vec<u8> {
    let mut b = header.bytes(MAXWELL_SPH_SIZE);
    for chunk in code.chunks(3) {
        b.extend_from_slice(&0x001f_8000_fc00_07e0u64.to_le_bytes());
        for i in 0..3 {
            b.extend_from_slice(&chunk.get(i).copied().unwrap_or(mx::nop()).to_le_bytes());
        }
    }
    b
}

/// deko3d-style geometry: Maxwell passthrough vertex shader, `ps_code` behind a pixel header
/// with target 0 and perspective varying 0, one program region with per-pipeline offsets.
fn maxwell_geometry(pb: &mut Vec<u32>, mem: &mut Mem, verts: &[[f32; 8]], ps_code: &[u64]) {
    let bytes: Vec<u8> = verts.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    mem.put(VB_VA, &bytes);
    let mut vs_h = SphBuilder::vertex();
    for a in (0x80..0xa0).step_by(4) {
        vs_h = vs_h.imap(a);
    }
    for a in (0x70..0x90).step_by(4) {
        vs_h = vs_h.omap(a);
    }
    let vs = program_mx(&vs_h, &[mx::ald(0, 0x80, 4), mx::ald(4, 0x90, 4), mx::ast(0, 0x70, 4), mx::ast(4, 0x80, 4), mx::exit()]);
    let mut ps_h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..4 {
        ps_h = ps_h.ps_imap(0, c, PixelImap::Perspective);
    }
    mem.put(VS_VA, &vs);
    mem.put(PS_VA, &program_mx(&ps_h, ps_code));
    pb.extend([inc_header(SC, m::SET_VERTEX_STREAM_A_FORMAT, 4), (1 << 12) | 32, hi(VB_VA), lo(VB_VA), 0]);
    let limit = VB_VA + bytes.len() as u64 - 1;
    pb.extend([inc_header(SC, m::B197_SET_VERTEX_STREAM_LIMIT_A_A, 2), hi(limit), lo(limit)]);
    let attr = |offset: u32| (7 << 27) | (0x01 << 21) | (offset << 7);
    pb.extend([inc_header(SC, m::SET_VERTEX_ATTRIBUTE_A, 2), attr(0), attr(16)]);
    pb.extend([inc_header(SC, m::B197_SET_PROGRAM_REGION_A, 2), hi(VS_VA), lo(VS_VA)]);
    for (pipe, kind, offset) in [(1u32, 1u32, 0u32), (5, 5, (PS_VA - VS_VA) as u32)] {
        let base = pipe * m::PIPELINE_STRIDE;
        pb.extend([inc_header(SC, m::SET_PIPELINE_SHADER + base, 1), (kind << 4) | 1]);
        pb.extend([inc_header(SC, m::B197_SET_PIPELINE_PROGRAM + base, 1), offset]);
    }
}

#[test]
fn maxwell_draw_renders_through_the_maxwell_front_end() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::MAXWELL_B, false);
    clear_color(&mut pb, [0.0, 0.0, 0.0, 1.0]);
    let verts = [vtx(-1.0, -1.0, 0.0, RED), vtx(1.0, -1.0, 0.0, GREEN), vtx(-1.0, 1.0, 0.0, BLUE)];
    let mut code = vec![mx::ipa(8, 0x7c, 0, 0), mx::mufu_rcp(9, 8)];
    code.extend((0..4).map(|c| mx::ipa(c as u8, 0x80 + 4 * c, 1, 9)));
    code.push(mx::exit());
    maxwell_geometry(&mut pb, &mut mem, &verts, &code);
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    assert_eq!(gpu.render_stats().draws, 1);
    let linear = rt_pixels(&mut mem);
    let near = |p: [u8; 4], want: [u8; 4]| p.iter().zip(want).all(|(&a, b)| (a as i32 - b as i32).abs() <= 2);
    assert!(near(px(&linear, 0, 0), [255, 0, 0, 255]), "{:?}", px(&linear, 0, 0));
    assert!(near(px(&linear, 64, 64), [127, 64, 64, 255]), "{:?}", px(&linear, 64, 64));
    assert_eq!(px(&linear, 250, 250), [0, 0, 0, 255]);
}

#[test]
fn unsupported_shader_skips_the_draw() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    // A vertex shader without EXIT inside the readable range.
    geometry(&mut pb, &mut mem, &[vtx(-1.0, -1.0, 0.0, RED), vtx(1.0, -1.0, 0.0, RED), vtx(-1.0, 1.0, 0.0, RED)], &varying_pixel_shader());
    mem.put(VS_VA + AMPERE_SPH_SIZE as u64, &[0xff; 16]);
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert_eq!(gpu.render_stats().draws, 0);
    assert!(matches!(gpu.diagnostics(), [Diagnostic::RenderSkipped { .. }]), "{:?}", gpu.diagnostics());
}

// --- Blending --------------------------------------------------------------------------------

/// Clears target 0 to `clear`, then draws a full-target triangle whose pixel shader outputs `src`
/// (from `c[1]`), after the blend state in `state`.
fn blended(clear: [f32; 4], src: [f32; 4], state: &[u32]) -> (Ampere3d, Vec<u8>) {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, clear);
    pb.extend_from_slice(state);
    let tri = [vtx(-1.0, -1.0, 0.0, RED), vtx(3.0, -1.0, 0.0, RED), vtx(-1.0, 3.0, 0.0, RED)];
    geometry(&mut pb, &mut mem, &tri, &cbuf_pixel_shader());
    pb.extend([inc_header(SC, m::SET_CONSTANT_BUFFER_SELECTOR_A, 3), 256, hi(CB_VA), lo(CB_VA)]);
    pb.extend([inc_header(SC, m::LOAD_CONSTANT_BUFFER_OFFSET, 1), 0]);
    pb.push(non_inc_header(SC, m::LOAD_CONSTANT_BUFFER, 4));
    pb.extend(src.map(f));
    pb.extend([inc_header(SC, m::BIND_GROUP_CONSTANT_BUFFER + 4 * m::BIND_GROUP_STRIDE, 1), (1 << 4) | 1]);
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    let linear = rt_pixels(&mut mem);
    (gpu, linear)
}

/// Shared blend registers: separate-alpha, color op/src/dst, alpha op/src/dst.
fn shared_blend(regs: [u32; 7]) -> Vec<u32> {
    let mut pb = vec![immd_header(SC, m::SET_BLEND, 1), inc_header(SC, m::SET_BLEND_SEPARATE_FOR_ALPHA, 7)];
    pb.extend(regs);
    // 0x1354 sits between ALPHA_SOURCE_COEFF and ALPHA_DEST_COEFF; rewrite the last one alone.
    pb.extend([inc_header(SC, m::SET_BLEND_ALPHA_DEST_COEFF, 1), regs[6]]);
    pb
}

#[test]
fn alpha_blending_mixes_with_the_target() {
    // GL: ADD, SRC_ALPHA, ONE_MINUS_SRC_ALPHA; separate alpha off, so alpha uses it too.
    let state = shared_blend([0, 0x8006, 0x4302, 0x4303, 0, 0, 0]);
    let (gpu, linear) = blended([0.0, 0.0, 1.0, 1.0], [1.0, 0.0, 0.0, 0.25], &state);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    // 0.25 red + 0.75 blue; alpha 0.25 * 0.25 + 0.75 * 1 = 0.8125.
    assert_eq!(px(&linear, 10, 10), [64, 0, 191, 207]);
    assert_eq!(px(&linear, 250, 250), [64, 0, 191, 207]);
}

#[test]
fn per_target_d3d_blend_with_separate_alpha() {
    // Per-target block for target 0: D3D REVSUBTRACT ONE ONE for color, MAX for alpha.
    let mut state = vec![immd_header(SC, m::SET_BLEND, 1), immd_header(SC, m::SET_BLEND_STATE_PER_TARGET, 1)];
    state.push(inc_header(SC, m::SET_BLEND_PER_TARGET, 7));
    state.extend([1, 3, 2, 2, 5, 2, 2]);
    // The shared registers hold something unsupported; per-target mode must ignore them.
    state.extend([inc_header(SC, m::SET_BLEND_COLOR_SOURCE_COEFF, 1), 0xc900]);
    let (gpu, linear) = blended([0.5, 0.5, 0.5, 0.5], [0.25, 0.75, 0.0, 1.0], &state);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    // dst - src per color channel (128/255 - src), clamped at 0; alpha max(1, 0.5).
    assert_eq!(px(&linear, 100, 100), [64, 0, 128, 255]);
}

#[test]
fn constant_color_blend_factor() {
    // ADD, CONSTANT_COLOR, ZERO: src * constant.
    let mut state = shared_blend([0, 0x8006, 0xc001, 0x4000, 0, 0, 0]);
    state.push(inc_header(SC, m::SET_BLEND_CONST_RED, 4));
    state.extend([0.5f32, 1.0, 0.0, 2.0].map(f));
    let (_, linear) = blended([0.0; 4], [1.0, 0.5, 1.0, 1.0], &state);
    // The constant clamps to 1 on a unorm target, so alpha stays 1.
    assert_eq!(px(&linear, 10, 10), [128, 128, 0, 255]);
}

#[test]
fn dual_source_blending_skips_the_draw() {
    let state = shared_blend([0, 0x8006, 0xc900, 0x4000, 0, 0, 0]);
    let (gpu, linear) = blended([0.0, 1.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], &state);
    assert_eq!(gpu.diagnostics(), &[Diagnostic::RenderSkipped { reason: "dual-source blending" }]);
    assert_eq!(gpu.render_stats().draws, 0);
    assert_eq!(px(&linear, 10, 10), [0, 255, 0, 255]);
}

// --- Indexed draws ---------------------------------------------------------------------------

const IB_VA: u64 = VB_VA + 0x800;

fn index_buffer(pb: &mut Vec<u32>, mem: &mut Mem, va: u64, bytes: &[u8]) {
    if !bytes.is_empty() {
        mem.put(va, bytes);
    }
    pb.extend([inc_header(SC, m::SET_INDEX_BUFFER_A, 2), hi(va), lo(va)]);
}

fn red_quad_vertices() -> Vec<[f32; 8]> {
    vec![vtx(-1.0, -1.0, 0.0, RED), vtx(1.0, -1.0, 0.0, RED), vtx(-1.0, 1.0, 0.0, RED), vtx(1.0, 1.0, 0.0, RED)]
}

#[test]
fn indexed_triangles_from_a_16_bit_buffer() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, [0.0, 0.0, 0.0, 1.0]);
    geometry(&mut pb, &mut mem, &red_quad_vertices(), &varying_pixel_shader());
    let idx: Vec<u8> = [0u16, 1, 2, 2, 1, 3].iter().flat_map(|i| i.to_le_bytes()).collect();
    index_buffer(&mut pb, &mut mem, IB_VA, &idx);
    // FIRST 0, COUNT 6, TRIANGLES. Read sequentially, positions 4 and 5 would fetch past the
    // four-vertex stream and skip the draw.
    pb.extend([inc_header(SC, m::DRAW_INDEX_BUFFER16_BEGIN_END_INSTANCE_FIRST, 1), (6 << 16) | (4 << 28)]);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    assert_eq!((gpu.render_stats().draws, gpu.render_stats().triangles), (1, 2));
    let linear = rt_pixels(&mut mem);
    assert_eq!(px(&linear, 5, 5), [255, 0, 0, 255]);
    assert_eq!(px(&linear, 250, 250), [255, 0, 0, 255]);
    match gpu.events().last() {
        Some(Event::Draw(d)) => assert_eq!(d.indices.map(|i| (i.gpu_va, i.index_bytes)), Some((IB_VA, 2))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn indexed_strip_with_first_base_vertex_and_restart() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, [0.0, 0.0, 0.0, 1.0]);
    // Vertex 0 is green and must never be fetched: the base vertex shifts every index by one.
    let mut verts = vec![vtx(0.0, 0.0, 0.0, GREEN)];
    verts.extend(red_quad_vertices());
    geometry(&mut pb, &mut mem, &verts, &varying_pixel_shader());
    // 32-bit indices from FIRST 1: [0, 1, 2], restart, [2, 1, 3] as two triangle strips.
    let restart = u32::MAX;
    let idx: Vec<u8> = [7u32, 0, 1, 2, restart, 2, 1, 3].iter().flat_map(|i| i.to_le_bytes()).collect();
    index_buffer(&mut pb, &mut mem, IB_VA, &idx);
    pb.extend([inc_header(SC, m::SET_INDEX_BUFFER_E, 2), 2, 1]);
    pb.extend([inc_header(SC, m::SET_GLOBAL_BASE_VERTEX_INDEX, 1), 1]);
    pb.extend([inc_header(SC, m::SET_DA_PRIMITIVE_RESTART, 2), 1, restart]);
    pb.extend([inc_header(SC, m::BEGIN, 1), 5]);
    pb.extend([inc_header(SC, m::DRAW_INDEX_BUFFER, 1), 7]);
    pb.extend([inc_header(SC, m::END, 1), 0]);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    assert_eq!(gpu.render_stats().triangles, 2);
    let linear = rt_pixels(&mut mem);
    for (x, y) in [(5, 5), (250, 250), (128, 30), (30, 128)] {
        assert_eq!(px(&linear, x, y), [255, 0, 0, 255], "pixel ({x}, {y})");
    }
}

#[test]
fn index_buffer_bounds_and_mapping_are_checked() {
    let draw_with = |va: u64, size: u32| {
        let mut mem = Mem::standard();
        let mut pb = Vec::new();
        setup(&mut pb, m::AMPERE_B, false);
        geometry(&mut pb, &mut mem, &red_quad_vertices(), &varying_pixel_shader());
        let idx: Vec<u8> = [0u8, 1, 2].to_vec();
        if va == IB_VA {
            mem.put(IB_VA, &idx);
        }
        index_buffer(&mut pb, &mut mem, va, &[]);
        pb.extend([inc_header(SC, m::SET_INDEX_BUFFER_SIZE_A, 2), 0, size]);
        pb.extend([inc_header(SC, m::DRAW_INDEX_BUFFER8_BEGIN_END_INSTANCE_FIRST, 1), (3 << 16) | (4 << 28)]);
        run(&pb, &mut mem)
    };
    let ok = draw_with(IB_VA, 3);
    assert!(ok.diagnostics().is_empty(), "{:?}", ok.diagnostics());
    let short = draw_with(IB_VA, 2);
    assert_eq!(short.diagnostics(), &[Diagnostic::RenderSkipped { reason: "index fetch past the index buffer size" }]);
    let unmapped = draw_with(0x02_0000_0000, 0);
    assert_eq!(unmapped.diagnostics(), &[Diagnostic::RenderSkipped { reason: "index buffer is not readable" }]);
}

// --- Textures --------------------------------------------------------------------------------

/// Block-linear 16x16 A8B8G8R8 header (version 3), 2-GOB blocks, normalized coordinates.
fn tic_block_linear(va: u64) -> [u32; 8] {
    let mut w = [0u32; 8];
    let unorm = 2;
    w[0] = 0x08 | (unorm << 7) | (unorm << 10) | (unorm << 13) | (unorm << 16) | (2 << 19) | (3 << 22) | (4 << 25) | (5 << 28);
    w[1] = va as u32 & !0x1ff;
    w[2] = ((va >> 32) as u32 & 0x1ffff) | (3 << 21);
    w[3] = 1 << 3; // GOBS_PER_BLOCK_HEIGHT = 2 GOBs
    w[4] = 15 | (1 << 23); // WIDTH_MINUS_ONE, TWO_D
    w[5] = 15 | (1 << 31); // HEIGHT_MINUS_ONE, NORMALIZED_COORDS
    w
}

fn words_to_bytes(w: &[u32; 8]) -> Vec<u8> {
    w.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn texture_memory() -> (Mem, Ampere3d) {
    let mut mem = Mem::standard();
    let linear: Vec<u8> = (0..16u32).flat_map(|y| (0..16u32).flat_map(move |x| [(x * 16) as u8, (y * 16) as u8, 0, 255])).collect();
    let surf = Surface::new_2d(16, 16, Format::R8G8B8A8Unorm, Layout::BlockLinear { height_log2: 1, depth_log2: 0 });
    mem.put(TEX_VA, &tiling::tile(&surf, &linear).unwrap());
    mem.put(TIC_VA + 2 * 32, &words_to_bytes(&tic_block_linear(TEX_VA)));
    // Pitch view of the same bytes would be wrong; this one points at a separate pitch copy.
    let pitch_va = TEX_VA + 0x800;
    mem.put(pitch_va, &linear);
    let mut pitch = tic_block_linear(pitch_va);
    pitch[1] = pitch_va as u32 & !0x1f;
    pitch[2] = ((pitch_va >> 32) as u32) | (2 << 21);
    pitch[3] = 64 >> 5; // PITCH_BITS20TO5
    mem.put(TIC_VA + 3 * 32, &words_to_bytes(&pitch));
    // Sampler 0: clamp to edge, point. Sampler 1: clamp to edge, linear.
    let mut tsc = [0u32; 8];
    tsc[0] = 2 | (2 << 3) | (2 << 6);
    tsc[1] = 1 | (1 << 4);
    mem.put(TSC_VA, &words_to_bytes(&tsc));
    tsc[1] = 2 | (2 << 4);
    mem.put(TSC_VA + 32, &words_to_bytes(&tsc));

    let mut pb = vec![inc_header(SC, m::SET_OBJECT, 1), m::AMPERE_B];
    pb.extend([inc_header(SC, m::SET_TEX_HEADER_POOL_A, 3), hi(TIC_VA), lo(TIC_VA), 7]);
    pb.extend([inc_header(SC, m::SET_TEX_SAMPLER_POOL_A, 3), hi(TSC_VA), lo(TSC_VA), 1]);
    let gpu = run(&pb, &mut mem);
    (mem, gpu)
}

#[test]
fn texture_header_decodes_from_pool() {
    use super::texture::*;
    let (mut mem, gpu) = texture_memory();
    let h = gpu.texture_header(2, &mut mem).expect("header");
    assert_eq!(h.version, HeaderVersion::BlockLinear);
    assert_eq!(h.gpu_va, TEX_VA);
    assert_eq!((h.width, h.height, h.depth), (16, 16, 1));
    assert_eq!(h.texture_type, TextureType::TwoD);
    assert_eq!(h.layout, TextureLayout::BlockLinear { block_height_log2: 1, block_depth_log2: 0 });
    assert_eq!(h.data_types, [DataType::Unorm; 4]);
    assert_eq!(h.swizzle, [Swizzle::R, Swizzle::G, Swizzle::B, Swizzle::A]);
    assert!(h.normalized_coords);
    let p = gpu.texture_header(3, &mut mem).expect("pitch header");
    assert_eq!(p.layout, TextureLayout::Pitch { pitch_bytes: 64 });
    assert_eq!(p.gpu_va, TEX_VA + 0x800);
    // MAXIMUM_INDEX 7 bounds the pool.
    assert!(gpu.texture_header(8, &mut mem).is_none());
    let s = gpu.sampler(1, &mut mem).expect("sampler");
    assert_eq!(s.address, [AddressMode::ClampToEdge; 3]);
    assert_eq!(s.mag_filter, Filter::Linear);
    assert!(gpu.sampler(2, &mut mem).is_none());
}

#[test]
fn texel_fetch_and_sampling_read_tiled_and_pitch_textures() {
    let (mut mem, gpu) = texture_memory();
    let bl = gpu.texture_header(2, &mut mem).unwrap();
    let pitch = gpu.texture_header(3, &mut mem).unwrap();
    let want = [48.0 / 255.0, 80.0 / 255.0, 0.0, 1.0];
    assert_eq!(bl.fetch(3, 5, &mut mem), Some(want));
    assert_eq!(pitch.fetch(3, 5, &mut mem), Some(want));
    assert_eq!(bl.fetch(16, 0, &mut mem), None);

    let point = gpu.sampler(0, &mut mem).unwrap();
    let linear = gpu.sampler(1, &mut mem).unwrap();
    assert_eq!(bl.sample(&point, 3.5 / 16.0, 5.5 / 16.0, &mut mem), Some(want));
    // Halfway between texels 3 and 4 in x: red 48 and 64 average to 56.
    let s = bl.sample(&linear, 4.0 / 16.0, 5.5 / 16.0, &mut mem).unwrap();
    assert!((s[0] - 56.0 / 255.0).abs() < 1e-6 && (s[1] - 80.0 / 255.0).abs() < 1e-6, "{s:?}");
    // Clamp to edge past the right border.
    assert_eq!(bl.sample(&point, 1.5, 0.0, &mut mem).unwrap()[0], 240.0 / 255.0);
}

/// Texture pools from [`texture_memory`] and a pixel-stage constant buffer in slot 1 whose
/// words are `words`.
fn texture_state(pb: &mut Vec<u32>, words: &[u32]) {
    pb.extend([inc_header(SC, m::SET_TEX_HEADER_POOL_A, 3), hi(TIC_VA), lo(TIC_VA), 7]);
    pb.extend([inc_header(SC, m::SET_TEX_SAMPLER_POOL_A, 3), hi(TSC_VA), lo(TSC_VA), 1]);
    pb.extend([inc_header(SC, m::SET_CONSTANT_BUFFER_SELECTOR_A, 3), 256, hi(CB_VA), lo(CB_VA)]);
    pb.extend([inc_header(SC, m::LOAD_CONSTANT_BUFFER_OFFSET, 1), 0]);
    pb.push(non_inc_header(SC, m::LOAD_CONSTANT_BUFFER, words.len() as u32));
    pb.extend(words);
    pb.extend([inc_header(SC, m::BIND_GROUP_CONSTANT_BUFFER + 4 * m::BIND_GROUP_STRIDE, 1), (1 << 4) | 1]);
}

/// Texel colour of [`texture_memory`]'s 16x16 texture: red = 16 x, green = 16 y.
fn texel(x: u32, y: u32) -> [u8; 4] {
    [(x * 16) as u8, (y * 16) as u8, 0, 255]
}

/// (u, v) in the colour slot: (0, 0) at the window origin, (1, 1) at the far corner.
fn uv_triangle() -> [[f32; 8]; 3] {
    [vtx(-1.0, -1.0, 0.0, [0.0, 0.0, 0.0, 1.0]), vtx(1.0, -1.0, 0.0, [1.0, 0.0, 0.0, 1.0]), vtx(-1.0, 1.0, 0.0, [0.0, 1.0, 0.0, 1.0])]
}

#[test]
fn ampere_pixel_shader_samples_a_texture() {
    let (mut mem, _) = texture_memory();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    // Handle word in c[1][0]: TIC 2 (block linear), TSC 0 (point, clamp).
    texture_state(&mut pb, &[2]);
    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..2 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    let code = [
        asm::ipa(8, 0x7c, 0, 0),
        asm::mufu_rcp(9, 8),
        asm::ipa(4, 0x80, 1, 9),
        asm::ipa(5, 0x84, 1, 9),
        asm::tex(false, 0, 2, 4, 1, 0, 0),
        asm::exit(),
    ];
    geometry(&mut pb, &mut mem, &uv_triangle(), &program(&h, &code));
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    assert_eq!(gpu.render_stats().draws, 1);
    let linear = rt_pixels(&mut mem);
    // Window pixel (x, y) samples u = (x + 0.5) / 256: texel x / 16.
    for (x, y) in [(40, 100), (100, 40), (5, 5), (130, 120)] {
        assert_eq!(px(&linear, x, y), texel(x / 16, y / 16), "pixel ({x}, {y})");
    }
}

#[test]
fn ampere_pixel_shader_fetches_a_texel() {
    let (mut mem, _) = texture_memory();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    // TIC 3 is the pitch copy; TLD needs no sampler.
    texture_state(&mut pb, &[0, 3]);
    let h = SphBuilder::pixel().ps_target(0, 0xf);
    let code = [asm::mov_imm(4, 3), asm::mov_imm(5, 5), asm::tex(true, 0, 2, 4, 1, 1, 1), asm::exit()];
    geometry(&mut pb, &mut mem, &uv_triangle(), &program(&h, &code));
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    let linear = rt_pixels(&mut mem);
    assert_eq!(px(&linear, 10, 10), texel(3, 5));
    assert_eq!(px(&linear, 200, 20), texel(3, 5));
}

#[test]
fn maxwell_texs_reads_the_handle_through_set_bindless_texture() {
    let (mut mem, _) = texture_memory();
    let mut pb = Vec::new();
    setup(&mut pb, m::MAXWELL_B, false);
    // Bound handle index 2 -> c[1][8] once SET_BINDLESS_TEXTURE selects slot 1: TIC 2, TSC 0.
    texture_state(&mut pb, &[0, 0, 2]);
    pb.extend([inc_header(SC, m::SET_BINDLESS_TEXTURE, 1), 1]);
    let code = [mx::ipa(8, 0x7c, 0, 0), mx::mufu_rcp(9, 8), mx::ipa(4, 0x80, 1, 9), mx::ipa(5, 0x84, 1, 9), mx::texs(0, 2, 4, 5, 2), mx::exit()];
    maxwell_geometry(&mut pb, &mut mem, &uv_triangle(), &code);
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    let linear = rt_pixels(&mut mem);
    for (x, y) in [(40, 100), (100, 40), (130, 120)] {
        assert_eq!(px(&linear, x, y), texel(x / 16, y / 16), "pixel ({x}, {y})");
    }

    // VIA_HEADER_BINDING: TSC index = TIC index (2), outside the 2-entry sampler pool.
    pb.extend([inc_header(SC, m::SET_SAMPLER_BINDING, 1), 1]);
    draw(&mut pb, 0, 3, 4);
    let gpu = run(&pb, &mut mem);
    assert_eq!(gpu.render_stats().draws, 1);
    assert_eq!(gpu.diagnostics(), &[Diagnostic::RenderSkipped { reason: "sampler is not readable" }]);
}

// --- Semaphores ------------------------------------------------------------------------------

fn host_semaphore(pb: &mut Vec<u32>, va: u64, payload: u32, d: u32) {
    pb.extend([inc_header(SC, m::HOST_SEMAPHOREA, 4), hi(va), lo(va), payload, d]);
}

#[test]
fn host_acquire_blocks_until_released() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    host_semaphore(&mut pb, SEM_VA, 5, 1); // ACQUIRE (equal)
    pb.extend([inc_header(SC, m::HOST_SET_REFERENCE, 1), 77]);
    let mut gpu = run(&pb, &mut mem);
    let wait = gpu.blocked_on().expect("blocked");
    assert_eq!((wait.gpu_va, wait.payload, wait.op), (SEM_VA, 5, HostSemaphoreOp::AcquireEqual));
    assert_eq!(gpu.reference(), 0);
    // Later segments queue behind the wait.
    gpu.submit(&[inc_header(SC, m::HOST_SET_REFERENCE, 1), 78], &mut mem).unwrap();
    assert!(!gpu.resume(&mut mem));
    mem.put(SEM_VA, &5u32.to_le_bytes());
    assert!(gpu.resume(&mut mem));
    assert_eq!(gpu.reference(), 78);
    assert!(gpu.events().contains(&Event::Host(HostRecord::SetReference(77))));
}

#[test]
fn host_release_sizes_and_reduction() {
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    host_semaphore(&mut pb, SEM_VA, 0xabcd, 2 | (1 << 24)); // RELEASE, 4 bytes
    host_semaphore(&mut pb, SEM_VA + 0x10, 0x1234, 2); // RELEASE, 16 bytes
    mem.put(SEM_VA + 0x40, &10u32.to_le_bytes());
    host_semaphore(&mut pb, SEM_VA + 0x40, 5, 0x10 | (5 << 27) | (1 << 31)); // REDUCTION ADD unsigned
    let gpu = run(&pb, &mut mem);
    assert!(gpu.blocked_on().is_none());
    assert_eq!(mem.get(SEM_VA, 8), [0xcd, 0xab, 0, 0, 0, 0, 0, 0]);
    assert_eq!(mem.get(SEM_VA + 0x10, 16), [[0x34, 0x12, 0, 0, 0, 0, 0, 0], 1u64.to_le_bytes()].concat());
    assert_eq!(mem.u32_at(SEM_VA + 0x40), 15);
}

#[test]
fn sem_execute_wide_release_and_wrapping_acquire() {
    let mut mem = Mem::standard();
    let sem = |pb: &mut Vec<u32>, payload: u64, exec: u32| {
        pb.extend([inc_header(SC, m::HOST_SEM_ADDR_LO, 5), lo(SEM_VA), hi(SEM_VA), payload as u32, (payload >> 32) as u32, exec]);
    };
    let mut pb = Vec::new();
    sem(&mut pb, 0x1_0000_0002, 1 | (1 << 24)); // RELEASE, 64-bit payload
    let gpu = run(&pb, &mut mem);
    assert_eq!(mem.get(SEM_VA, 8), 0x1_0000_0002u64.to_le_bytes());
    assert!(gpu.blocked_on().is_none());

    mem.put(SEM_VA, &[2, 0, 0, 0, 0, 0, 0, 0]);
    // 2 is "after" 0xffff_fffe once the counter wraps, but not by plain unsigned comparison.
    let mut pb = Vec::new();
    sem(&mut pb, 0xffff_fffe, 3); // ACQ_CIRC_GEQ, 32-bit
    assert!(run(&pb, &mut mem).blocked_on().is_none());
    let mut pb = Vec::new();
    sem(&mut pb, 0xffff_fffe, 2); // ACQ_STRICT_GEQ
    assert_eq!(run(&pb, &mut mem).blocked_on().map(|w| w.op), Some(HostSemaphoreOp::AcquireStrictGeq));
}

#[test]
fn report_semaphore_acquire_waits_for_greater_or_equal() {
    let mut mem = Mem::standard();
    mem.put(SEM_VA, &3u32.to_le_bytes());
    let mut pb = vec![inc_header(SC, m::SET_OBJECT, 1), m::AMPERE_B];
    // ACQUIRE, COMPARISON GE, ONE_WORD.
    pb.extend([inc_header(SC, m::SET_REPORT_SEMAPHORE_A, 4), hi(SEM_VA), lo(SEM_VA), 4, 1 | (1 << 16) | (1 << 28)]);
    pb.extend([inc_header(SC, m::SET_REPORT_SEMAPHORE_A, 4), hi(SEM_VA + 8), lo(SEM_VA + 8), 9, 1 << 28]);
    let mut gpu = run(&pb, &mut mem);
    assert!(gpu.blocked_on().is_some());
    assert_eq!(mem.u32_at(SEM_VA + 8), 0);
    mem.put(SEM_VA, &7u32.to_le_bytes());
    assert!(gpu.resume(&mut mem));
    assert_eq!(mem.u32_at(SEM_VA + 8), 9);
}

#[test]
fn write_only_memory_keeps_records_only() {
    // The step-1 contract: without reads, nothing renders and acquires never block.
    struct WriteOnly(usize);
    impl GpuMemoryWriter for WriteOnly {
        fn write(&mut self, _: u64, _: &[u8]) {
            self.0 += 1;
        }
    }
    let mut mem = Mem::standard();
    let mut pb = Vec::new();
    setup(&mut pb, m::AMPERE_B, false);
    clear_color(&mut pb, RED);
    geometry(&mut pb, &mut mem, &[vtx(-1.0, -1.0, 0.0, RED), vtx(1.0, -1.0, 0.0, RED), vtx(-1.0, 1.0, 0.0, RED)], &varying_pixel_shader());
    draw(&mut pb, 0, 3, 4);
    host_semaphore(&mut pb, SEM_VA, 5, 1);
    let mut wo = WriteOnly(0);
    let mut gpu = Ampere3d::new();
    gpu.submit(&pb, &mut wo).unwrap();
    assert_eq!(wo.0, 0);
    assert_eq!(gpu.render_stats(), RenderStats::default());
    assert!(gpu.blocked_on().is_none());
    assert!(gpu.diagnostics().is_empty());
}
