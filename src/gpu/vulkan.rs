#![allow(dead_code)]

use super::shader::{SPIRV_MAGIC, SPIRV_VERSION_1_5};
use crate::mmu::Memory;

/// Legacy bookkeeping fixture. Does not open a device or execute Vulkan commands.
#[derive(Debug, Clone)]
pub struct VulkanDeviceInfo {
    pub device_name: String,
    pub api_version: (u32, u32, u32),
    pub driver_version: u32,
    pub device_type: &'static str,
    pub supports_mesh_shaders: bool,
    pub supports_upscale_model: bool, // model flag only; no DLSS is implemented
}

impl Default for VulkanDeviceInfo {
    fn default() -> Self {
        Self {
            device_name: "No device (legacy pipeline fixture)".to_string(),
            api_version: (0, 0, 0),
            driver_version: 0,
            device_type: "None",
            supports_mesh_shaders: false,
            supports_upscale_model: false,
        }
    }
}

/// Fixture storing SPIR-V words after header checks only.
#[derive(Debug, Clone)]
pub struct VulkanShaderModule {
    pub spirv_words: Vec<u32>,
    pub stage: ShaderStage,
    pub entry_point: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

impl VulkanShaderModule {
    /// Checks the SPIR-V header; does not validate instructions or create a driver module.
    pub fn create_from_spirv(words: &[u32], stage: ShaderStage, entry_point: &str) -> Result<Self, &'static str> {
        if words.len() < 5 {
            return Err("SPIR-V bytecode too short (< 5 header words)");
        }
        if words[0] != SPIRV_MAGIC {
            return Err("Invalid SPIR-V Magic Header (expected 0x07230203)");
        }
        if words[1] != SPIRV_VERSION_1_5 {
            return Err("Unsupported SPIR-V version (expected 1.5)");
        }

        Ok(Self {
            spirv_words: words.to_vec(),
            stage,
            entry_point: entry_point.to_string(),
        })
    }
}

/// Legacy pipeline record; does not bridge NVN or create a host pipeline.
#[derive(Debug, Clone)]
pub struct VulkanGraphicsPipeline {
    pub vertex_shader: VulkanShaderModule,
    pub fragment_shader: VulkanShaderModule,
    pub depth_test_enabled: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

/// Legacy pipeline bookkeeping; real guest execution lives in ampere/render/host.
#[derive(Debug)]
pub struct VulkanGpuBackend {
    pub device_info: VulkanDeviceInfo,
    pub active_pipeline: Option<VulkanGraphicsPipeline>,
    pub pipelines_created: u64,
    pub draw_calls_submitted: u64,
    pub spirv_modules_loaded: u64,
    pub hardware_frames_presented: u64,
}

impl Default for VulkanGpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VulkanGpuBackend {
    pub fn new() -> Self {
        Self {
            device_info: VulkanDeviceInfo::default(),
            active_pipeline: None,
            pipelines_created: 0,
            draw_calls_submitted: 0,
            spirv_modules_loaded: 0,
            hardware_frames_presented: 0,
        }
    }

    /// Creates a graphics pipeline combining guest SPIR-V vertex and fragment stages.
    pub fn create_graphics_pipeline(
        &mut self,
        vs_spirv: &[u32],
        fs_spirv: &[u32],
        viewport_w: u32,
        viewport_h: u32,
        depth_test: bool,
    ) -> Result<(), &'static str> {
        let vs = VulkanShaderModule::create_from_spirv(vs_spirv, ShaderStage::Vertex, "main")?;
        let fs = VulkanShaderModule::create_from_spirv(fs_spirv, ShaderStage::Fragment, "main")?;

        self.active_pipeline = Some(VulkanGraphicsPipeline {
            vertex_shader: vs,
            fragment_shader: fs,
            depth_test_enabled: depth_test,
            viewport_width: viewport_w,
            viewport_height: viewport_h,
        });

        self.spirv_modules_loaded += 2;
        self.pipelines_created += 1;
        Ok(())
    }

    /// Records a fixture draw; submits no hardware commands.
    pub fn draw_arrays(&mut self, _mmu: &Memory, _vertex_buffer_gpu_addr: u64, _vertex_count: u32) -> Result<(), &'static str> {
        if self.active_pipeline.is_none() {
            return Err("No active Vulkan pipeline bound for draw_arrays");
        }

        self.draw_calls_submitted += 1;
        Ok(())
    }

    /// Fixture has no swapchain; cannot present a hardware frame.
    pub fn present_frame(&mut self) {
        // Kept for source compatibility. Only an actual backend may claim presentation.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::shader::{SassOp, SassShaderDecompiler};

    #[test]
    fn test_vulkan_backend_spirv_module_and_pipeline_creation() {
        // Compile Ampere shader into SPIR-V 1.5 words
        let program = vec![
            SassOp::FMovImm { rd: 0, imm: 1.0 }.encode(),
            SassOp::FMovImm { rd: 1, imm: 0.5 }.encode(),
            SassOp::Exit.encode(),
        ];
        let decompiled = SassShaderDecompiler::decompile(&program);

        let mut backend = VulkanGpuBackend::new();
        let res = backend.create_graphics_pipeline(
            &decompiled.spirv_words,
            &decompiled.spirv_words,
            1280,
            720,
            true,
        );

        assert!(res.is_ok());
        assert_eq!(backend.pipelines_created, 1);
        assert_eq!(backend.spirv_modules_loaded, 2);

        let mut mmu = Memory::new();
        assert!(backend.draw_arrays(&mut mmu, 0x8000_0000, 3).is_ok());
        assert_eq!(backend.draw_calls_submitted, 1);
        backend.present_frame();
        assert_eq!(backend.hardware_frames_presented, 0);
        assert_eq!(backend.device_info.device_type, "None");
    }
}
