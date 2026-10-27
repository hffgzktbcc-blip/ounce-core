#![allow(dead_code)]

use crate::gpu::upscaler::UpscaleEngine;
use crate::mmu::Memory;

/// Tegra NVGPU Channel / Pushbuffer & 3D Rasterizer Engine.
///
/// Models the Nvidia Tegra (Maxwell GM20B / Ampere T239) command submission interface
/// used by Horizon OS to feed draw calls, surface clears, and vertex arrays to the GPU.

pub const NVGPU_IOCTL_CHANNEL_SUBMIT_GPFIFO: u32 = 0xC0104808;
pub const NVHOST_IOCTL_CTRL_SYNCPT_INCR: u32 = 0x40040003;
pub const NVHOST_IOCTL_CTRL_SYNCPT_WAIT: u32 = 0xC00C0006;

/// Method Register Offsets on the Tegra 3D Engine
pub const METHOD_SET_RENDER_TARGET_ADDR: u16 = 0x0200;
pub const METHOD_SET_CLEAR_COLOR: u16 = 0x0360;
pub const METHOD_SET_MVP_MATRIX: u16 = 0x0400;
pub const METHOD_BIND_SHADER_PROGRAM: u16 = 0x0480;
pub const METHOD_SET_DEPTH_TEST_ENABLE: u16 = 0x04B0;
pub const METHOD_BIND_TEXTURE_2D: u16 = 0x0500;
pub const METHOD_DRAW_ARRAYS: u16 = 0x0585;
pub const METHOD_DRAW_TEXTURED_ARRAYS: u16 = 0x0586;
pub const METHOD_SET_INDEX_ARRAY_ADDR: u16 = 0x05F2;
pub const METHOD_DRAW_INDEXED: u16 = 0x05F4;
pub const METHOD_CLEAR_SURFACE: u16 = 0x0674;
pub const METHOD_SET_VERTEX_ARRAY_ADDR: u16 = 0x0700;
pub const METHOD_SET_VIEWPORT_DIMS: u16 = 0x0860;
pub const METHOD_DISPATCH_UPSCALE_2X: u16 = 0x0900;

/// Maximum unique vertices extracted in a single hardware-accelerated draw call.
/// Protects host memory against out-of-memory denial-of-service from malformed guest index buffers.
pub const MAX_UNIQUE_VERTICES_PER_DRAW: usize = 65_536;

/// Maximum number of index buffer words processed in a single draw call.
pub const MAX_INDEX_COUNT_PER_DRAW: usize = 196_608; // 65,536 * 3

/// Subchannels available on the Tegra GPU
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuSubchannel {
    Engine2D = 1,
    Engine3D = 2,
    Compute = 3,
    CopyEngine = 4,
}

/// Pushbuffer method submission mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionMode {
    NonIncreasing = 0,
    Increasing = 1,
    Once = 2,
}

/// Decoded GPU Command from the GPFIFO pushbuffer
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuCommand {
    pub subchannel: u8,
    pub method: u16,
    pub mode: SubmissionMode,
    pub args: Vec<u32>,
}

/// 3D Vertex with Normalized Device Coordinates (NDC), RGB color, and UV texture coordinates
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub u: f32,
    pub v: f32,
}

/// Standardized 32-byte vertex representation for host GPU 3D rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gpu3DVertex {
    pub position: [f32; 3], // offset 0:  x, y, z
    pub color: [f32; 3],    // offset 12: r, g, b
    pub uv: [f32; 2],       // offset 24: u, v (padded to 0.0, 0.0 for untextured draws)
}

impl Gpu3DVertex {
    pub const fn new(position: [f32; 3], color: [f32; 3], uv: [f32; 2]) -> Self {
        Self { position, color, uv }
    }

    /// Endian-safe, pure safe Rust serialization (zero unsafe).
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..4].copy_from_slice(&self.position[0].to_le_bytes());
        b[4..8].copy_from_slice(&self.position[1].to_le_bytes());
        b[8..12].copy_from_slice(&self.position[2].to_le_bytes());
        b[12..16].copy_from_slice(&self.color[0].to_le_bytes());
        b[16..20].copy_from_slice(&self.color[1].to_le_bytes());
        b[20..24].copy_from_slice(&self.color[2].to_le_bytes());
        b[24..28].copy_from_slice(&self.uv[0].to_le_bytes());
        b[28..32].copy_from_slice(&self.uv[1].to_le_bytes());
        b
    }
}

/// Primitive topology for host GPU draw calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuPrimitiveTopology {
    TriangleList,
    TriangleStrip,
}

/// Host representation of a bound guest 2D RGBA8 texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Gpu3DTextureData {
    pub width: u32,
    pub height: u32,
    pub rgba_bytes: Vec<u8>,
}

/// Structured 3D render operation captured from Tegra Maxwell 3D pushbuffers.
#[derive(Debug, Clone, PartialEq)]
pub enum Gpu3DDrawOp {
    ClearSurface {
        color: [f32; 4],
        depth: f32,
    },
    SetViewport {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    SetMvpMatrix {
        matrix: [f32; 16],
    },
    SetDepthTest {
        enabled: bool,
    },
    BindTexture {
        texture: Option<Gpu3DTextureData>,
    },
    DrawArrays {
        vertices: Vec<Gpu3DVertex>,
        topology: GpuPrimitiveTopology,
    },
    DrawIndexed {
        vertices: Vec<Gpu3DVertex>,
        indices: Vec<u32>,
        topology: GpuPrimitiveTopology,
    },
}

/// Transposes a 4x4 row-major guest matrix into column-major WGSL uniform layout.
#[inline]
pub fn transpose_row_to_col_major(m: &[f32; 16]) -> [f32; 16] {
    [
        m[0], m[4], m[8],  m[12],
        m[1], m[5], m[9],  m[13],
        m[2], m[6], m[10], m[14],
        m[3], m[7], m[11], m[15],
    ]
}

/// NVGPU 3D State Engine
#[derive(Debug)]
pub struct Gpu3DState {
    pub render_target_addr: u64,
    pub vertex_array_addr: u64,
    pub index_array_addr: u64,
    pub is_index_32: bool,
    pub texture_addr: u64,
    pub texture_width: u32,
    pub texture_height: u32,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub clear_color: [f32; 4],
    pub mvp_matrix: [f32; 16],
    pub depth_test_enabled: bool,
    pub depth_buffer: Vec<f32>,
    pub last_decompiled_glsl: Option<String>,
    pub last_decompiled_spirv_words: usize,
    pub total_draw_calls: u64,
    pub total_triangles_rendered: u64,
    pub total_texels_sampled: u64,
    pub total_depth_tests_passed: u64,
    pub total_depth_tests_rejected: u64,
    pub total_commands_processed: u64,
}

impl Default for Gpu3DState {
    fn default() -> Self {
        Self {
            render_target_addr: 0x90000000,
            vertex_array_addr: 0,
            index_array_addr: 0,
            is_index_32: false,
            texture_addr: 0,
            texture_width: 0,
            texture_height: 0,
            viewport_width: 1280,
            viewport_height: 720,
            clear_color: [0.05, 0.05, 0.10, 1.0],
            // 4x4 Identity Matrix (Column-Major / Row-Major neutral for identity)
            mvp_matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
            depth_test_enabled: false,
            depth_buffer: Vec::new(),
            last_decompiled_glsl: None,
            last_decompiled_spirv_words: 0,
            total_draw_calls: 0,
            total_triangles_rendered: 0,
            total_texels_sampled: 0,
            total_depth_tests_passed: 0,
            total_depth_tests_rejected: 0,
            total_commands_processed: 0,
        }
    }
}

pub struct NvgpuChannel {
    pub state: Gpu3DState,
    pub upscaler: UpscaleEngine,
    pub hardware_acceleration_enabled: bool,
    pub draw_queue: Vec<Gpu3DDrawOp>,
}

impl Default for NvgpuChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl NvgpuChannel {
    pub fn new() -> Self {
        Self {
            state: Gpu3DState::default(),
            upscaler: UpscaleEngine::new(),
            hardware_acceleration_enabled: false,
            draw_queue: Vec::new(),
        }
    }

