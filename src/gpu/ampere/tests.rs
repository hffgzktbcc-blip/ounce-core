//! Synthetic command streams: clear plus one triangle, written the way a driver for AMPERE_B
//! would program it, checked against the decoded state and the resulting records.

use super::engine::*;
use super::methods as m;
use super::pushbuf::{immd_header, inc_header, non_inc_header, one_inc_header};
use super::surface::*;

const SC: u8 = 0;
const RT_VA: u64 = 0x01_2340_0000;
const VB_VA: u64 = 0x01_5000_0000;
const VS_VA: u64 = 0x01_6000_0100;
const PS_VA: u64 = 0x01_6000_0800;
const SEM_VA: u64 = 0x01_7000_0040;
const SHADER_REGION: u64 = 0x01_6000_0000;

#[derive(Default)]
struct Writes(Vec<(u64, Vec<u8>)>);

impl GpuMemoryWriter for Writes {
    fn write(&mut self, gpu_va: u64, bytes: &[u8]) {
        self.0.push((gpu_va, bytes.to_vec()));
    }
}

fn hi(va: u64) -> u32 {
    (va >> 32) as u32
}

fn lo(va: u64) -> u32 {
    va as u32
}

fn bind_class(pb: &mut Vec<u32>, class: u32) {
    pb.extend([inc_header(SC, m::SET_OBJECT, 1), class]);
}

/// 256x256 A8B8G8R8 render target 0. `pitch` selects pitch linear (stride 1024 B) or block linear
/// with 16-GOB-tall blocks.
fn bind_render_target(pb: &mut Vec<u32>, pitch: bool) {
    let (width_field, memory) = if pitch { (1024, 1 << 12) } else { (256, 4 << 4) };
    pb.extend([inc_header(SC, m::SET_COLOR_TARGET_A, 9), hi(RT_VA), lo(RT_VA), width_field, 256, 0xd5, memory, 1, 0, 0]);
    // TARGET_COUNT = 1, TARGET0 = slot 0.
    pb.extend([immd_header(SC, m::SET_CT_SELECT, 1)]);
    pb.extend([inc_header(SC, m::SET_SURFACE_CLIP_HORIZONTAL, 2), 256 << 16, 256 << 16]);
}

fn clear_dark_blue(pb: &mut Vec<u32>) {
    pb.extend([inc_header(SC, m::SET_COLOR_CLEAR_VALUE, 4)]);
    pb.extend([0.0f32, 0.0, 0.25, 1.0].map(f32::to_bits));
    // R, G, B, A enabled (bits 5:2), MRT 0, layer 0.
    pb.extend([immd_header(SC, m::CLEAR_SURFACE, 0x3c)]);
}

