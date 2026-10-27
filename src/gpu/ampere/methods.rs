//! 3D class method offsets used by the step-1 decoder: one shared table for AMPERE_B (`0xC797`)
//! and MAXWELL_B (`0xB197`), plus the few per-class overrides.
//!
//! Every number here is copied from NVIDIA's public class headers
//! `open-gpu-doc/classes/3d/clc797.h` (`NVC797_*`) and `clb197.h` (`NVB197_*`), MIT licensed.
//! Offsets are byte offsets, as in the headers; the pushbuffer carries them as dword addresses
//! (offset >> 2). A diff of the two headers shows every shared constant below has the same offset
//! and fields in both classes, except where a comment says otherwise. The differences that matter
//! here: shader code addressing (Ampere: per-pipeline 40-bit `SET_PIPELINE_PROGRAM_ADDRESS_A/B`;
//! Maxwell: `SET_PROGRAM_REGION_A/B` plus a per-pipeline offset in `SET_PIPELINE_PROGRAM`) and
//! vertex stream bounds (Ampere: `SET_VERTEX_STREAM_SIZE_A/B`; Maxwell:
//! `SET_VERTEX_STREAM_LIMIT_A_A/B`).

/// AMPERE_B. Linux nouveau `include/nvif/class.h` names `0xc797` AMPERE_B, and
/// `nvkm/engine/gr/ga102.c` lists it as the 3D class of the GA10x family (GA10B included).
pub const AMPERE_B: u32 = 0xC797;
/// MAXWELL_B (GM20x 3D class, `clb197.h`). Accepted so Maxwell-targeting homebrew such as deko3d
/// decodes through the shared table.
pub const MAXWELL_B: u32 = 0xB197;

/// `NVC797_SET_OBJECT`, bits 15:0 = class id.
pub const SET_OBJECT: u32 = 0x0000;
/// Methods below this offset belong to Host (the channel), not to the bound class.
pub const FIRST_CLASS_METHOD: u32 = 0x0100;
/// `NVC797_NO_OPERATION`.
pub const NO_OPERATION: u32 = 0x0100;

// Vertex streams: 32 streams.
pub const VERTEX_STREAM_COUNT: usize = 32;
/// `NVC797_SET_VERTEX_STREAM_SIZE_A(j)` (upper 7:0), `_B(j)` = `+4` (lower 31:0).
pub const SET_VERTEX_STREAM_SIZE_A: u32 = 0x0600;
pub const VERTEX_STREAM_SIZE_STRIDE: u32 = 8;
/// `NVC797_SET_VERTEX_STREAM_A_FORMAT(j)`: STRIDE 11:0, ENABLE 12:12.
pub const SET_VERTEX_STREAM_A_FORMAT: u32 = 0x1c00;
/// `NVC797_SET_VERTEX_STREAM_A_LOCATION_A(j)` (upper 7:0), `_B(j)` (lower 31:0).
pub const SET_VERTEX_STREAM_A_LOCATION_A: u32 = 0x1c04;
pub const SET_VERTEX_STREAM_A_LOCATION_B: u32 = 0x1c08;
/// `NVC797_SET_VERTEX_STREAM_A_FREQUENCY(j)`.
pub const SET_VERTEX_STREAM_A_FREQUENCY: u32 = 0x1c0c;
pub const VERTEX_STREAM_A_STRIDE: u32 = 16;

// Color targets: 8 targets, 64 bytes apart.
pub const COLOR_TARGET_COUNT: usize = 8;
pub const COLOR_TARGET_STRIDE: u32 = 64;
/// `NVC797_SET_COLOR_TARGET_A(j)` (offset upper 7:0) .. `_LAYER(j)`.
pub const SET_COLOR_TARGET_A: u32 = 0x0800;
pub const SET_COLOR_TARGET_B: u32 = 0x0804;
pub const SET_COLOR_TARGET_WIDTH: u32 = 0x0808;
pub const SET_COLOR_TARGET_HEIGHT: u32 = 0x080c;
pub const SET_COLOR_TARGET_FORMAT: u32 = 0x0810;
/// BLOCK_WIDTH 3:0, BLOCK_HEIGHT 7:4, BLOCK_DEPTH 11:8 (log2 GOBs), LAYOUT 12:12 (0 block linear,
/// 1 pitch), THIRD_DIMENSION_CONTROL 16:16.
pub const SET_COLOR_TARGET_MEMORY: u32 = 0x0814;
pub const SET_COLOR_TARGET_THIRD_DIMENSION: u32 = 0x0818;
pub const SET_COLOR_TARGET_ARRAY_PITCH: u32 = 0x081c;
pub const SET_COLOR_TARGET_LAYER: u32 = 0x0820;

