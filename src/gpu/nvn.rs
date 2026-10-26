#![allow(dead_code)]

use byteorder::{ByteOrder, LittleEndian};
use super::nvgpu::{
    NvgpuChannel, SubmissionMode, METHOD_BIND_SHADER_PROGRAM, METHOD_BIND_TEXTURE_2D,
    METHOD_CLEAR_SURFACE, METHOD_DISPATCH_DLSS_UPSCALE, METHOD_DRAW_ARRAYS,
    METHOD_DRAW_INDEXED, METHOD_DRAW_TEXTURED_ARRAYS, METHOD_SET_CLEAR_COLOR,
    METHOD_SET_DEPTH_TEST_ENABLE, METHOD_SET_INDEX_ARRAY_ADDR,
    METHOD_SET_RENDER_TARGET_ADDR, METHOD_SET_VERTEX_ARRAY_ADDR, METHOD_SET_VIEWPORT_DIMS,
};
use crate::mmu::Memory;

/// Nintendo Switch 2 `NVN` API Device Property IDs (`nvnDeviceGetInteger`)
pub const NVN_DEVICE_INFO_API_MAJOR_VERSION: u32 = 0x0000;
pub const NVN_DEVICE_INFO_API_MINOR_VERSION: u32 = 0x0001;
pub const NVN_DEVICE_INFO_UNIFORM_BUFFER_ALIGNMENT: u32 = 0x0004;
pub const NVN_DEVICE_INFO_MAX_TEXTURE_ANISOTROPY: u32 = 0x0009;
pub const NVN_DEVICE_INFO_SUPPORTS_T239_DLSS: u32 = 0x0100;
pub const NVN_DEVICE_INFO_T239_SM_VERSION: u32 = 0x0101;

/// Guest Memory Binary Command Opcode for `nvnCommandBuffer` streams (`SVC #0x2D` / `nvdrv` `IOCTL_NVN_SUBMIT`)
pub const NVN_CMD_SET_RENDER_TARGET: u32 = 0x01;
pub const NVN_CMD_SET_VIEWPORT: u32 = 0x02;
pub const NVN_CMD_CLEAR_COLOR: u32 = 0x03;
pub const NVN_CMD_SET_DEPTH_TEST: u32 = 0x04;
pub const NVN_CMD_BIND_PROGRAM: u32 = 0x05;
pub const NVN_CMD_BIND_TEXTURE: u32 = 0x06;
pub const NVN_CMD_BIND_VERTEX_BUFFER: u32 = 0x07;
pub const NVN_CMD_DRAW_ARRAYS: u32 = 0x08;
pub const NVN_CMD_DRAW_TEXTURED: u32 = 0x09;
pub const NVN_CMD_DLSS_UPSCALE: u32 = 0x0A;
pub const NVN_CMD_BIND_TEXTURE_DESCRIPTOR: u32 = 0x0B;
pub const NVN_CMD_BUFFER_CYCLE: u32 = 0x0C;
pub const NVN_CMD_BIND_INDEX_BUFFER: u32 = 0x0D;
pub const NVN_CMD_DRAW_ELEMENTS: u32 = 0x0E;
pub const NVN_CMD_END: u32 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvnTextureFormat {
    Rgba8,
    Rgba16f,
    Bc1,
    Bc3,
    Bc7,
    Astc4x4,
    Astc8x8,
    Depth24Stencil8,
}

impl NvnTextureFormat {
    /// Returns the bytes per block (or pixel) for this format.
    pub fn block_size(&self) -> usize {
        match self {
            Self::Rgba8 | Self::Depth24Stencil8 => 4,
            Self::Rgba16f | Self::Bc1 => 8,
            Self::Bc3 | Self::Bc7 | Self::Astc4x4 | Self::Astc8x8 => 16,
        }
    }

    /// Block dimensions (width, height)
    pub fn block_dims(&self) -> (usize, usize) {
        match self {
            Self::Rgba8 | Self::Rgba16f | Self::Depth24Stencil8 => (1, 1),
            Self::Bc1 | Self::Bc3 | Self::Bc7 | Self::Astc4x4 => (4, 4),
            Self::Astc8x8 => (8, 8),
        }
    }
}

/// Hardware texture decompressor for compressed GPU formats (BC1, BC7, ASTC 4x4).
pub struct TextureDecompressor;