/// Same triangle state on either class; only shader addressing and stream bounds differ.
fn setup_triangle(pb: &mut Vec<u32>, class: u32) {
    // Viewport 0: 256x256, y down, z 0..1.
    pb.extend([inc_header(SC, m::SET_VIEWPORT_SCALE_X, 6)]);
    pb.extend([128.0f32, 128.0, 1.0, 128.0, 128.0, 0.0].map(f32::to_bits));
    pb.extend([inc_header(SC, m::SET_VIEWPORT_CLIP_HORIZONTAL, 2), 256 << 16, 256 << 16]);
    pb.extend([inc_header(SC, m::SET_SCISSOR_ENABLE, 3), 1, 256 << 16, 256 << 16]);
    // Stream 0: 3 vertices of vec3 position + vec4 color = 28 bytes each.
    pb.extend([inc_header(SC, m::SET_VERTEX_STREAM_A_FORMAT, 4), (1 << 12) | 28, hi(VB_VA), lo(VB_VA), 0]);
    if class == m::MAXWELL_B {
        let limit = VB_VA + 84 - 1;
        pb.extend([inc_header(SC, m::B197_SET_VERTEX_STREAM_LIMIT_A_A, 2), hi(limit), lo(limit)]);
        // Shader code lives at region + per-pipeline offset.
        pb.extend([inc_header(SC, m::B197_SET_PROGRAM_REGION_A, 2), hi(SHADER_REGION), lo(SHADER_REGION)]);
    } else {
        pb.extend([inc_header(SC, m::SET_VERTEX_STREAM_SIZE_A, 2), 0, 84]);
    }
    // Attribute 0: stream 0, offset 0, R32_G32_B32 float. Attribute 1: offset 12, R32_G32_B32_A32.
    let attr = |offset: u32, widths: u32| (7 << 27) | (widths << 21) | (offset << 7);
    pb.extend([inc_header(SC, m::SET_VERTEX_ATTRIBUTE_A, 3), attr(0, 0x02), attr(12, 0x01), 1 << 6]);
    // Vertex shader in pipeline 1, pixel shader in pipeline 5.
    for (pipe, kind, va, regs) in [(1u32, 1u32, VS_VA, 8u32), (5, 5, PS_VA, 4)] {
        let base = pipe * m::PIPELINE_STRIDE;
        pb.extend([inc_header(SC, m::SET_PIPELINE_SHADER + base, 1), (kind << 4) | 1]);
        pb.extend([inc_header(SC, m::SET_PIPELINE_REGISTER_COUNT + base, 1), regs]);
        if class == m::MAXWELL_B {
            pb.extend([inc_header(SC, m::B197_SET_PIPELINE_PROGRAM + base, 1), (va - SHADER_REGION) as u32]);
        } else {
            pb.extend([inc_header(SC, m::SET_PIPELINE_PROGRAM_ADDRESS_A + base, 2), hi(va), lo(va)]);
        }
    }
}

fn draw_triangle(pb: &mut Vec<u32>) {
    pb.extend([immd_header(SC, m::BEGIN, 4)]); // TRIANGLES, INSTANCE_ID FIRST
    pb.extend([inc_header(SC, m::SET_VERTEX_ARRAY_START, 2), 0, 3]); // START = 0, then DRAW count 3
    pb.extend([immd_header(SC, m::END, 0)]);
}

fn release_semaphore(pb: &mut Vec<u32>, payload: u32, one_word: bool) {
    let d = if one_word { 1 << 28 } else { 0 };
    pb.extend([inc_header(SC, m::SET_REPORT_SEMAPHORE_A, 4), hi(SEM_VA), lo(SEM_VA), payload, d]);
}

fn run(pb: &[u32]) -> (Ampere3d, Writes) {
    let mut gpu = Ampere3d::new();
    let mut mem = Writes::default();
    gpu.submit(pb, &mut mem).expect("pushbuffer decodes");
    (gpu, mem)
}

fn pitch_target() -> ColorTarget {
    ColorTarget {
        index: 0,
        gpu_va: RT_VA,
        width_field: 1024,
        height: 256,
        format: ColorFormat::A8B8G8R8,
        layout: SurfaceLayout::Pitch { pitch_bytes: 1024 },
        third_dimension: 1,
        array_pitch: 0,
        layer: 0,
    }
}

#[test]
fn clear_then_one_triangle() {
    clear_then_one_triangle_on(m::AMPERE_B);
}

#[test]
fn clear_then_one_triangle_on_maxwell_b() {
    clear_then_one_triangle_on(m::MAXWELL_B);
}