// Viewports: 16 viewports.
pub const VIEWPORT_COUNT: usize = 16;
/// `NVC797_SET_VIEWPORT_SCALE_X(j)` .. `_OFFSET_Z(j)`, 32 bytes apart, IEEE floats.
pub const SET_VIEWPORT_SCALE_X: u32 = 0x0a00;
pub const VIEWPORT_STRIDE: u32 = 32;
/// `NVC797_SET_VIEWPORT_CLIP_HORIZONTAL(j)`: X0 15:0, WIDTH 31:16. `_VERTICAL(j)` = `+4`.
pub const SET_VIEWPORT_CLIP_HORIZONTAL: u32 = 0x0c00;
pub const SET_VIEWPORT_CLIP_VERTICAL: u32 = 0x0c04;
pub const VIEWPORT_CLIP_STRIDE: u32 = 16;

/// `NVC797_SET_VERTEX_ARRAY_START`.
pub const SET_VERTEX_ARRAY_START: u32 = 0x0d74;
/// `NVC797_DRAW_VERTEX_ARRAY`: COUNT 31:0, draws `[START, START + COUNT)` inside BEGIN/END.
pub const DRAW_VERTEX_ARRAY: u32 = 0x0d78;
/// `NVC797_SET_COLOR_CLEAR_VALUE(i)`, i = 0..3 (R, G, B, A).
pub const SET_COLOR_CLEAR_VALUE: u32 = 0x0d80;
/// `NVC797_SET_Z_CLEAR_VALUE` (float).
pub const SET_Z_CLEAR_VALUE: u32 = 0x0d90;
/// `NVC797_SET_STENCIL_CLEAR_VALUE` (7:0).
pub const SET_STENCIL_CLEAR_VALUE: u32 = 0x0da0;

/// `NVC797_SET_SCISSOR_ENABLE(j)`; `_HORIZONTAL(j)`: XMIN 15:0, XMAX 31:16; `_VERTICAL(j)`.
pub const SET_SCISSOR_ENABLE: u32 = 0x0e00;
pub const SET_SCISSOR_HORIZONTAL: u32 = 0x0e04;
pub const SET_SCISSOR_VERTICAL: u32 = 0x0e08;
pub const SCISSOR_STRIDE: u32 = 16;

/// `NVC797_SET_ZT_A` (upper 7:0), `_B` (lower), `_FORMAT` (4:0), `_BLOCK_SIZE`, `_ARRAY_PITCH`.
pub const SET_ZT_A: u32 = 0x0fe0;
pub const SET_ZT_B: u32 = 0x0fe4;
pub const SET_ZT_FORMAT: u32 = 0x0fe8;
pub const SET_ZT_BLOCK_SIZE: u32 = 0x0fec;
pub const SET_ZT_ARRAY_PITCH: u32 = 0x0ff0;
/// `NVC797_SET_SURFACE_CLIP_HORIZONTAL`: X 15:0, WIDTH 31:16. `_VERTICAL`: Y 15:0, HEIGHT 31:16.
pub const SET_SURFACE_CLIP_HORIZONTAL: u32 = 0x0ff4;
pub const SET_SURFACE_CLIP_VERTICAL: u32 = 0x0ff8;

/// `NVC797_SET_VERTEX_ATTRIBUTE_A(i)`, 32 attributes: STREAM 4:0, SOURCE 6:6 (1 = inactive),
/// OFFSET 20:7, COMPONENT_BIT_WIDTHS 26:21, NUMERICAL_TYPE 29:27, SWAP_R_AND_B 31:31.
pub const SET_VERTEX_ATTRIBUTE_A: u32 = 0x1160;
pub const VERTEX_ATTRIBUTE_COUNT: usize = 32;