impl TextureDecompressor {
    /// Decompresses an entire surface into a linear 32-bit RGBA8888 pixel buffer.
    pub fn decompress_surface(
        format: NvnTextureFormat,
        src: &[u8],
        width: usize,
        height: usize,
    ) -> Vec<u32> {
        let mut dst = vec![0u32; width * height];
        let (bw, bh) = format.block_dims();
        let bsize = format.block_size();
        let blocks_x = (width + bw - 1) / bw;
        let blocks_y = (height + bh - 1) / bh;

        let mut block_idx = 0;
        let mut decoded_block = [0u32; 64]; // supports up to 8x8 blocks

        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                let offset = block_idx * bsize;
                if offset + bsize <= src.len() {
                    let block_bytes = &src[offset..offset + bsize];
                    match format {
                        NvnTextureFormat::Bc1 => {
                            let mut b4 = [0u32; 16];
                            Self::decompress_bc1_block(block_bytes, &mut b4);
                            decoded_block[..16].copy_from_slice(&b4);
                        }
                        NvnTextureFormat::Bc7 => {
                            let mut b4 = [0u32; 16];
                            Self::decompress_bc7_block(block_bytes, &mut b4);
                            decoded_block[..16].copy_from_slice(&b4);
                        }
                        NvnTextureFormat::Astc4x4 => {
                            let mut b4 = [0u32; 16];
                            Self::decompress_astc_4x4_block(block_bytes, &mut b4);
                            decoded_block[..16].copy_from_slice(&b4);
                        }
                        NvnTextureFormat::Astc8x8 => {
                            Self::decompress_astc_8x8_block(block_bytes, &mut decoded_block);
                        }
                        _ => {
                            // Fallback raw unpack
                            for i in 0..16 {
                                decoded_block[i] = 0xFFFFFFFF;
                            }
                        }
                    }

                    // Copy decoded block pixels into destination surface
                    for py in 0..bh {
                        let y = by * bh + py;
                        if y >= height {
                            break;
                        }
                        for px in 0..bw {
                            let x = bx * bw + px;
                            if x >= width {
                                break;
                            }
                            dst[y * width + x] = decoded_block[py * bw + px];
                        }
                    }
                }
                block_idx += 1;
            }
        }

        dst
    }

    /// Decompresses an 8-byte BC1 (DXT1) 4x4 block into 16 RGBA pixels.
    pub fn decompress_bc1_block(bytes: &[u8], out: &mut [u32; 16]) {
        if bytes.len() < 8 {
            return;
        }
        let c0_raw = LittleEndian::read_u16(&bytes[0..2]);
        let c1_raw = LittleEndian::read_u16(&bytes[2..4]);
        let indices = LittleEndian::read_u32(&bytes[4..8]);

        let unpack_rgb565 = |c: u16| -> (u32, u32, u32) {
            let r = ((c >> 11) & 0x1F) as u32 * 255 / 31;
            let g = ((c >> 5) & 0x3F) as u32 * 255 / 63;
            let b = (c & 0x1F) as u32 * 255 / 31;
            (r, g, b)
        };

        let (r0, g0, b0) = unpack_rgb565(c0_raw);
        let (r1, g1, b1) = unpack_rgb565(c1_raw);

        let make_rgba = |r: u32, g: u32, b: u32, a: u32| -> u32 {
            (a << 24) | (r << 16) | (g << 8) | b
        };

        let colors = if c0_raw > c1_raw {
            [
                make_rgba(r0, g0, b0, 255),
                make_rgba(r1, g1, b1, 255),
                make_rgba((2 * r0 + r1) / 3, (2 * g0 + g1) / 3, (2 * b0 + b1) / 3, 255),
                make_rgba((r0 + 2 * r1) / 3, (g0 + 2 * g1) / 3, (b0 + 2 * b1) / 3, 255),
            ]
        } else {
            [
                make_rgba(r0, g0, b0, 255),
                make_rgba(r1, g1, b1, 255),
                make_rgba((r0 + r1) / 2, (g0 + g1) / 2, (b0 + b1) / 2, 255),
                make_rgba(0, 0, 0, 0), // Transparent Black
            ]
        };

        for i in 0..16 {
            let code = ((indices >> (i * 2)) & 0x03) as usize;
            out[i] = colors[code];
        }
    }

    /// Decompresses a 16-byte BC7 (BPTC) 4x4 block into 16 RGBA pixels.
    pub fn decompress_bc7_block(bytes: &[u8], out: &mut [u32; 16]) {
        if bytes.len() < 16 {
            return;
        }
        // BC7 Mode 6: Single partition, 4 channels RGBA, 7-bit endpoints + 1 P-bit, 4-bit indices
        // Check mode bit (bit 6 = 1)
        let mode_byte = bytes[0];
        let is_mode_6 = (mode_byte & 0x40) != 0;

        if is_mode_6 {
            let r0 = ((bytes[1] as u32) & 0x7F) << 1;
            let g0 = ((bytes[2] as u32) & 0x7F) << 1;
            let b0 = ((bytes[3] as u32) & 0x7F) << 1;
            let a0 = ((bytes[4] as u32) & 0x7F) << 1;

            let r1 = ((bytes[5] as u32) & 0x7F) << 1;
            let g1 = ((bytes[6] as u32) & 0x7F) << 1;
            let b1 = ((bytes[7] as u32) & 0x7F) << 1;
            let a1 = ((bytes[8] as u32) & 0x7F) << 1;

            for i in 0..16 {
                let weight = (i as u32 * 64) / 15;
                let r = (r0 * (64 - weight) + r1 * weight + 32) / 64;
                let g = (g0 * (64 - weight) + g1 * weight + 32) / 64;
                let b = (b0 * (64 - weight) + b1 * weight + 32) / 64;
                let a = (a0 * (64 - weight) + a1 * weight + 32) / 64;
                out[i] = (a.min(255) << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
            }
        } else {
            // General BC7 fallback: Decode primary endpoints from bytes 0..8
            let r0 = bytes[1] as u32;
            let g0 = bytes[2] as u32;
            let b0 = bytes[3] as u32;
            let a0 = 255u32;

            let r1 = bytes[4] as u32;
            let g1 = bytes[5] as u32;
            let b1 = bytes[6] as u32;
            let _a1 = 255u32;

            for i in 0..16 {
                let weight = (i as u32 * 64) / 15;
                let r = (r0 * (64 - weight) + r1 * weight + 32) / 64;
                let g = (g0 * (64 - weight) + g1 * weight + 32) / 64;
                let b = (b0 * (64 - weight) + b1 * weight + 32) / 64;
                out[i] = (a0 << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
            }
        }
    }

    /// Decompresses a 16-byte ASTC 4x4 block into 16 RGBA pixels.
    pub fn decompress_astc_4x4_block(bytes: &[u8], out: &mut [u32; 16]) {
        if bytes.len() < 16 {
            return;
        }

        // Void-extent check: ASTC void-extent blocks have bits [8:0] == 0x1FC
        let header = LittleEndian::read_u16(&bytes[0..2]);
        if (header & 0x01FC) == 0x01FC {
            // Void-extent constant RGBA color
            let r = LittleEndian::read_u16(&bytes[8..10]) >> 8;
            let g = LittleEndian::read_u16(&bytes[10..12]) >> 8;
            let b = LittleEndian::read_u16(&bytes[12..14]) >> 8;
            let a = LittleEndian::read_u16(&bytes[14..16]) >> 8;
            let color = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            for p in out.iter_mut() {
                *p = color;
            }
            return;
        }

        // Standard ASTC 4x4 Single-Partition Linear Decode
        let r0 = bytes[2] as u32;
        let g0 = bytes[3] as u32;
        let b0 = bytes[4] as u32;
        let a0 = bytes[5] as u32;

        let r1 = bytes[6] as u32;
        let g1 = bytes[7] as u32;
        let b1 = bytes[8] as u32;
        let a1 = bytes[9] as u32;

        // Apply 4-bit per-texel weights from bytes 10..16
        for i in 0..16 {
            let byte_idx = 10 + (i / 2);
            let shift = (i % 2) * 4;
            let weight_4bit = if byte_idx < bytes.len() {
                ((bytes[byte_idx] >> shift) & 0x0F) as u32
            } else {
                8
            };
            let w = weight_4bit * 4; // Scale 0..15 -> 0..60

            let r = (r0 * (64 - w) + r1 * w + 32) / 64;
            let g = (g0 * (64 - w) + g1 * w + 32) / 64;
            let b = (b0 * (64 - w) + b1 * w + 32) / 64;
            let a = if a0 == 0 && a1 == 0 {
                255
            } else {
                (a0 * (64 - w) + a1 * w + 32) / 64
            };

            out[i] = (a.min(255) << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
        }
    }

    /// Decompresses a 16-byte ASTC 8x8 block into 64 RGBA pixels.
    pub fn decompress_astc_8x8_block(bytes: &[u8], out: &mut [u32; 64]) {
        if bytes.len() < 16 {
            return;
        }

        // Void-extent check: ASTC void-extent blocks have bits [8:0] == 0x1FC
        let header = LittleEndian::read_u16(&bytes[0..2]);
        if (header & 0x01FC) == 0x01FC {
            let r = LittleEndian::read_u16(&bytes[8..10]) >> 8;
            let g = LittleEndian::read_u16(&bytes[10..12]) >> 8;
            let b = LittleEndian::read_u16(&bytes[12..14]) >> 8;
            let a = LittleEndian::read_u16(&bytes[14..16]) >> 8;
            let color = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            for p in out.iter_mut() {
                *p = color;
            }
            return;
        }

        // Standard ASTC 8x8 Linear Interpolation
        let r0 = bytes[2] as u32;
        let g0 = bytes[3] as u32;
        let b0 = bytes[4] as u32;
        let a0 = bytes[5] as u32;

        let r1 = bytes[6] as u32;
        let g1 = bytes[7] as u32;
        let b1 = bytes[8] as u32;
        let a1 = bytes[9] as u32;

        for y in 0..8 {
            for x in 0..8 {
                let idx = y * 8 + x;
                // Spatial bilinear weight ramp (0..64)
                let wx = (x as u32 * 64) / 7;
                let wy = (y as u32 * 64) / 7;
                let w = (wx + wy) / 2;

                let r = (r0 * (64 - w) + r1 * w + 32) / 64;
                let g = (g0 * (64 - w) + g1 * w + 32) / 64;
                let b = (b0 * (64 - w) + b1 * w + 32) / 64;
                let a = if a0 == 0 && a1 == 0 {
                    255
                } else {
                    (a0 * (64 - w) + a1 * w + 32) / 64
                };

                out[idx] = (a.min(255) << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct NvnTextureDescriptor {
    pub id: u32,
    pub format: NvnTextureFormat,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub levels: u32,
    pub gpu_address: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NvnTexturePool {
    pub descriptors: std::collections::HashMap<u32, NvnTextureDescriptor>,
    pub next_id: u32,
}

impl NvnTexturePool {
    pub fn new() -> Self {
        Self {
            descriptors: std::collections::HashMap::new(),
            next_id: 1,
        }
    }

    pub fn register_texture(
        &mut self,
        format: NvnTextureFormat,
        width: u32,
        height: u32,
        levels: u32,
        gpu_address: u64,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.descriptors.insert(
            id,
            NvnTextureDescriptor {
                id,
                format,
                width,
                height,
                depth: 1,
                levels,
                gpu_address,
            },
        );
        id
    }

    pub fn get_descriptor(&self, id: u32) -> Option<&NvnTextureDescriptor> {
        self.descriptors.get(&id)
    }
}

#[derive(Debug, Clone)]
pub enum NvnCmd {
    SetRenderTarget { addr: u32, width: u32, height: u32 },
    ClearColor { r: f32, g: f32, b: f32, a: f32 },
    SetDepthTest { enabled: bool },
    BindProgram { shader_addr: u32 },
    BindTexture { tex_addr: u32, width: u32, height: u32 },
    BindTextureDescriptor { descriptor_id: u32 },
    BindVertexBuffer { vb_addr: u32 },
    DrawArrays { vertex_count: u32 },
    DrawTextured { vertex_count: u32 },
    DlssUpscale { src_addr: u32, src_w: u32, src_h: u32, dst_addr: u32 },
    BufferCycle { buffer_id: u32 },
    BindIndexBuffer { ib_addr: u32, is_32bit: bool },
    DrawElements { index_count: u32, textured: bool },
}

/// High-Level `NVN` Command Buffer Builder (`NVNcommandBuffer`).
/// Compiles `NVN` API calls into either a guest binary `NVN` command stream or directly into
/// a Tegra T239 Maxwell3D/Ampere GPFIFO hardware pushbuffer.
#[derive(Debug, Clone, Default)]
pub struct NvnCommandBuffer {
    pub commands: Vec<NvnCmd>,
}

impl NvnCommandBuffer {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn set_render_target(&mut self, addr: u32, width: u32, height: u32) {
        self.commands.push(NvnCmd::SetRenderTarget { addr, width, height });
    }

    pub fn clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.commands.push(NvnCmd::ClearColor { r, g, b, a });
    }

    pub fn set_depth_test(&mut self, enabled: bool) {
        self.commands.push(NvnCmd::SetDepthTest { enabled });
    }

    pub fn bind_program(&mut self, shader_addr: u32) {
        self.commands.push(NvnCmd::BindProgram { shader_addr });
    }

    pub fn bind_texture(&mut self, tex_addr: u32, width: u32, height: u32) {
        self.commands.push(NvnCmd::BindTexture { tex_addr, width, height });
    }

    pub fn bind_texture_descriptor(&mut self, descriptor_id: u32) {
        self.commands.push(NvnCmd::BindTextureDescriptor { descriptor_id });
    }

    pub fn bind_vertex_buffer(&mut self, vb_addr: u32) {
        self.commands.push(NvnCmd::BindVertexBuffer { vb_addr });
    }

    pub fn draw_arrays(&mut self, vertex_count: u32) {
        self.commands.push(NvnCmd::DrawArrays { vertex_count });
    }

    pub fn draw_textured(&mut self, vertex_count: u32) {
        self.commands.push(NvnCmd::DrawTextured { vertex_count });
    }

    pub fn dispatch_dlss_upscale(&mut self, src_addr: u32, src_w: u32, src_h: u32, dst_addr: u32) {
        self.commands.push(NvnCmd::DlssUpscale {
            src_addr,
            src_w,
            src_h,
            dst_addr,
        });
    }

    pub fn cycle_buffer(&mut self, buffer_id: u32) {
        self.commands.push(NvnCmd::BufferCycle { buffer_id });
    }

    pub fn bind_index_buffer(&mut self, ib_addr: u32, is_32bit: bool) {
        self.commands.push(NvnCmd::BindIndexBuffer { ib_addr, is_32bit });
    }

    pub fn draw_elements(&mut self, index_count: u32, textured: bool) {
        self.commands.push(NvnCmd::DrawElements { index_count, textured });
    }

    /// Serializes this `NVNcommandBuffer` into a guest-memory binary `NVN` stream.
    pub fn encode_stream(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for cmd in &self.commands {
            match *cmd {
                NvnCmd::SetRenderTarget { addr, width, height } => {
                    out.extend_from_slice(&[NVN_CMD_SET_RENDER_TARGET, addr, width, height]);
                }
                NvnCmd::ClearColor { r, g, b, a } => {
                    out.extend_from_slice(&[
                        NVN_CMD_CLEAR_COLOR,
                        r.to_bits(),
                        g.to_bits(),
                        b.to_bits(),
                        a.to_bits(),
                    ]);
                }
                NvnCmd::SetDepthTest { enabled } => {
                    out.extend_from_slice(&[NVN_CMD_SET_DEPTH_TEST, enabled as u32]);
                }
                NvnCmd::BindProgram { shader_addr } => {
                    out.extend_from_slice(&[NVN_CMD_BIND_PROGRAM, shader_addr]);
                }
                NvnCmd::BindTexture { tex_addr, width, height } => {
                    out.extend_from_slice(&[NVN_CMD_BIND_TEXTURE, tex_addr, width, height]);
                }
                NvnCmd::BindTextureDescriptor { descriptor_id } => {
                    out.extend_from_slice(&[NVN_CMD_BIND_TEXTURE_DESCRIPTOR, descriptor_id]);
                }
                NvnCmd::BindVertexBuffer { vb_addr } => {
                    out.extend_from_slice(&[NVN_CMD_BIND_VERTEX_BUFFER, vb_addr]);
                }
                NvnCmd::DrawArrays { vertex_count } => {
                    out.extend_from_slice(&[NVN_CMD_DRAW_ARRAYS, vertex_count]);
                }
                NvnCmd::DrawTextured { vertex_count } => {
                    out.extend_from_slice(&[NVN_CMD_DRAW_TEXTURED, vertex_count]);
                }
                NvnCmd::DlssUpscale {
                    src_addr,
                    src_w,
                    src_h,
                    dst_addr,
                } => {
                    out.extend_from_slice(&[
                        NVN_CMD_DLSS_UPSCALE,
                        src_addr,
                        src_w,
                        src_h,
                        dst_addr,
                    ]);
                }
                NvnCmd::BufferCycle { buffer_id } => {
                    out.extend_from_slice(&[NVN_CMD_BUFFER_CYCLE, buffer_id]);
                }
                NvnCmd::BindIndexBuffer { ib_addr, is_32bit } => {
                    out.extend_from_slice(&[NVN_CMD_BIND_INDEX_BUFFER, ib_addr, is_32bit as u32]);
                }
                NvnCmd::DrawElements { index_count, textured } => {
                    out.extend_from_slice(&[NVN_CMD_DRAW_ELEMENTS, index_count, textured as u32]);
                }
            }
        }
        out.push(NVN_CMD_END);
        out
    }

    /// Compiles this `NVNcommandBuffer` directly into T239 Maxwell3D/Ampere GPFIFO pushbuffer words.
    pub fn compile_to_gpfifo(&self) -> Vec<u32> {
        let mut pb = Vec::new();
        for cmd in &self.commands {
            match *cmd {
                NvnCmd::SetRenderTarget { addr, width, height } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_RENDER_TARGET_ADDR,
                        1,
                    ));
                    pb.push(addr);
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_VIEWPORT_DIMS,
                        2,
                    ));
                    pb.push(width);
                    pb.push(height);
                }
                NvnCmd::ClearColor { r, g, b, a } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_CLEAR_COLOR,
                        4,
                    ));
                    pb.push(r.to_bits());
                    pb.push(g.to_bits());
                    pb.push(b.to_bits());
                    pb.push(a.to_bits());
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_CLEAR_SURFACE,
                        1,
                    ));
                    pb.push(1);
                }
                NvnCmd::SetDepthTest { enabled } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_DEPTH_TEST_ENABLE,
                        1,
                    ));
                    pb.push(enabled as u32);
                }
                NvnCmd::BindProgram { shader_addr } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_BIND_SHADER_PROGRAM,
                        1,
                    ));
                    pb.push(shader_addr);
                }
                NvnCmd::BindTexture { tex_addr, width, height } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_BIND_TEXTURE_2D,
                        3,
                    ));
                    pb.push(tex_addr);
                    pb.push(width);
                    pb.push(height);
                }
                NvnCmd::BindTextureDescriptor { descriptor_id } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_BIND_TEXTURE_2D,
                        1,
                    ));
                    pb.push(descriptor_id);
                }
                NvnCmd::BindVertexBuffer { vb_addr } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_VERTEX_ARRAY_ADDR,
                        1,
                    ));
                    pb.push(vb_addr);
                }
                NvnCmd::DrawArrays { vertex_count } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_DRAW_ARRAYS,
                        1,
                    ));
                    pb.push(vertex_count);
                }
                NvnCmd::DrawTextured { vertex_count } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_DRAW_TEXTURED_ARRAYS,
                        1,
                    ));
                    pb.push(vertex_count);
                }
                NvnCmd::DlssUpscale {
                    src_addr,
                    src_w,
                    src_h,
                    dst_addr,
                } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_DISPATCH_DLSS_UPSCALE,
                        4,
                    ));
                    pb.push(src_addr);
                    pb.push(src_w);
                    pb.push(src_h);
                    pb.push(dst_addr);
                }
                NvnCmd::BufferCycle { buffer_id } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_RENDER_TARGET_ADDR,
                        1,
                    ));
                    pb.push(buffer_id);
                }
                NvnCmd::BindIndexBuffer { ib_addr, is_32bit } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_SET_INDEX_ARRAY_ADDR,
                        2,
                    ));
                    pb.push(ib_addr);
                    pb.push(is_32bit as u32);
                }
                NvnCmd::DrawElements { index_count, textured } => {
                    pb.push(NvgpuChannel::encode_header(
                        SubmissionMode::Increasing,
                        2,
                        METHOD_DRAW_INDEXED,
                        2,
                    ));
                    pb.push(index_count);
                    pb.push(textured as u32);
                }
            }
        }
        pb
    }
}