fn clear_then_one_triangle_on(class: u32) {
    let mut pb = Vec::new();
    bind_class(&mut pb, class);
    bind_render_target(&mut pb, true);
    clear_dark_blue(&mut pb);
    setup_triangle(&mut pb, class);
    draw_triangle(&mut pb);
    release_semaphore(&mut pb, 0x1234, true);

    let (gpu, mem) = run(&pb);
    assert_eq!(gpu.bound_class(SC), Some(class));
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    let events = gpu.events();
    assert_eq!(events.len(), 3);

    let Event::Clear(clear) = &events[0] else { panic!("expected clear, got {:?}", events[0]) };
    assert_eq!(clear.color_f32(), [0.0, 0.0, 0.25, 1.0]);
    assert_eq!(clear.color_mask, [true; 4]);
    assert!(!clear.clear_depth && !clear.clear_stencil);
    assert_eq!(clear.color_target, Some(pitch_target()));
    assert_eq!(clear.color_target.unwrap().width_texels(), Some(256));

    let Event::Draw(draw) = &events[1] else { panic!("expected draw, got {:?}", events[1]) };
    assert_eq!(draw.class, class);
    assert_eq!(draw.topology, Topology::Triangles);
    assert_eq!((draw.first_vertex, draw.vertex_count, draw.instance), (0, 3, 0));
    assert_eq!(draw.color_targets, vec![pitch_target()]);
    assert_eq!(draw.depth_target, None);
    assert_eq!(draw.vertex_streams, vec![VertexStream { index: 0, gpu_va: VB_VA, stride: 28, size: 84, frequency: 0, instanced: false }]);
    assert_eq!(
        draw.vertex_attributes,
        vec![
            VertexAttribute { index: 0, stream: 0, offset: 0, component_bit_widths: 0x02, numerical_type: 7, swap_r_and_b: false },
            VertexAttribute { index: 1, stream: 0, offset: 12, component_bit_widths: 0x01, numerical_type: 7, swap_r_and_b: false },
        ]
    );
    assert_eq!(
        draw.shaders,
        vec![
            ShaderStage { pipeline: 1, kind: ShaderType::Vertex, program_va: VS_VA, register_count: 8 },
            ShaderStage { pipeline: 5, kind: ShaderType::Pixel, program_va: PS_VA, register_count: 4 },
        ]
    );
    assert_eq!(draw.viewport0.scale, [128.0, 128.0, 1.0]);
    assert_eq!(draw.viewport0.offset, [128.0, 128.0, 0.0]);
    assert_eq!(draw.viewport0.clip, [0, 0, 256, 256]);
    assert_eq!(draw.scissor0, Scissor { enabled: true, xmin: 0, xmax: 256, ymin: 0, ymax: 256 });

    let Event::Semaphore(sem) = &events[2] else { panic!("expected semaphore, got {:?}", events[2]) };
    assert_eq!(sem.op, SemaphoreOp::Release);
    assert_eq!((sem.gpu_va, sem.payload, sem.one_word), (SEM_VA, 0x1234, true));
    assert_eq!(mem.0, vec![(SEM_VA, 0x1234u32.to_le_bytes().to_vec())]);
}

#[test]
fn block_linear_target_and_depth_are_described_raw() {
    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    bind_render_target(&mut pb, false);
    // Z24S8 depth target, 256x256, 8-GOB-tall blocks.
    let zt_va = 0x01_2400_0000u64;
    pb.extend([inc_header(SC, m::SET_ZT_A, 5), hi(zt_va), lo(zt_va), 0x14, 3 << 4, 0]);
    pb.extend([inc_header(SC, m::SET_ZT_SIZE_A, 3), 256, 256, 1 << 16]);
    pb.extend([immd_header(SC, m::SET_ZT_SELECT, 1)]);
    pb.extend([inc_header(SC, m::SET_Z_CLEAR_VALUE, 1), 1.0f32.to_bits()]);
    pb.extend([immd_header(SC, m::CLEAR_SURFACE, 0x3f)]); // depth, stencil and RGBA

    let (gpu, _) = run(&pb);
    let Event::Clear(clear) = &gpu.events()[0] else { panic!() };
    let rt = clear.color_target.unwrap();
    assert_eq!(rt.layout, SurfaceLayout::BlockLinear { block_width_log2: 0, block_height_log2: 4, block_depth_log2: 0 });
    assert_eq!((rt.gpu_va, rt.width_texels(), rt.height), (RT_VA, Some(256), 256));
    assert!(clear.clear_depth && clear.clear_stencil);
    assert_eq!(clear.depth, 1.0);
    assert_eq!(
        clear.depth_target,
        Some(DepthTarget {
            gpu_va: zt_va,
            width: 256,
            height: 256,
            format: DepthFormat::Z24S8,
            layout: SurfaceLayout::BlockLinear { block_width_log2: 0, block_height_log2: 3, block_depth_log2: 0 },
            array_pitch: 0,
            layer: 0,
        })
    );
}