    /// Toggles host GPU hardware acceleration offloading for 3D draw commands.
    #[inline]
    pub fn set_hardware_acceleration(&mut self, enabled: bool) {
        self.hardware_acceleration_enabled = enabled;
    }

    /// Drains all captured 3D draw operations for presentation by the host GPU.
    #[inline]
    pub fn drain_draw_ops(&mut self) -> Vec<Gpu3DDrawOp> {
        std::mem::take(&mut self.draw_queue)
    }

    /// Fetches raw, unclipped vertex coordinates directly from guest memory.
    pub fn fetch_raw_vertex(&self, mmu: &Memory, vertex_idx: usize, stride_bytes: u64, textured: bool) -> Gpu3DVertex {
        let v_addr = self.state.vertex_array_addr + (vertex_idx as u64) * stride_bytes;
        let px = f32::from_bits(mmu.read_u32(v_addr).unwrap_or(0));
        let py = f32::from_bits(mmu.read_u32(v_addr + 4).unwrap_or(0));
        let pz = f32::from_bits(mmu.read_u32(v_addr + 8).unwrap_or(0));
        let r = f32::from_bits(mmu.read_u32(v_addr + 12).unwrap_or(0));
        let g = f32::from_bits(mmu.read_u32(v_addr + 16).unwrap_or(0));
        let b = f32::from_bits(mmu.read_u32(v_addr + 20).unwrap_or(0));
        let (u, v) = if textured {
            (
                f32::from_bits(mmu.read_u32(v_addr + 24).unwrap_or(0)),
                f32::from_bits(mmu.read_u32(v_addr + 28).unwrap_or(0)),
            )
        } else {
            (0.0, 0.0)
        };
        Gpu3DVertex::new([px, py, pz], [r, g, b], [u, v])
    }

    /// Helper to encode a Tegra GPFIFO method header word.
    #[inline]
    pub const fn encode_header(mode: SubmissionMode, subchannel: u8, method: u16, count: u16) -> u32 {
        ((mode as u32 & 0x7) << 29)
            | ((count as u32 & 0x1FFF) << 16)
            | ((subchannel as u32 & 0x7) << 13)
            | (method as u32 & 0x1FFF)
    }

    /// Parses and executes a slice of pre-extracted GPFIFO pushbuffer words directly.
    pub fn process_pushbuffer_slice(&mut self, mmu: &mut Memory, words: &[u32]) {
        let mut offset = 0;
        let word_count = words.len();

        while offset < word_count {
            let header = words[offset];
            offset += 1;

            let mode_val = (header >> 29) & 0x7;
            let count = ((header >> 16) & 0x1FFF) as usize;
            let subchannel = ((header >> 13) & 0x7) as u8;
            let method = (header & 0x1FFF) as u16;

            let mode = match mode_val {
                0 => SubmissionMode::NonIncreasing,
                1 => SubmissionMode::Increasing,
                _ => SubmissionMode::Once,
            };

            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                if offset >= word_count {
                    break;
                }
                args.push(words[offset]);
                offset += 1;
            }

            self.execute_command(
                mmu,
                GpuCommand {
                    subchannel,
                    method,
                    mode,
                    args,
                },
            );
        }
    }

