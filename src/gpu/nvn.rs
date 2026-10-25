#![allow(dead_code)]

use super::nvgpu::{
    NvgpuChannel, SubmissionMode, METHOD_BIND_SHADER_PROGRAM, METHOD_BIND_TEXTURE_2D,
    METHOD_CLEAR_SURFACE, METHOD_DISPATCH_DLSS_UPSCALE, METHOD_DRAW_ARRAYS,
    METHOD_DRAW_TEXTURED_ARRAYS, METHOD_SET_CLEAR_COLOR, METHOD_SET_DEPTH_TEST_ENABLE,
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
pub const NVN_CMD_END: u32 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvnTextureFormat {
    Rgba8,
    Rgba16f,
    Bc1,
    Bc3,
    Bc7,
    Depth24Stencil8,
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
            NVN_DEVICE_INFO_SUPPORTS_T239_DLSS => 1,       // Hardware Tensor Core DLSS
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
}