#[test]
fn four_word_semaphore_writes_payload_and_timestamp() {
    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    release_semaphore(&mut pb, 7, false);
    let (_, mem) = run(&pb);
    let (va, bytes) = &mem.0[0];
    assert_eq!(*va, SEM_VA);
    assert_eq!(bytes.len(), 16);
    assert_eq!(&bytes[0..8], &[7, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(u64::from_le_bytes(bytes[8..16].try_into().unwrap()), 1);
}

#[test]
fn one_shot_draw_and_instances() {
    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    bind_render_target(&mut pb, true);
    // TRIANGLE_STRIP (5), count 4, start 2; then the same for the next instance.
    let one_shot = (5 << 28) | (4 << 16) | 2;
    pb.extend([inc_header(SC, m::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, 2), one_shot, one_shot]);
    // BEGIN with INSTANCE_ID SUBSEQUENT continues counting (too wide for an immediate header).
    pb.extend([inc_header(SC, m::BEGIN, 1), (1 << 26) | 4, inc_header(SC, m::SET_VERTEX_ARRAY_START, 2), 9, 3]);
    pb.extend([immd_header(SC, m::END, 0)]);

    let (gpu, _) = run(&pb);
    let draws: Vec<(Topology, u32, u32, u32)> = gpu
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::Draw(d) => Some((d.topology, d.first_vertex, d.vertex_count, d.instance)),
            _ => None,
        })
        .collect();
    assert_eq!(
        draws,
        vec![(Topology::TriangleStrip, 2, 4, 0), (Topology::TriangleStrip, 2, 4, 1), (Topology::Triangles, 9, 3, 2)]
    );
}

#[test]
fn several_draws_in_one_begin_use_non_incrementing_counts() {
    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    pb.extend([immd_header(SC, m::BEGIN, 4), inc_header(SC, m::SET_VERTEX_ARRAY_START, 1), 6]);
    pb.extend([non_inc_header(SC, m::DRAW_VERTEX_ARRAY, 2), 3, 3]);
    pb.extend([immd_header(SC, m::END, 0)]);
    let (gpu, _) = run(&pb);
    assert_eq!(gpu.events().len(), 2);
    assert!(gpu.events().iter().all(|e| matches!(e, Event::Draw(d) if d.first_vertex == 6 && d.vertex_count == 3)));
}

#[test]
fn other_classes_and_stray_draws_are_reported_not_decoded() {
    let mut pb = Vec::new();
    bind_class(&mut pb, 0xc7c0); // AMPERE_COMPUTE_B
    pb.extend([immd_header(SC, m::CLEAR_SURFACE, 0x3c)]);
    let (gpu, _) = run(&pb);
    assert!(gpu.events().is_empty());
    assert_eq!(
        gpu.diagnostics(),
        &[Diagnostic::ForeignSubchannel { subchannel: SC, class: Some(0xc7c0), method: m::CLEAR_SURFACE }]
    );

    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    pb.extend([immd_header(SC, m::DRAW_VERTEX_ARRAY, 3), immd_header(SC, m::END, 0)]);
    pb.extend([immd_header(SC, m::BEGIN, 4), immd_header(SC, m::BEGIN, 4)]);
    let (gpu, _) = run(&pb);
    assert!(gpu.events().is_empty());
    assert_eq!(
        gpu.diagnostics(),
        &[
            Diagnostic::DrawOutsideBegin { method: m::DRAW_VERTEX_ARRAY },
            Diagnostic::DrawOutsideBegin { method: m::END },
            Diagnostic::NestedBegin,
        ]
    );
}

#[test]
fn class_methods_land_in_the_shadow_registers() {
    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    pb.extend([inc_header(SC, m::SET_STENCIL_CLEAR_VALUE, 1), 0x5a]);
    let (gpu, _) = run(&pb);
    assert_eq!(gpu.reg(m::SET_STENCIL_CLEAR_VALUE), 0x5a);
    assert_eq!(gpu.reg(m::SET_OBJECT), 0); // Host methods are not class state.
}

