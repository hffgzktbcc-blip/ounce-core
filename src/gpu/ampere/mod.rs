//! AMPERE_B (`0xC797`) 3D class: method decoder and state model for T239's GA10B. MAXWELL_B
//! (`0xB197`) goes through the same decoder, since the two classes share almost all of their method
//! offsets (see [`methods`]).
//!
//! Step 1 of M5 ("clear + one triangle"): pushbuffer segments decode into method writes
//! ([`pushbuf`]), the engine keeps the class state and emits clear, draw and semaphore records
//! ([`engine`]), and bound render targets are described as raw surfaces plus their layout
//! ([`surface`]) for the present side to detile. Offsets come only from NVIDIA's public
//! open-gpu-doc headers; see [`methods`].
//!
//! Step 3: clears and draws render into guest memory as raw surfaces ([`render`], software path
//! running the `gpu::sass` IR), texture headers and samplers decode from their pools ([`texture`]),
//! and host semaphores can block the channel ([`engine`]).
//!
//! Not wired into a channel yet: the `nvgpu.rs` routing hook (Track B) calls [`Ampere3d::submit`]
//! for subchannels bound to AMPERE_B or MAXWELL_B. Until then only the tests use it.
#![allow(dead_code)]

mod blend;
mod clip;
mod depth;
pub mod engine;
pub mod methods;
pub mod mme;
pub mod pushbuf;
pub mod render;
mod shade;
pub mod surface;
pub mod texture;

#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;

pub use engine::{
    Ampere3d, ClearRecord, ConstantBufferBinding, Diagnostic, DrawRecord, Event, GpuMemoryWriter, HostRecord,
    HostSemaphoreOp, HostSemaphoreRecord, Scissor, SemaphoreOp, SemaphoreRecord, SemaphoreWait, ShaderStage, ShaderType,
    Topology, VertexAttribute, VertexStream, Viewport,
};
pub use render::RenderStats;
pub use texture::{Sampler, TextureHeader};
pub use methods::{AMPERE_B, MAXWELL_B};
pub use pushbuf::{MethodCall, PushbufError};
pub use surface::{ColorFormat, ColorTarget, DepthFormat, DepthTarget, SurfaceLayout};