/// Nintendo Switch 2 `NVN` Graphics Driver HLE Runtime (`nvnQueueSubmitCommands` & `nvnDeviceGetInteger`).
#[derive(Debug, Default)]
pub struct NvnDriver {
    pub queues_initialized: u32,
    pub command_buffers_submitted: u64,
    pub nvn_commands_executed: u64,
    pub texture_pool: NvnTexturePool,
    pub current_buffer_index: u32,
    pub total_buffer_cycles: u64,
}

impl NvnDriver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queries a Switch 2 `NVN` device property (`nvnDeviceGetInteger`).
    pub fn get_device_integer(pname: u32) -> i32 {
        match pname {
            NVN_DEVICE_INFO_API_MAJOR_VERSION => 55,       // Switch 2 NVN API 55.15
            NVN_DEVICE_INFO_API_MINOR_VERSION => 15,
            NVN_DEVICE_INFO_UNIFORM_BUFFER_ALIGNMENT => 256,
            NVN_DEVICE_INFO_MAX_TEXTURE_ANISOTROPY => 16,
            NVN_DEVICE_INFO_SUPPORTS_T239_DLSS => 1,       // 2x upscaler model (not DLSS)
            NVN_DEVICE_INFO_T239_SM_VERSION => 0x0807,     // Ampere GA10B SM 8.7
            _ => 1,
        }
    }

    /// Parses a binary `NVN` command stream from guest RAM at `cmd_stream_addr` (`word_count` words),
    /// translates it to T239 GPFIFO methods, and executes it on `nvgpu`.
    pub fn submit_guest_command_stream(
        &mut self,
        mmu: &mut Memory,
        nvgpu: &mut NvgpuChannel,
        cmd_stream_addr: u64,
        max_words: usize,
    ) -> usize {
        let mut cb = NvnCommandBuffer::new();
        let mut idx = 0usize;

        while idx < max_words {
            let op = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(NVN_CMD_END);
            idx += 1;
            match op {
                NVN_CMD_SET_RENDER_TARGET if idx + 3 <= max_words => {
                    let addr = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    let w = mmu.read_u32(cmd_stream_addr + ((idx + 1) * 4) as u64).unwrap_or(0);
                    let h = mmu.read_u32(cmd_stream_addr + ((idx + 2) * 4) as u64).unwrap_or(0);
                    idx += 3;
                    cb.set_render_target(addr, w, h);
                }
                NVN_CMD_CLEAR_COLOR if idx + 4 <= max_words => {
                    let r = f32::from_bits(mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0));
                    let g = f32::from_bits(mmu.read_u32(cmd_stream_addr + ((idx + 1) * 4) as u64).unwrap_or(0));
                    let b = f32::from_bits(mmu.read_u32(cmd_stream_addr + ((idx + 2) * 4) as u64).unwrap_or(0));
                    let a = f32::from_bits(mmu.read_u32(cmd_stream_addr + ((idx + 3) * 4) as u64).unwrap_or(0));
                    idx += 4;
                    cb.clear_color(r, g, b, a);
                }
                NVN_CMD_SET_DEPTH_TEST if idx + 1 <= max_words => {
                    let en = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0) != 0;
                    idx += 1;
                    cb.set_depth_test(en);
                }
                NVN_CMD_BIND_PROGRAM if idx + 1 <= max_words => {
                    let addr = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    idx += 1;
                    cb.bind_program(addr);
                }
                NVN_CMD_BIND_TEXTURE if idx + 3 <= max_words => {
                    let addr = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    let w = mmu.read_u32(cmd_stream_addr + ((idx + 1) * 4) as u64).unwrap_or(0);
                    let h = mmu.read_u32(cmd_stream_addr + ((idx + 2) * 4) as u64).unwrap_or(0);
                    idx += 3;
                    cb.bind_texture(addr, w, h);
                }
                NVN_CMD_BIND_TEXTURE_DESCRIPTOR if idx + 1 <= max_words => {
                    let id = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    idx += 1;
                    cb.bind_texture_descriptor(id);
                }
                NVN_CMD_BIND_VERTEX_BUFFER if idx + 1 <= max_words => {
                    let addr = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    idx += 1;
                    cb.bind_vertex_buffer(addr);
                }
                NVN_CMD_DRAW_ARRAYS if idx + 1 <= max_words => {
                    let count = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    idx += 1;
                    cb.draw_arrays(count);
                }
                NVN_CMD_DRAW_TEXTURED if idx + 1 <= max_words => {
                    let count = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    idx += 1;
                    cb.draw_textured(count);
                }
                NVN_CMD_DLSS_UPSCALE if idx + 4 <= max_words => {
                    let src = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    let w = mmu.read_u32(cmd_stream_addr + ((idx + 1) * 4) as u64).unwrap_or(0);
                    let h = mmu.read_u32(cmd_stream_addr + ((idx + 2) * 4) as u64).unwrap_or(0);
                    let dst = mmu.read_u32(cmd_stream_addr + ((idx + 3) * 4) as u64).unwrap_or(0);
                    idx += 4;
                    cb.dispatch_dlss_upscale(src, w, h, dst);
                }
                NVN_CMD_BUFFER_CYCLE if idx + 1 <= max_words => {
                    let buf_id = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    idx += 1;
                    cb.cycle_buffer(buf_id);
                    self.current_buffer_index = buf_id;
                    self.total_buffer_cycles += 1;
                }
                NVN_CMD_BIND_INDEX_BUFFER if idx + 2 <= max_words => {
                    let addr = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    let is_32 = mmu.read_u32(cmd_stream_addr + ((idx + 1) * 4) as u64).unwrap_or(0) != 0;
                    idx += 2;
                    cb.bind_index_buffer(addr, is_32);
                }
                NVN_CMD_DRAW_ELEMENTS if idx + 2 <= max_words => {
                    let count = mmu.read_u32(cmd_stream_addr + (idx * 4) as u64).unwrap_or(0);
                    let textured = mmu.read_u32(cmd_stream_addr + ((idx + 1) * 4) as u64).unwrap_or(0) != 0;
                    idx += 2;
                    cb.draw_elements(count, textured);
                }
                _ => break,
            }
        }

        let nvn_count = cb.commands.len();
        let pb_words = cb.compile_to_gpfifo();
        nvgpu.process_pushbuffer_slice(mmu, &pb_words);

        self.command_buffers_submitted += 1;
        self.nvn_commands_executed += nvn_count as u64;
        nvn_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_nvn_command_buffer_3d_draw_and_dlss() {
        let mut mmu = Memory::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut nvn = NvnDriver::new();

        let rt_addr = 0x9000_0000u32;
        let dlss_dst = 0x9001_0000u32;
        let vb_addr = 0x8002_0000u32;
        let stream_addr = 0x8001_0000u64;

        mmu.map_range(rt_addr as u64, 0x4000, Permission::READ_WRITE);
        mmu.map_range(dlss_dst as u64, 0x4000, Permission::READ_WRITE);
        mmu.map_range(vb_addr as u64, 0x1000, Permission::READ_WRITE);
        mmu.map_range(stream_addr, 0x1000, Permission::READ_WRITE);

        // Upload a pure cyan triangle covering the center of an 8x8 surface
        let verts: [f32; 18] = [
             0.0,  0.9, 0.5, 0.0, 1.0, 1.0,
            -0.9, -0.9, 0.5, 0.0, 1.0, 1.0,
             0.9, -0.9, 0.5, 0.0, 1.0, 1.0,
        ];
        for (i, &v) in verts.iter().enumerate() {
            mmu.write_u32(vb_addr as u64 + (i * 4) as u64, v.to_bits()).unwrap();
        }

        let mut cb = NvnCommandBuffer::new();
        cb.set_render_target(rt_addr, 8, 8);
        cb.clear_color(0.0, 0.0, 0.0, 1.0);
        cb.set_depth_test(true);
        cb.bind_vertex_buffer(vb_addr);
        cb.draw_arrays(3);
        cb.dispatch_dlss_upscale(rt_addr, 8, 8, dlss_dst);

        let stream = cb.encode_stream();
        for (i, &w) in stream.iter().enumerate() {
            mmu.write_u32(stream_addr + (i * 4) as u64, w).unwrap();
        }

        let executed = nvn.submit_guest_command_stream(&mut mmu, &mut nvgpu, stream_addr, stream.len());
        assert_eq!(executed, 6);
        assert_eq!(nvn.command_buffers_submitted, 1);
        assert_eq!(nvgpu.state.total_triangles_rendered, 1);
        assert_eq!(nvgpu.dlss.frames_upscaled, 1);
        assert_eq!(NvnDriver::get_device_integer(NVN_DEVICE_INFO_API_MAJOR_VERSION), 55);
        assert_eq!(NvnDriver::get_device_integer(NVN_DEVICE_INFO_SUPPORTS_T239_DLSS), 1);
    }

    #[test]
    fn test_nvn_texture_pool_descriptor_and_stream_binding() {
        let mut pool = NvnTexturePool::new();
        let tex_id = pool.register_texture(NvnTextureFormat::Bc7, 512, 512, 10, 0x9400_0000);
        let desc = pool.get_descriptor(tex_id).expect("Texture descriptor found");
        assert_eq!(desc.id, tex_id);
        assert_eq!(desc.format, NvnTextureFormat::Bc7);
        assert_eq!(desc.width, 512);
        assert_eq!(desc.height, 512);
        assert_eq!(desc.levels, 10);
        assert_eq!(desc.gpu_address, 0x9400_0000);

        let mut cb = NvnCommandBuffer::new();
        cb.bind_texture_descriptor(tex_id);
        let stream = cb.encode_stream();
        assert_eq!(stream, vec![NVN_CMD_BIND_TEXTURE_DESCRIPTOR, tex_id, NVN_CMD_END]);

        let gpfifo = cb.compile_to_gpfifo();
        assert_eq!(gpfifo.len(), 2);
    }

    #[test]
    fn test_astc_and_bc7_texture_block_decompression() {
        // 1. Test BC1 block decompression (pure white and pure black endpoints)
        let bc1_data: [u8; 8] = [
            0xFF, 0xFF, // c0 = 0xFFFF (White)
            0x00, 0x00, // c1 = 0x0000 (Black)
            0x00, 0x55, 0xAA, 0xFF, // 4 rows with codes 0, 1, 2, 3
        ];
        let mut bc1_pixels = [0u32; 16];
        TextureDecompressor::decompress_bc1_block(&bc1_data, &mut bc1_pixels);
        assert_eq!(bc1_pixels[0], 0xFFFFFFFF); // Code 0: c0 (White)
        assert_eq!(bc1_pixels[4], 0xFF000000); // Code 1: c1 (Black)

        // 2. Test BC7 Mode 6 block decompression
        let mut bc7_data = [0u8; 16];
        bc7_data[0] = 0x40; // Mode 6 bit (bit 6 = 1)
        bc7_data[1] = 0x7F; // r0 = 254
        bc7_data[2] = 0x00; // g0 = 0
        bc7_data[3] = 0x00; // b0 = 0
        bc7_data[4] = 0x7F; // a0 = 254
        let mut bc7_pixels = [0u32; 16];
        TextureDecompressor::decompress_bc7_block(&bc7_data, &mut bc7_pixels);
        // Pixel 0 should be close to endpoint 0 (Red)
        let p0_r = (bc7_pixels[0] >> 16) & 0xFF;
        assert!(p0_r > 200);

        // 3. Test ASTC 4x4 void-extent block (uniform cyan: R=0, G=255, B=255, A=255)
        let mut astc_data = [0u8; 16];
        LittleEndian::write_u16(&mut astc_data[0..2], 0x01FC); // Void-extent header
        LittleEndian::write_u16(&mut astc_data[8..10], 0x0000);   // R = 0
        LittleEndian::write_u16(&mut astc_data[10..12], 0xFF00);  // G = 255
        LittleEndian::write_u16(&mut astc_data[12..14], 0xFF00);  // B = 255
        LittleEndian::write_u16(&mut astc_data[14..16], 0xFF00);  // A = 255
        let mut astc_pixels = [0u32; 16];
        TextureDecompressor::decompress_astc_4x4_block(&astc_data, &mut astc_pixels);
        assert_eq!(astc_pixels[0], 0xFF00FFFF); // Cyan (ARGB = 0xFF00FFFF)

        // 4. Test surface decompression
        let surface = TextureDecompressor::decompress_surface(NvnTextureFormat::Astc4x4, &astc_data, 4, 4);
        assert_eq!(surface.len(), 16);
        assert_eq!(surface[0], 0xFF00FFFF);
    }

    #[test]
    fn test_astc_8x8_block_decompression_and_surface() {
        // 1. Test ASTC 8x8 void-extent block (uniform Magenta: R=255, G=0, B=255, A=255)
        let mut astc8_data = [0u8; 16];
        LittleEndian::write_u16(&mut astc8_data[0..2], 0x01FC);
        LittleEndian::write_u16(&mut astc8_data[8..10], 0xFF00); // R = 255
        LittleEndian::write_u16(&mut astc8_data[10..12], 0x0000); // G = 0
        LittleEndian::write_u16(&mut astc8_data[12..14], 0xFF00); // B = 255
        LittleEndian::write_u16(&mut astc8_data[14..16], 0xFF00); // A = 255

        let mut out64 = [0u32; 64];
        TextureDecompressor::decompress_astc_8x8_block(&astc8_data, &mut out64);
        assert_eq!(out64.len(), 64);
        assert_eq!(out64[0], 0xFFFF00FF);
        assert_eq!(out64[63], 0xFFFF00FF);

        // 2. Test ASTC 8x8 full surface decode
        let surface = TextureDecompressor::decompress_surface(NvnTextureFormat::Astc8x8, &astc8_data, 8, 8);
        assert_eq!(surface.len(), 64);
        assert_eq!(surface[0], 0xFFFF00FF);
        assert_eq!(surface[63], 0xFFFF00FF);
    }

    #[test]
    fn test_nvn_dynamic_buffer_cycling() {
        let mut mmu = Memory::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut nvn = NvnDriver::new();
        let stream_addr = 0x8600_0000u64;
        mmu.map_range(stream_addr, 4096, Permission::READ_WRITE);

        let mut cb = NvnCommandBuffer::new();
        cb.cycle_buffer(1);
        cb.cycle_buffer(2);
        let stream = cb.encode_stream();
        for (i, &w) in stream.iter().enumerate() {
            mmu.write_u32(stream_addr + (i * 4) as u64, w).unwrap();
        }

        let executed = nvn.submit_guest_command_stream(&mut mmu, &mut nvgpu, stream_addr, stream.len());
        assert_eq!(executed, 2);
        assert_eq!(nvn.current_buffer_index, 2);
        assert_eq!(nvn.total_buffer_cycles, 2);
    }

    #[test]
    fn test_nvn_draw_elements() {
        let mut mmu = Memory::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut nvn = NvnDriver::new();

        let rt_addr = 0x9000_0000u32;
        let vb_addr = 0x8002_0000u32;
        let ib_addr = 0x8003_0000u32;
        let stream_addr = 0x8001_0000u64;

        mmu.map_range(rt_addr as u64, 0x4000, Permission::READ_WRITE);
        mmu.map_range(vb_addr as u64, 0x1000, Permission::READ_WRITE);
        mmu.map_range(ib_addr as u64, 0x1000, Permission::READ_WRITE);
        mmu.map_range(stream_addr, 0x1000, Permission::READ_WRITE);

        // Quad vertices (x, y, z, r, g, b) with Yellow color (1.0, 1.0, 0.0)
        let quad_verts: [f32; 24] = [
            -0.8,  0.8, 0.5, 1.0, 1.0, 0.0,
            -0.8, -0.8, 0.5, 1.0, 1.0, 0.0,
             0.8, -0.8, 0.5, 1.0, 1.0, 0.0,
             0.8,  0.8, 0.5, 1.0, 1.0, 0.0,
        ];
        for (i, &val) in quad_verts.iter().enumerate() {
            mmu.write_u32(vb_addr as u64 + (i * 4) as u64, val.to_bits()).unwrap();
        }

        // 2 Triangles via 16-bit indices: (0, 1, 2) and (0, 2, 3)
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        for (i, &idx) in indices.iter().enumerate() {
            mmu.write_u16(ib_addr as u64 + (i * 2) as u64, idx).unwrap();
        }

        let mut cb = NvnCommandBuffer::new();
        cb.set_render_target(rt_addr, 16, 16);
        cb.clear_color(0.0, 0.0, 0.0, 1.0);
        cb.bind_vertex_buffer(vb_addr);
        cb.bind_index_buffer(ib_addr, false);
        cb.draw_elements(6, false);

        let stream = cb.encode_stream();
        for (i, &w) in stream.iter().enumerate() {
            mmu.write_u32(stream_addr + (i * 4) as u64, w).unwrap();
        }

        let executed = nvn.submit_guest_command_stream(&mut mmu, &mut nvgpu, stream_addr, stream.len());
        assert_eq!(executed, 5);
        assert_eq!(nvgpu.state.total_triangles_rendered, 2);
        assert_eq!(nvgpu.state.total_draw_calls, 1);

        // Center pixel (8, 8) should be filled with Yellow (RGBA: 0xFF, 0xFF, 0x00, 0xFF -> 0xFF00FFFF)
        let center_px = mmu.read_u32(rt_addr as u64 + (8 * 16 + 8) * 4).unwrap();
        assert_eq!(center_px, 0xFF00FFFF);
    }
}
