#![allow(unused_imports)]

pub mod ampere;
pub mod display;
pub mod upscaler;
pub mod memory_pool;
pub mod nvgpu;
pub mod nvn;
pub mod present;
pub mod sass;
pub mod shader;
pub mod tensor;
pub mod tiling;
pub mod vulkan;

pub use display::{DisplayConfig, DisplayServer, PresentationMode, DEFAULT_FRAMEBUFFER_ADDR, SCREEN_HEIGHT, SCREEN_WIDTH};
pub use present::{BackendKind, Frame, HostKey, PresentBackend, PresentError, PresentStatus, ScaleMode};
pub use upscaler::UpscaleEngine;
pub use memory_pool::{NvnBuffer, NvnBufferType, NvnMemoryPool, NvnMemoryPoolManager};
pub use nvgpu::{
    Gpu3DDrawOp, Gpu3DState, Gpu3DTextureData, Gpu3DVertex, GpuCommand, GpuPrimitiveTopology,
    NvgpuChannel, METHOD_BIND_SHADER_PROGRAM, METHOD_BIND_TEXTURE_2D, METHOD_DISPATCH_UPSCALE_2X,
    METHOD_DRAW_TEXTURED_ARRAYS, METHOD_SET_DEPTH_TEST_ENABLE,
};
pub use nvn::{NvnCmd, NvnCommandBuffer, NvnDriver};
pub use shader::{DecompiledShader, SassOp, SassShaderDecompiler};
pub use tensor::{HmmaShape, TensorCoreUnit, WarpFragments};
pub use vulkan::{VulkanDeviceInfo, VulkanGpuBackend, VulkanShaderModule};