    /// Parses and executes a stream of GPFIFO pushbuffer words from guest memory.
    pub fn process_pushbuffer(&mut self, mmu: &mut Memory, pb_addr: u64, word_count: usize) {
        let mut offset = 0;

        while offset < word_count {
            let addr = pb_addr + (offset * 4) as u64;
            let header = match mmu.read_u32(addr) {
                Ok(h) => h,
                Err(_) => break,
            };
            offset += 1;

            // Tegra Pushbuffer Header format:
            // bits [31:29] = Submission Mode (0 = NonIncreasing, 1 = Increasing, 2 = Once)
            // bits [28:16] = Count (number of argument words)
            // bits [15:13] = Subchannel (0..7)
            // bits [12:0]  = Method register offset
            let mode_val = (header >> 29) & 0x7;
            let count = ((header >> 16) & 0x1FFF) as usize;
            let subchannel = ((header >> 13) & 0x7) as u8;
            let method = (header & 0x1FFF) as u16;

            let mode = match mode_val {
                0 => SubmissionMode::NonIncreasing,
                1 => SubmissionMode::Increasing,
                _ => SubmissionMode::Once,
            };

            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                if offset >= word_count {
                    break;
                }
                let arg_addr = pb_addr + (offset * 4) as u64;
                if let Ok(val) = mmu.read_u32(arg_addr) {
                    args.push(val);
                }
                offset += 1;
            }

            self.execute_command(
                mmu,
                GpuCommand {
                    subchannel,
                    method,
                    mode,
                    args,
                },
            );
        }
    }

    /// Dispatches high-level 3D engine commands and executes rasterization into VRAM.
    fn execute_command(&mut self, mmu: &mut Memory, cmd: GpuCommand) {
        self.state.total_commands_processed += 1;

        match cmd.method {
            // 0x0200: Set Render Target Address
            METHOD_SET_RENDER_TARGET_ADDR => {
                if cmd.args.len() >= 2 {
                    self.state.render_target_addr =
                        ((cmd.args[1] as u64) << 32) | (cmd.args[0] as u64);
                } else if !cmd.args.is_empty() {
                    self.state.render_target_addr = cmd.args[0] as u64;
                }
            }
            // 0x0860: Set Viewport Dimensions
            METHOD_SET_VIEWPORT_DIMS => {
                if cmd.args.len() >= 2 {
                    self.state.viewport_width = cmd.args[0];
                    self.state.viewport_height = cmd.args[1];
                }
                if self.hardware_acceleration_enabled {
                    self.draw_queue.push(Gpu3DDrawOp::SetViewport {
                        x: 0.0,
                        y: 0.0,
                        width: self.state.viewport_width as f32,
                        height: self.state.viewport_height as f32,
                    });
                }
            }
            // 0x0360: Set Clear Color (RGBA floats)
            METHOD_SET_CLEAR_COLOR => {
                if cmd.args.len() >= 4 {
                    self.state.clear_color = [
                        f32::from_bits(cmd.args[0]),
                        f32::from_bits(cmd.args[1]),
                        f32::from_bits(cmd.args[2]),
                        f32::from_bits(cmd.args[3]),
                    ];
                }
            }
            // 0x0400: Set 4x4 Model-View-Projection (MVP) Matrix Uniform (16 floats)
            METHOD_SET_MVP_MATRIX => {
                for (i, &word) in cmd.args.iter().take(16).enumerate() {
                    self.state.mvp_matrix[i] = f32::from_bits(word);
                }
                if self.hardware_acceleration_enabled {
                    self.draw_queue.push(Gpu3DDrawOp::SetMvpMatrix {
                        matrix: self.state.mvp_matrix,
                    });
                }
            }
            // 0x0480: Bind and Decompile Ampere SASS Shader Program from Guest Memory
            METHOD_BIND_SHADER_PROGRAM => {
                if !cmd.args.is_empty() {
                    let shader_addr = if cmd.args.len() >= 2 {
                        ((cmd.args[1] as u64) << 32) | (cmd.args[0] as u64)
                    } else {
                        cmd.args[0] as u64
                    };
                    let mut sass_words = Vec::new();
                    for i in 0..32 {
                        if let Ok(w) = mmu.read_u64(shader_addr + (i * 8) as u64) {
                            sass_words.push(w);
                            if (w >> 56) == 0xFF {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    let decompiled = crate::gpu::shader::SassShaderDecompiler::decompile(&sass_words);
                    self.state.last_decompiled_spirv_words = decompiled.spirv_words.len();
                    self.state.last_decompiled_glsl = Some(decompiled.glsl_source);
                }
            }
            // 0x04B0: Enable/Disable 32-bit Floating-Point Depth Testing (Z-Buffer)
            METHOD_SET_DEPTH_TEST_ENABLE => {
                let enable = cmd.args.first().copied().unwrap_or(0) != 0;
                self.state.depth_test_enabled = enable;
                if enable {
                    let needed = (self.state.viewport_width * self.state.viewport_height) as usize;
                    if self.state.depth_buffer.len() != needed {
                        self.state.depth_buffer.resize(needed, 1.0);
                    }
                }
                if self.hardware_acceleration_enabled {
                    self.draw_queue.push(Gpu3DDrawOp::SetDepthTest {
                        enabled: enable,
                    });
                }
            }
            // 0x0674: Clear Render Target Surface in VRAM (and Z-Buffer to 1.0)
            METHOD_CLEAR_SURFACE => {
                if self.hardware_acceleration_enabled {
                    self.draw_queue.push(Gpu3DDrawOp::ClearSurface {
                        color: self.state.clear_color,
                        depth: 1.0,
                    });
                } else {
                    self.rasterize_clear(mmu);
                }
            }
            // 0x0700: Set Vertex Array Address in Guest Memory
            METHOD_SET_VERTEX_ARRAY_ADDR => {
                if cmd.args.len() >= 2 {
                    self.state.vertex_array_addr =
                        ((cmd.args[1] as u64) << 32) | (cmd.args[0] as u64);
                } else if !cmd.args.is_empty() {
                    self.state.vertex_array_addr = cmd.args[0] as u64;
                }
            }
            // 0x0500: Bind 2D RGBA8888 Texture in Guest Memory (addr, width, height)
            METHOD_BIND_TEXTURE_2D => {
                if cmd.args.len() >= 3 {
                    self.state.texture_addr = cmd.args[0] as u64;
                    self.state.texture_width = cmd.args[1];
                    self.state.texture_height = cmd.args[2];
                }
                if self.hardware_acceleration_enabled {
                    let tex_data = if self.state.texture_addr != 0
                        && self.state.texture_width > 0
                        && self.state.texture_height > 0
                    {
                        let size = (self.state.texture_width * self.state.texture_height * 4) as usize;
                        let mut buf = vec![0u8; size];
                        if mmu.read_bytes(self.state.texture_addr, &mut buf).is_ok() {
                            Some(Gpu3DTextureData {
                                width: self.state.texture_width,
                                height: self.state.texture_height,
                                rgba_bytes: buf,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    self.draw_queue.push(Gpu3DDrawOp::BindTexture { texture: tex_data });
                }
            }
            // 0x0585: Draw Arrays (Fetches 6-float vertices [x,y,z,r,g,b] and rasterizes 3D triangles)
            METHOD_DRAW_ARRAYS => {
                self.state.total_draw_calls += 1;
                let raw_vertex_count = cmd.args.first().copied().unwrap_or(0) as usize;
                let vertex_count = raw_vertex_count.min(MAX_UNIQUE_VERTICES_PER_DRAW);
                if self.hardware_acceleration_enabled {
                    if vertex_count >= 3 && self.state.vertex_array_addr != 0 {
                        let mut vertices = Vec::with_capacity(vertex_count);
                        for i in 0..vertex_count {
                            vertices.push(self.fetch_raw_vertex(mmu, i, 24, false));
                        }
                        self.draw_queue.push(Gpu3DDrawOp::DrawArrays {
                            vertices,
                            topology: GpuPrimitiveTopology::TriangleList,
                        });
                        self.state.total_triangles_rendered += (vertex_count / 3) as u64;
                    }
                } else if raw_vertex_count >= 3 {
                    self.rasterize_triangles(mmu, raw_vertex_count, false);
                }
            }
            // 0x0586: Draw Textured Arrays (Fetches 8-float vertices [x,y,z,r,g,b,u,v] with 2D texture sampling)
            METHOD_DRAW_TEXTURED_ARRAYS => {
                self.state.total_draw_calls += 1;
                let raw_vertex_count = cmd.args.first().copied().unwrap_or(0) as usize;
                let vertex_count = raw_vertex_count.min(MAX_UNIQUE_VERTICES_PER_DRAW);
                if self.hardware_acceleration_enabled {
                    if vertex_count >= 3 && self.state.vertex_array_addr != 0 {
                        let mut vertices = Vec::with_capacity(vertex_count);
                        for i in 0..vertex_count {
                            vertices.push(self.fetch_raw_vertex(mmu, i, 32, true));
                        }
                        self.draw_queue.push(Gpu3DDrawOp::DrawArrays {
                            vertices,
                            topology: GpuPrimitiveTopology::TriangleList,
                        });
                        self.state.total_triangles_rendered += (vertex_count / 3) as u64;
                    }
                } else if raw_vertex_count >= 3 {
                    self.rasterize_triangles(mmu, raw_vertex_count, true);
                }
            }
            // 0x05F2: Set Index Array Address & Format (Args: [addr, is_32bit])
            METHOD_SET_INDEX_ARRAY_ADDR => {
                if let Some(&addr) = cmd.args.first() {
                    self.state.index_array_addr = addr as u64;
                    self.state.is_index_32 = cmd.args.get(1).map_or(false, |&val| val == 1);
                }
            }
            // 0x05F4: Draw Indexed Elements (Args: [index_count, textured])
            METHOD_DRAW_INDEXED => {
                self.state.total_draw_calls += 1;
                let raw_index_count = cmd.args.first().copied().unwrap_or(0) as usize;
                let textured = cmd.args.get(1).map_or(false, |&val| val == 1);

                if self.hardware_acceleration_enabled {
                    let index_count = raw_index_count.min(MAX_INDEX_COUNT_PER_DRAW);
                    if index_count >= 3 && self.state.vertex_array_addr != 0 && self.state.index_array_addr != 0 {
                        let stride_bytes = if textured { 32u64 } else { 24u64 };
                        let is_32 = self.state.is_index_32;
                        let restart_index: u32 = if is_32 { 0xFFFF_FFFF } else { 0x0000_FFFF };

                        let num_tris = index_count / 3;
                        let mut unique_vertices = Vec::new();
                        let mut remapped_indices = Vec::with_capacity(num_tris * 3);
                        let mut index_remap = std::collections::HashMap::<u32, u32>::new();
                        let mut valid_triangles = 0u64;

                        for t in 0..num_tris {
                            let mut tri_raw = [0u32; 3];
                            let mut skip_triangle = false;

                            for (i, slot) in tri_raw.iter_mut().enumerate() {
                                let offset = (t * 3 + i) as u64;
                                let elem_size = if is_32 { 4u64 } else { 2u64 };
                                let byte_offset = match offset.checked_mul(elem_size) {
                                    Some(off) => off,
                                    None => {
                                        skip_triangle = true;
                                        break;
                                    }
                                };
                                let addr = match self.state.index_array_addr.checked_add(byte_offset) {
                                    Some(a) => a,
                                    None => {
                                        skip_triangle = true;
                                        break;
                                    }
                                };

                                let idx = if is_32 {
                                    match mmu.read_u32(addr) {
                                        Ok(val) => val,
                                        Err(_) => {
                                            skip_triangle = true;
                                            break;
                                        }
                                    }
                                } else {
                                    match mmu.read_u16(addr) {
                                        Ok(val) => val as u32,
                                        Err(_) => {
                                            skip_triangle = true;
                                            break;
                                        }
                                    }
                                };

                                if idx == restart_index {
                                    skip_triangle = true;
                                    break;
                                }
                                *slot = idx;
                            }

                            if skip_triangle {
                                continue;
                            }

                            // Validate vertex memory bounds for all 3 vertices
                            let mut tri_valid = true;
                            for &idx in &tri_raw {
                                let v_offset = match (idx as u64).checked_mul(stride_bytes) {
                                    Some(off) => off,
                                    None => {
                                        tri_valid = false;
                                        break;
                                    }
                                };
                                let v_start = match self.state.vertex_array_addr.checked_add(v_offset) {
                                    Some(a) => a,
                                    None => {
                                        tri_valid = false;
                                        break;
                                    }
                                };
                                let v_end = match v_start.checked_add(stride_bytes - 1) {
                                    Some(a) => a,
                                    None => {
                                        tri_valid = false;
                                        break;
                                    }
                                };
                                if !mmu.is_mapped(v_start) || !mmu.is_mapped(v_end) {
                                    tri_valid = false;
                                    break;
                                }
                            }

                            if !tri_valid {
                                continue;
                            }

                            // Check unique vertex capacity before inserting
                            let new_verts_needed = tri_raw
                                .iter()
                                .filter(|idx| !index_remap.contains_key(idx))
                                .count();

                            if unique_vertices.len() + new_verts_needed > MAX_UNIQUE_VERTICES_PER_DRAW {
                                // Hit safe limit: truncate triangle stream for this draw call
                                break;
                            }

                            // Remap unique indices into compact contiguous list
                            for &idx in &tri_raw {
                                let remapped_idx = match index_remap.entry(idx) {
                                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                                    std::collections::hash_map::Entry::Vacant(entry) => {
                                        let new_id = unique_vertices.len() as u32;
                                        unique_vertices.push(self.fetch_raw_vertex(
                                            mmu,
                                            idx as usize,
                                            stride_bytes,
                                            textured,
                                        ));
                                        entry.insert(new_id);
                                        new_id
                                    }
                                };
                                remapped_indices.push(remapped_idx);
                            }

                            valid_triangles += 1;
                        }

                        if !remapped_indices.is_empty() {
                            self.draw_queue.push(Gpu3DDrawOp::DrawIndexed {
                                vertices: unique_vertices,
                                indices: remapped_indices,
                                topology: GpuPrimitiveTopology::TriangleList,
                            });
                            self.state.total_triangles_rendered += valid_triangles;
                        }
                    }
                } else if raw_index_count >= 3 {
                    self.rasterize_indexed_triangles(mmu, raw_index_count, textured);
                }
            }
            // 0x0900: Dispatch 2x upscaler model (bilinear + sharpen, not DLSS)
            // Args: [src_addr, src_w, src_h, dst_addr]
            METHOD_DISPATCH_UPSCALE_2X => {
                if cmd.args.len() >= 4 {
                    let src_addr = cmd.args[0] as u64;
                    let src_w = cmd.args[1];
                    let src_h = cmd.args[2];
                    let dst_addr = cmd.args[3] as u64;
                    self.upscaler.upscale_2x(mmu, src_addr, src_w, src_h, dst_addr);
                }
            }
            _ => {}
        }
    }

    /// Fills the entire active render target in VRAM with the configured clear color and resets Z-Buffer.
    fn rasterize_clear(&mut self, mmu: &mut Memory) {
        let w = self.state.viewport_width as usize;
        let h = self.state.viewport_height as usize;
        if w == 0 || h == 0 || self.state.render_target_addr == 0 {
            return;
        }

        if self.state.depth_test_enabled {
            let needed = w * h;
            if self.state.depth_buffer.len() != needed {
                self.state.depth_buffer.resize(needed, 1.0);
            } else {
                self.state.depth_buffer.fill(1.0);
            }
        }

        let r = (self.state.clear_color[0].clamp(0.0, 1.0) * 255.0) as u8;
        let g = (self.state.clear_color[1].clamp(0.0, 1.0) * 255.0) as u8;
        let b = (self.state.clear_color[2].clamp(0.0, 1.0) * 255.0) as u8;
        let a = (self.state.clear_color[3].clamp(0.0, 1.0) * 255.0) as u8;

        let mut row_bytes = vec![0u8; w * 4];
        for px in 0..w {
            row_bytes[px * 4] = r;
            row_bytes[px * 4 + 1] = g;
            row_bytes[px * 4 + 2] = b;
            row_bytes[px * 4 + 3] = a;
        }

        let stride = (w * 4) as u64;
        for y in 0..h {
            let row_addr = self.state.render_target_addr + (y as u64) * stride;
            let _ = mmu.write_bytes(row_addr, &row_bytes);
        }
    }

    /// Helper to fetch and transform a single vertex from guest memory.
    fn fetch_vertex(&self, mmu: &Memory, vertex_idx: usize, stride_bytes: u64, textured: bool) -> Vertex {
        let v_addr = self.state.vertex_array_addr + (vertex_idx as u64) * stride_bytes;
        let px = f32::from_bits(mmu.read_u32(v_addr).unwrap_or(0));
        let py = f32::from_bits(mmu.read_u32(v_addr + 4).unwrap_or(0));
        let pz = f32::from_bits(mmu.read_u32(v_addr + 8).unwrap_or(0));
        let r = f32::from_bits(mmu.read_u32(v_addr + 12).unwrap_or(0));
        let g = f32::from_bits(mmu.read_u32(v_addr + 16).unwrap_or(0));
        let b = f32::from_bits(mmu.read_u32(v_addr + 20).unwrap_or(0));
        let (u, v) = if textured {
            (
                f32::from_bits(mmu.read_u32(v_addr + 24).unwrap_or(0)),
                f32::from_bits(mmu.read_u32(v_addr + 28).unwrap_or(0)),
            )
        } else {
            (0.0, 0.0)
        };

        // Vertex Shader Stage: gl_Position = u_mvp * vec4(px, py, pz, 1.0)
        let m = &self.state.mvp_matrix;
        let cx = m[0] * px + m[1] * py + m[2] * pz + m[3];
        let cy = m[4] * px + m[5] * py + m[6] * pz + m[7];
        let cz = m[8] * px + m[9] * py + m[10] * pz + m[11];
        let cw = (m[12] * px + m[13] * py + m[14] * pz + m[15]).max(1e-4);

        Vertex {
            x: cx / cw,
            y: cy / cw,
            z: cz / cw,
            r,
            g,
            b,
            u,
            v,
        }
    }

    /// Fetches vertices from guest memory and rasterizes shaded (and optionally 2D-textured) triangles into VRAM.
    fn rasterize_triangles(&mut self, mmu: &mut Memory, vertex_count: usize, textured: bool) {
        let w = self.state.viewport_width as f32;
        let h = self.state.viewport_height as f32;
        if w <= 0.0 || h <= 0.0 || self.state.vertex_array_addr == 0 {
            return;
        }

        let stride_bytes = if textured { 32 } else { 24 };
        let num_tris = vertex_count / 3;
        for t in 0..num_tris {
            let v0 = self.fetch_vertex(mmu, t * 3 + 0, stride_bytes, textured);
            let v1 = self.fetch_vertex(mmu, t * 3 + 1, stride_bytes, textured);
            let v2 = self.fetch_vertex(mmu, t * 3 + 2, stride_bytes, textured);

            self.draw_single_triangle(mmu, &v0, &v1, &v2, textured);
            self.state.total_triangles_rendered += 1;
        }
    }

    /// Fetches index buffer elements and rasterizes indexed geometry from guest vertex buffers.
    fn rasterize_indexed_triangles(&mut self, mmu: &mut Memory, index_count: usize, textured: bool) {
        let w = self.state.viewport_width as f32;
        let h = self.state.viewport_height as f32;
        if w <= 0.0 || h <= 0.0 || self.state.vertex_array_addr == 0 || self.state.index_array_addr == 0 {
            return;
        }

        let stride_bytes = if textured { 32 } else { 24 };
        let is_32 = self.state.is_index_32;
        let num_tris = index_count / 3;

        for t in 0..num_tris {
            let mut indices = [0usize; 3];
            for (i, idx_slot) in indices.iter_mut().enumerate() {
                let offset = (t * 3 + i) as u64;
                let raw_idx = if is_32 {
                    let addr = self.state.index_array_addr + offset * 4;
                    mmu.read_u32(addr).unwrap_or(0) as usize
                } else {
                    let addr = self.state.index_array_addr + offset * 2;
                    mmu.read_u16(addr).unwrap_or(0) as usize
                };
                *idx_slot = raw_idx;
            }

            let v0 = self.fetch_vertex(mmu, indices[0], stride_bytes, textured);
            let v1 = self.fetch_vertex(mmu, indices[1], stride_bytes, textured);
            let v2 = self.fetch_vertex(mmu, indices[2], stride_bytes, textured);

            self.draw_single_triangle(mmu, &v0, &v1, &v2, textured);
            self.state.total_triangles_rendered += 1;
        }
    }

    /// Barycentric 2D/3D triangle rasterizer with smooth RGB vertex color and 2D texture UV sampling.
    fn draw_single_triangle(
        &mut self,
        mmu: &mut Memory,
        v0: &Vertex,
        v1: &Vertex,
        v2: &Vertex,
        textured: bool,
    ) {
        let w = self.state.viewport_width as i32;
        let h = self.state.viewport_height as i32;
        let wf = w as f32;
        let hf = h as f32;

        // Viewport transformation: NDC [-1.0, 1.0] -> Screen space [0, W] x [0, H]
        let sx0 = (v0.x + 1.0) * 0.5 * wf;
        let sy0 = (1.0 - v0.y) * 0.5 * hf;
        let sx1 = (v1.x + 1.0) * 0.5 * wf;
        let sy1 = (1.0 - v1.y) * 0.5 * hf;
        let sx2 = (v2.x + 1.0) * 0.5 * wf;
        let sy2 = (1.0 - v2.y) * 0.5 * hf;

        // Bounding box clipped to viewport
        let min_x = (sx0.min(sx1).min(sx2).floor() as i32).max(0);
        let max_x = (sx0.max(sx1).max(sx2).ceil() as i32).min(w - 1);
        let min_y = (sy0.min(sy1).min(sy2).floor() as i32).max(0);
        let max_y = (sy0.max(sy1).max(sy2).ceil() as i32).min(h - 1);

        let area = edge_fn(sx0, sy0, sx1, sy1, sx2, sy2);
        if area.abs() < 1e-5 {
            return;
        }
        let inv_area = 1.0 / area;

        let has_tex = textured
            && self.state.texture_addr != 0
            && self.state.texture_width > 0
            && self.state.texture_height > 0;

        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;

                let w0 = edge_fn(sx1, sy1, sx2, sy2, fx, fy) * inv_area;
                let w1 = edge_fn(sx2, sy2, sx0, sy0, fx, fy) * inv_area;
                let w2 = edge_fn(sx0, sy0, sx1, sy1, fx, fy) * inv_area;

                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    let pixel_idx = (py * w + px) as usize;
                    if self.state.depth_test_enabled {
                        let needed = (w * h) as usize;
                        if self.state.depth_buffer.len() != needed {
                            self.state.depth_buffer.resize(needed, 1.0);
                        }
                        let frag_z = w0 * v0.z + w1 * v1.z + w2 * v2.z;
                        if frag_z < self.state.depth_buffer[pixel_idx] {
                            self.state.depth_buffer[pixel_idx] = frag_z;
                            self.state.total_depth_tests_passed += 1;
                        } else {
                            self.state.total_depth_tests_rejected += 1;
                            continue;
                        }
                    }

                    let mut r_f = (w0 * v0.r + w1 * v1.r + w2 * v2.r).clamp(0.0, 1.0);
                    let mut g_f = (w0 * v0.g + w1 * v1.g + w2 * v2.g).clamp(0.0, 1.0);
                    let mut b_f = (w0 * v0.b + w1 * v1.b + w2 * v2.b).clamp(0.0, 1.0);

                    if has_tex {
                        let u = (w0 * v0.u + w1 * v1.u + w2 * v2.u).clamp(0.0, 1.0);
                        let v = (w0 * v0.v + w1 * v1.v + w2 * v2.v).clamp(0.0, 1.0);
                        let tx = ((u * (self.state.texture_width as f32 - 1.0)).round() as u32)
                            .min(self.state.texture_width - 1);
                        let ty = ((v * (self.state.texture_height as f32 - 1.0)).round() as u32)
                            .min(self.state.texture_height - 1);
                        let texel_addr = self.state.texture_addr
                            + ((ty * self.state.texture_width + tx) as u64) * 4;
                        if let Ok(texel) = mmu.read_u32(texel_addr) {
                            let tr = (texel & 0xFF) as f32 / 255.0;
                            let tg = ((texel >> 8) & 0xFF) as f32 / 255.0;
                            let tb = ((texel >> 16) & 0xFF) as f32 / 255.0;
                            r_f *= tr;
                            g_f *= tg;
                            b_f *= tb;
                            self.state.total_texels_sampled += 1;
                        }
                    }

                    let r = (r_f * 255.0).round() as u32;
                    let g = (g_f * 255.0).round() as u32;
                    let b = (b_f * 255.0).round() as u32;
                    let rgba = r | (g << 8) | (b << 16) | 0xFF000000;

                    let pixel_addr =
                        self.state.render_target_addr + ((py * w + px) as u64) * 4;
                    let _ = mmu.write_u32(pixel_addr, rgba);
                }
            }
        }
    }
}

#[inline(always)]
fn edge_fn(ax: f32, ay: f32, bx: f32, by: f32, cx: f32, cy: f32) -> f32 {
    (cx - ax) * (by - ay) - (cy - ay) * (bx - ax)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_nvgpu_pushbuffer_clear_and_triangle() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();

        // Map tiny 16x16 render target at 0x90000000 and pushbuffer at 0x80010000
        let rt_addr = 0x90000000u64;
        let vb_addr = 0x80020000u64;
        let pb_addr = 0x80010000u64;
        mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // Write 3 vertices at vb_addr covering the center of the 16x16 viewport
        let verts: [f32; 18] = [
            0.0,  0.8, 0.0, 1.0, 0.0, 0.0, // Top (Red)
           -0.8, -0.8, 0.0, 0.0, 1.0, 0.0, // Bottom-Left (Green)
            0.8, -0.8, 0.0, 0.0, 0.0, 1.0, // Bottom-Right (Blue)
        ];
        for (i, &val) in verts.iter().enumerate() {
            mmu.write_u32(vb_addr + (i * 4) as u64, val.to_bits()).unwrap();
        }

        // Build GPFIFO pushbuffer commands
        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
            rt_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
            16, 16,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
            1,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            vb_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_ARRAYS, 1),
            3, // 3 vertices = 1 triangle
        ];

        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());

        assert_eq!(gpu.state.total_draw_calls, 1);
        assert_eq!(gpu.state.total_triangles_rendered, 1);

        // Center pixel (8, 8) should have non-zero interpolated RGB color!
        let center_px = mmu.read_u32(rt_addr + (8 * 16 + 8) * 4).unwrap();
        assert_ne!(center_px & 0x00FFFFFF, 0);
    }

    #[test]
    fn test_nvgpu_textured_triangle_sampling() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();

        let rt_addr = 0x90000000u64;
        let vb_addr = 0x80020000u64;
        let tex_addr = 0x80030000u64;
        let pb_addr = 0x80010000u64;
        mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(tex_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // Write a 2x2 RGBA8888 texture with solid bright cyan/gold texels (0xFF40C0FF)
        for i in 0..4 {
            mmu.write_u32(tex_addr + i * 4, 0xFF40C0FF).unwrap();
        }

        // Write 3 textured vertices (8 floats each: x, y, z, r, g, b, u, v)
        let verts: [f32; 24] = [
             0.0,  0.8, 0.0, 1.0, 1.0, 1.0, 0.5, 0.0,
            -0.8, -0.8, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0,
             0.8, -0.8, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];
        for (i, &val) in verts.iter().enumerate() {
            mmu.write_u32(vb_addr + (i * 4) as u64, val.to_bits()).unwrap();
        }

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
            rt_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
            16, 16,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_BIND_TEXTURE_2D, 3),
            tex_addr as u32, 2, 2,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            vb_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_TEXTURED_ARRAYS, 1),
            3,
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());
        assert!(gpu.state.total_texels_sampled > 0);
        let center_px = mmu.read_u32(rt_addr + (8 * 16 + 8) * 4).unwrap();
        assert_eq!(center_px, 0xFF40C0FF);
    }

    #[test]
    fn test_nvgpu_zbuffer_depth_occlusion() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();

        let rt_addr = 0x90000000u64;
        let vb_addr = 0x80020000u64;
        let pb_addr = 0x80010000u64;
        mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // Two overlapping triangles:
        // Triangle 0 (Near, z = 0.2): Pure Green (r=0, g=1, b=0)
        // Triangle 1 (Far,  z = 0.8): Pure Red   (r=1, g=0, b=0) — drawn SECOND!
        let verts: [f32; 36] = [
            // Near Green Triangle (z = 0.2)
             0.0,  0.8, 0.2, 0.0, 1.0, 0.0,
            -0.8, -0.8, 0.2, 0.0, 1.0, 0.0,
             0.8, -0.8, 0.2, 0.0, 1.0, 0.0,
            // Far Red Triangle (z = 0.8)
             0.0,  0.8, 0.8, 1.0, 0.0, 0.0,
            -0.8, -0.8, 0.8, 1.0, 0.0, 0.0,
             0.8, -0.8, 0.8, 1.0, 0.0, 0.0,
        ];
        for (i, &val) in verts.iter().enumerate() {
            mmu.write_u32(vb_addr + (i * 4) as u64, val.to_bits()).unwrap();
        }

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
            rt_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
            16, 16,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_DEPTH_TEST_ENABLE, 1),
            1,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
            1,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            vb_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_ARRAYS, 1),
            6, // 2 triangles (6 vertices)
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());
        assert!(gpu.state.total_depth_tests_passed > 0);
        assert!(gpu.state.total_depth_tests_rejected > 0);

        // Because the near green triangle (z=0.2) is in front of the far red triangle (z=0.8),
        // the center pixel (8, 8) MUST remain Pure Green (0xFF00FF00)!
        let center_px = mmu.read_u32(rt_addr + (8 * 16 + 8) * 4).unwrap();
        assert_eq!(center_px, 0xFF00FF00);
    }

    #[test]
    fn test_nvgpu_indexed_draw() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();

        let rt_addr = 0x90000000u64;
        let vb_addr = 0x80020000u64;
        let ib_addr = 0x80030000u64;
        let pb_addr = 0x80010000u64;

        mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(ib_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // 4 Quad vertices (x, y, z, r, g, b):
        // 0: top-left (-0.8, 0.8, 0.5) cyan (0.0, 1.0, 1.0)
        // 1: bottom-left (-0.8, -0.8, 0.5) cyan
        // 2: bottom-right (0.8, -0.8, 0.5) cyan
        // 3: top-right (0.8, 0.8, 0.5) cyan
        let quad_verts: [f32; 24] = [
            -0.8,  0.8, 0.5, 0.0, 1.0, 1.0,
            -0.8, -0.8, 0.5, 0.0, 1.0, 1.0,
             0.8, -0.8, 0.5, 0.0, 1.0, 1.0,
             0.8,  0.8, 0.5, 0.0, 1.0, 1.0,
        ];
        for (i, &val) in quad_verts.iter().enumerate() {
            mmu.write_u32(vb_addr + (i * 4) as u64, val.to_bits()).unwrap();
        }

        // Two triangles using 16-bit indices:
        // Tri 1: 0, 1, 2
        // Tri 2: 0, 2, 3
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        for (i, &idx) in indices.iter().enumerate() {
            mmu.write_u16(ib_addr + (i * 2) as u64, idx).unwrap();
        }

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
            rt_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
            16, 16,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
            1,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            vb_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_INDEX_ARRAY_ADDR, 2),
            ib_addr as u32, 0, // 0 = 16-bit indices
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_INDEXED, 2),
            6, 0, // 6 indices, textured = false
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());
        assert_eq!(gpu.state.total_triangles_rendered, 2);
        assert_eq!(gpu.state.total_draw_calls, 1);

        // Center pixel (8, 8) should be filled with cyan (RGBA: 0x00, 0xFF, 0xFF, 0xFF -> 0xFFFFFF00)
        let center_px = mmu.read_u32(rt_addr + (8 * 16 + 8) * 4).unwrap();
        assert_eq!(center_px, 0xFFFFFF00);
    }

    #[test]
    fn test_gpu_3d_draw_op_queueing_and_draining() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();
        gpu.set_hardware_acceleration(true);
        assert!(gpu.hardware_acceleration_enabled);

        let rt_addr = 0x90000000u64;
        let vb_addr = 0x80020000u64;
        let pb_addr = 0x80010000u64;
        mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        let triangle_verts: [f32; 18] = [
            -0.5,  0.5, 0.0, 1.0, 0.0, 0.0,
            -0.5, -0.5, 0.0, 0.0, 1.0, 0.0,
             0.5, -0.5, 0.0, 0.0, 0.0, 1.0,
        ];
        for (i, &v) in triangle_verts.iter().enumerate() {
            mmu.write_u32(vb_addr + (i * 4) as u64, v.to_bits()).unwrap();
        }

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
            1280, 720,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_CLEAR_COLOR, 4),
            (0.1f32).to_bits(), (0.2f32).to_bits(), (0.3f32).to_bits(), (1.0f32).to_bits(),
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_MVP_MATRIX, 16),
            (2.0f32).to_bits(), 0, 0, 0,
            0, (2.0f32).to_bits(), 0, 0,
            0, 0, (1.0f32).to_bits(), 0,
            0, 0, 0, (1.0f32).to_bits(),
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_DEPTH_TEST_ENABLE, 1),
            1,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
            1,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            vb_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_ARRAYS, 1),
            3,
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());

        let ops = gpu.drain_draw_ops();
        assert!(!ops.is_empty(), "Draw queue should contain captured commands");
        assert!(gpu.draw_queue.is_empty(), "Queue should be empty after drain");

        let mut has_viewport = false;
        let mut has_mvp = false;
        let mut has_depth = false;
        let mut has_clear = false;
        let mut has_draw = false;

        for op in &ops {
            match op {
                Gpu3DDrawOp::SetViewport { width, height, .. } => {
                    assert_eq!(*width, 1280.0);
                    assert_eq!(*height, 720.0);
                    has_viewport = true;
                }
                Gpu3DDrawOp::SetMvpMatrix { matrix } => {
                    assert_eq!(matrix[0], 2.0);
                    assert_eq!(matrix[5], 2.0);
                    has_mvp = true;
                }
                Gpu3DDrawOp::SetDepthTest { enabled } => {
                    assert!(*enabled);
                    has_depth = true;
                }
                Gpu3DDrawOp::ClearSurface { color, depth } => {
                    assert_eq!(color[0], 0.1f32);
                    assert_eq!(color[1], 0.2f32);
                    assert_eq!(color[2], 0.3f32);
                    assert_eq!(*depth, 1.0f32);
                    has_clear = true;
                }
                Gpu3DDrawOp::DrawArrays { vertices, topology } => {
                    assert_eq!(*topology, GpuPrimitiveTopology::TriangleList);
                    assert_eq!(vertices.len(), 3);
                    assert_eq!(vertices[0].position, [-0.5, 0.5, 0.0]);
                    assert_eq!(vertices[0].color, [1.0, 0.0, 0.0]);
                    has_draw = true;
                }
                _ => {}
            }
        }

        assert!(has_viewport && has_mvp && has_depth && has_clear && has_draw);
    }

    #[test]
    fn test_row_major_to_column_major_matrix_transposition() {
        let row_major: [f32; 16] = [
            1.0,  2.0,  3.0,  4.0,
            5.0,  6.0,  7.0,  8.0,
            9.0, 10.0, 11.0, 12.0,
           13.0, 14.0, 15.0, 16.0,
        ];
        let col_major = transpose_row_to_col_major(&row_major);
        for row in 0..4 {
            for col in 0..4 {
                assert_eq!(
                    col_major[col * 4 + row],
                    row_major[row * 4 + col],
                    "Mismatch at row {row}, col {col}"
                );
            }
        }
    }

    #[test]
    fn test_software_rasterizer_fallback_invariant() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();
        // Hardware acceleration explicitly disabled (default):
        assert!(!gpu.hardware_acceleration_enabled);

        let rt_addr = 0x90000000u64;
        let pb_addr = 0x80010000u64;
        mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
            rt_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
            16, 16,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_CLEAR_COLOR, 4),
            (1.0f32).to_bits(), (0.0f32).to_bits(), (0.0f32).to_bits(), (1.0f32).to_bits(),
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
            1,
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());

        // In fallback mode, draw_queue remains empty and mmu is written directly
        assert!(gpu.draw_queue.is_empty());
        let pixel = mmu.read_u32(rt_addr).unwrap();
        // Red clear color: RGBA (0xFF, 0x00, 0x00, 0xFF) -> 0xFF0000FF
        assert_eq!(pixel, 0xFF0000FF);
    }

    #[test]
    fn test_gpu3d_vertex_serialization_endian_safety() {
        let v = Gpu3DVertex::new([1.5, -2.5, 0.75], [0.1, 0.2, 0.3], [0.4, 0.8]);
        let bytes = v.to_bytes();
        assert_eq!(bytes.len(), 32);

        let px = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let py = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let pz = f32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let r  = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let g  = f32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let b  = f32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let u  = f32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let v_coord = f32::from_le_bytes(bytes[28..32].try_into().unwrap());

        assert_eq!(px, 1.5);
        assert_eq!(py, -2.5);
        assert_eq!(pz, 0.75);
        assert_eq!(r, 0.1);
        assert_eq!(g, 0.2);
        assert_eq!(b, 0.3);
        assert_eq!(u, 0.4);
        assert_eq!(v_coord, 0.8);
    }

    #[test]
    fn test_stress_indexed_draw_extreme_32bit_index_bounds() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();
        gpu.set_hardware_acceleration(true);

        let v_addr = 0x80000000u64;
        let i_addr = 0x80010000u64;
        let pb_addr = 0x80020000u64;
        mmu.map_range(v_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(i_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // 3 indices: [0, 1, 0xFFFFFFFF] (primitive restart index / extreme bound)
        mmu.write_u32(i_addr, 0).unwrap();
        mmu.write_u32(i_addr + 4, 1).unwrap();
        mmu.write_u32(i_addr + 8, 0xFFFFFFFF).unwrap();

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            v_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_INDEX_ARRAY_ADDR, 2),
            i_addr as u32,
            1, // is_32bit = 1
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_INDEXED, 2),
            3, // index_count = 3
            0, // textured = false
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        // Must complete instantaneously (<10ms) without hanging, crashing, or allocating 128 GiB
        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());
        assert_eq!(gpu.state.total_draw_calls, 1);
        let ops = gpu.drain_draw_ops();
        for op in ops {
            if let Gpu3DDrawOp::DrawIndexed { vertices, .. } = op {
                assert!(vertices.len() <= 3, "Vertices vector must be bounded, got {}", vertices.len());
            }
        }
    }

    #[test]
    fn test_indexed_draw_sparse_deduplication() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();
        gpu.set_hardware_acceleration(true);

        let v_addr = 0x80000000u64;
        let i_addr = 0x80010000u64;
        let pb_addr = 0x80020000u64;
        mmu.map_range(v_addr, 65536, Permission::READ_WRITE);
        mmu.map_range(i_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // Sparse vertex indices: 50, 100, 200, 50, 200, 300
        let sparse_indices = [50u32, 100u32, 200u32, 50u32, 200u32, 300u32];
        for (i, &idx) in sparse_indices.iter().enumerate() {
            mmu.write_u32(i_addr + (i * 4) as u64, idx).unwrap();
        }

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            v_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_INDEX_ARRAY_ADDR, 2),
            i_addr as u32,
            1, // is_32bit = 1
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_INDEXED, 2),
            6, // index_count = 6
            0, // textured = false
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());
        assert_eq!(gpu.state.total_draw_calls, 1);

        let ops = gpu.drain_draw_ops();
        assert_eq!(ops.len(), 1);
        if let Gpu3DDrawOp::DrawIndexed { vertices, indices, .. } = &ops[0] {
            // Deduplication MUST produce exactly 4 unique vertices (not 301 dense elements)
            assert_eq!(vertices.len(), 4, "Sparse extraction must only fetch unique vertices");
            assert_eq!(indices.len(), 6);
            assert_eq!(indices, &[0, 1, 2, 0, 2, 3]);
        } else {
            panic!("Expected DrawIndexed op");
        }
    }

    #[test]
    fn test_indexed_draw_primitive_restart_filtering() {
        let mut mmu = Memory::new();
        let mut gpu = NvgpuChannel::new();
        gpu.set_hardware_acceleration(true);

        let v_addr = 0x80000000u64;
        let i_addr = 0x80010000u64;
        let pb_addr = 0x80020000u64;
        mmu.map_range(v_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(i_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

        // 16-bit indices with primitive restart:
        // Tri 1: [0, 1, 2] (valid)
        // Tri 2: [0xFFFF, 3, 4] (invalid: contains restart)
        // Tri 3: [2, 3, 4] (valid)
        let raw_indices: [u16; 9] = [0, 1, 2, 0xFFFF, 3, 4, 2, 3, 4];
        for (i, &idx) in raw_indices.iter().enumerate() {
            mmu.write_u16(i_addr + (i * 2) as u64, idx).unwrap();
        }

        let pb_words: Vec<u32> = vec![
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
            v_addr as u32,
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_INDEX_ARRAY_ADDR, 2),
            i_addr as u32,
            0, // is_32bit = 0 (16-bit)
            NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_INDEXED, 2),
            9, // index_count = 9
            0,
        ];
        for (i, &w) in pb_words.iter().enumerate() {
            mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
        }

        gpu.process_pushbuffer(&mut mmu, pb_addr, pb_words.len());
        let ops = gpu.drain_draw_ops();
        assert_eq!(ops.len(), 1);
        if let Gpu3DDrawOp::DrawIndexed { indices, .. } = &ops[0] {
            // Degenerate triangle was dropped; 2 valid triangles (6 indices) remain
            assert_eq!(indices.len(), 6);
        } else {
            panic!("Expected DrawIndexed op");
        }
    }
}