/// `NVC797_DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST`: START_INDEX 15:0, COUNT 27:16,
/// TOPOLOGY 31:28. `_SUBSEQUENT` is the same with the instance id advanced.
pub const DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST: u32 = 0x1214;
pub const DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT: u32 = 0x1218;
/// `NVC797_SET_CT_SELECT`: TARGET_COUNT 3:0, TARGETn at 6:4, 9:7, ... (3 bits each).
pub const SET_CT_SELECT: u32 = 0x121c;
/// `NVC797_SET_ZT_SIZE_A` (WIDTH 27:0), `_B` (HEIGHT 17:0; `NVB197_`: 16:0), `_C`
/// (THIRD_DIMENSION 15:0).
pub const SET_ZT_SIZE_A: u32 = 0x1228;
pub const SET_ZT_SIZE_B: u32 = 0x122c;
pub const SET_ZT_SIZE_C: u32 = 0x1230;
/// `NVC797_SET_ZT_SELECT`: TARGET_COUNT 0:0.
pub const SET_ZT_SELECT: u32 = 0x1538;
/// `NVC797_SET_ZT_LAYER`: OFFSET 15:0.
pub const SET_ZT_LAYER: u32 = 0x179c;

/// `NVC797_END`.
pub const END: u32 = 0x1614;
/// `NVC797_BEGIN`: OP 15:0 (topology), INSTANCE_ID 27:26.
pub const BEGIN: u32 = 0x1618;

/// `NVC797_CLEAR_SURFACE`: Z 0:0, STENCIL 1:1, R..A 2:2..5:5, MRT_SELECT 9:6, RT_ARRAY_INDEX 25:10.
pub const CLEAR_SURFACE: u32 = 0x19d0;

/// `NVC797_SET_REPORT_SEMAPHORE_A` (offset upper 7:0), `_B` (lower), `_C` (payload), `_D`
/// (OPERATION 1:0, AWAKEN_ENABLE 20:20, STRUCTURE_SIZE 28:28). Writing `_D` triggers.
pub const SET_REPORT_SEMAPHORE_A: u32 = 0x1b00;
pub const SET_REPORT_SEMAPHORE_B: u32 = 0x1b04;
pub const SET_REPORT_SEMAPHORE_C: u32 = 0x1b08;
pub const SET_REPORT_SEMAPHORE_D: u32 = 0x1b0c;

/// `NVC797_SET_PIPELINE_SHADER(j)`: ENABLE 0:0, TYPE 7:4. Pipelines are 64 bytes apart.
pub const SET_PIPELINE_SHADER: u32 = 0x2000;
/// `NVC797_SET_PIPELINE_REGISTER_COUNT(j)`: V 8:0 (`NVB197_`: V 7:0).
pub const SET_PIPELINE_REGISTER_COUNT: u32 = 0x200c;
/// `NVC797_SET_PIPELINE_PROGRAM_ADDRESS_A(j)` (upper 7:0), `_B(j)` (lower 31:0).
pub const SET_PIPELINE_PROGRAM_ADDRESS_A: u32 = 0x2014;
pub const SET_PIPELINE_PROGRAM_ADDRESS_B: u32 = 0x2018;
pub const PIPELINE_STRIDE: u32 = 64;
/// VERTEX_CULL_BEFORE_FETCH, VERTEX, TESSELLATION_INIT, TESSELLATION, GEOMETRY, PIXEL.
pub const PIPELINE_COUNT: usize = 6;

// MAXWELL_B-only methods (`clb197.h`); on AMPERE_B these offsets are unused or mean something else.
/// `NVB197_SET_PROGRAM_REGION_A` (address upper 7:0), `_B` (lower 31:0): base of all shader code.
pub const B197_SET_PROGRAM_REGION_A: u32 = 0x1608;
pub const B197_SET_PROGRAM_REGION_B: u32 = 0x160c;
/// `NVB197_SET_PIPELINE_PROGRAM(j)`: OFFSET 31:0 from the program region.
pub const B197_SET_PIPELINE_PROGRAM: u32 = 0x2004;
/// `NVB197_SET_VERTEX_STREAM_LIMIT_A_A(j)` (upper 7:0), `_B(j)` (lower 31:0), 8 bytes apart.
pub const B197_SET_VERTEX_STREAM_LIMIT_A_A: u32 = 0x1f00;
pub const B197_SET_VERTEX_STREAM_LIMIT_A_B: u32 = 0x1f04;
pub const B197_VERTEX_STREAM_LIMIT_STRIDE: u32 = 8;

/// One past the highest method the 12-bit pushbuffer dword address can reach.
pub const METHOD_SPACE: u32 = 0x4000;