/// MAXWELL_B macro 3 at code word 0x20: writes four parameters to `SET_COLOR_CLEAR_VALUE` and the
/// fifth to `CLEAR_SURFACE`, the way deko3d clears through a macro.
fn upload_clear_macro(pb: &mut Vec<u32>) {
    let addi_maddr = |method: u32, inc: u32| (((method / 4) | (inc << 12)) << 14) | (2 << 4) | 1;
    // ALU ADD r1 + r0, assignment 3: emit r1, then r1 = next parameter.
    let emit_r1_fetch = (1 << 11) | (1 << 8) | (3 << 4);
    let code = [
        addi_maddr(m::SET_COLOR_CLEAR_VALUE, 1),
        emit_r1_fetch,
        emit_r1_fetch,
        emit_r1_fetch,
        emit_r1_fetch,
        addi_maddr(m::CLEAR_SURFACE, 0),
        (1 << 11) | (4 << 4) | 0x80, // emit r1, exit
        0x11,                        // delay slot: r0 = r0
    ];
    pb.extend([inc_header(SC, m::LOAD_MME_INSTRUCTION_RAM_POINTER, 1), 0x20]);
    pb.push(non_inc_header(SC, m::LOAD_MME_INSTRUCTION_RAM, code.len() as u32));
    pb.extend(code);
    pb.extend([inc_header(SC, m::LOAD_MME_START_ADDRESS_RAM_POINTER, 2), 3, 0x20]);
}

fn call_clear_macro(pb: &mut Vec<u32>, words: &[u32]) {
    pb.push(one_inc_header(SC, m::CALL_MME_MACRO + 3 * m::CALL_MME_STRIDE, words.len() as u32));
    pb.extend(words);
}

#[test]
fn maxwell_macro_expands_into_a_clear() {
    let mut pb = Vec::new();
    bind_class(&mut pb, m::MAXWELL_B);
    bind_render_target(&mut pb, true);
    upload_clear_macro(&mut pb);
    let color = [0.0f32, 0.0, 0.25, 1.0].map(f32::to_bits);
    call_clear_macro(&mut pb, &[color[0], color[1], color[2], color[3], 0x3c]);
    // The macro runs before the next method, so the clear lands ahead of the release.
    release_semaphore(&mut pb, 7, true);

    let (gpu, _) = run(&pb);
    assert!(gpu.diagnostics().is_empty(), "{:?}", gpu.diagnostics());
    let events = gpu.events();
    assert_eq!(events.len(), 2);
    let Event::Clear(clear) = &events[0] else { panic!("expected clear, got {:?}", events[0]) };
    assert_eq!(clear.color_f32(), [0.0, 0.0, 0.25, 1.0]);
    assert_eq!(clear.color_mask, [true; 4]);
    assert_eq!(clear.color_target, Some(pitch_target()));
    assert!(matches!(events[1], Event::Semaphore(_)));
    assert_eq!(gpu.reg(m::SET_COLOR_CLEAR_VALUE + 8), 0.25f32.to_bits());
}

#[test]
fn macro_failures_are_reported() {
    // Too few parameters: the clear color is written, the LOAD for the clear word is reported.
    let mut pb = Vec::new();
    bind_class(&mut pb, m::MAXWELL_B);
    bind_render_target(&mut pb, true);
    upload_clear_macro(&mut pb);
    call_clear_macro(&mut pb, &[1, 2, 3, 4]);
    let (gpu, _) = run(&pb);
    assert_eq!(gpu.diagnostics(), [Diagnostic::MacroFailed { index: 3, reason: "macro read past its parameters" }]);

    // AMPERE_B macros use another instruction format.
    let mut pb = Vec::new();
    bind_class(&mut pb, m::AMPERE_B);
    call_clear_macro(&mut pb, &[0]);
    let (gpu, _) = run(&pb);
    assert_eq!(gpu.diagnostics(), [Diagnostic::MacroFailed { index: 3, reason: "AMPERE_B macro format not modelled" }]);
    assert!(gpu.events().is_empty());
}
