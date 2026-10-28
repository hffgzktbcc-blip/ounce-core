//! Ampere (SM 8.6 / 8.7, Tegra T239 GA10B) shader translator: SASS -> SSA IR -> WGSL.
//!
//! Step 1 covers a passthrough vertex shader and a solid-colour (or varying-colour)
//! fragment shader: `ALD`/`AST`, `IPA`, `MOV`, `FADD`/`FMUL`/`FFMA`, `MUFU`, `LDC` and
//! constant-buffer operands, `NOP`, `EXIT`, and the Shader Program Header, plus 2D texture
//! reads with a bound handle (`TEX`/`TLD`, and Maxwell `TEXS`/`TLDS`). No control flow or
//! integer ops yet; anything else is rejected with an error rather than guessed.
//!
//! The Volta+ SASS encoding is not public. Field positions come from community sources
//! cited in [`decode`] and [`sph`]; entries marked UNVERIFIED were recalled rather than
//! fetched and must be checked against a real compiled shader. This is not a
//! hardware-accurate decoder.
//!
//! [`maxwell`] is a second front end for the 64-bit Maxwell encoding that deko3d's `uam`
//! emits for the `MAXWELL_B` class; it feeds the same IR. [`translate_for_class`] picks
//! the front end from the 3D class a draw was issued on.
//!
//! Input: the shader bytes starting at its header (program region base + per-stage start
//! offset), as handed over by the Ampere 3D class decoder in `src/gpu/ampere/`.

#![allow(dead_code)]

pub mod decode;
pub mod eval;
pub mod ir;
pub mod maxwell;
pub mod sph;
pub mod wgsl;

#[cfg(test)]
mod maxwell_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tex_tests;

pub use maxwell::MAXWELL_SPH_SIZE;
pub use sph::{Sph, SphType, AMPERE_SPH_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
}

impl ShaderStage {
    /// Maps the Ampere 3D decoder's per-pipeline type (`DrawRecord.shaders[i].kind`);
    /// `None` for stages step 1 does not translate. Read the shader bytes at that
    /// stage's `program_va`, which (our reading of NVK) points at the header.
    pub fn from_ampere(kind: crate::gpu::ampere::engine::ShaderType) -> Option<Self> {
        use crate::gpu::ampere::engine::ShaderType as T;
        match kind {
            T::Vertex | T::VertexCullBeforeFetch => Some(ShaderStage::Vertex),
            T::Pixel => Some(ShaderStage::Fragment),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Perspective,
    Linear,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Varying {
    pub location: u32,
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstantBuffer {
    pub group: u32,
    /// Hardware constant buffer bank.
    pub binding: u32,
    /// Bytes the shader can address (a multiple of 16).
    pub size_bytes: u32,
}

/// What the wgpu side needs to build vertex layouts and bind groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShaderInterface {
    /// Vertex stage: `@location`s of `vec4<f32>` vertex inputs.
    pub vertex_inputs: Vec<u32>,
    /// Vertex outputs or fragment inputs.
    pub varyings: Vec<Varying>,
    /// Fragment stage: colour target `@location`s (`vec4<f32>`).
    pub color_targets: Vec<u32>,
    pub constant_buffers: Vec<ConstantBuffer>,
    pub textures: Vec<TextureBinding>,
    pub writes_position: bool,
}

/// One texture the shader reads; the host resolves `handle` to a TIC/TSC pair at draw time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureBinding {
    pub group: u32,
    /// `texture_2d<f32>` binding.
    pub binding: u32,
    /// `sampler` binding, when the shader samples (not only fetches).
    pub sampler_binding: Option<u32>,
    pub handle: decode::TexHandle,
}

#[derive(Debug, Clone)]
pub struct TranslatedShader {
    pub stage: ShaderStage,
    pub wgsl: String,
    pub interface: ShaderInterface,
    /// Decoded instructions, one per line.
    pub listing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum XlateError {
    #[error("shader truncated: need {need} bytes, have {have}")]
    Truncated { need: usize, have: usize },
    #[error("bad shader program header: {0}")]
    BadHeader(String),
    #[error("header is for a different stage than {0:?}")]
    StageMismatch(ShaderStage),
    #[error("no EXIT before the end of the shader")]
    MissingExit,
    #[error("code offset {at:#x}: cannot decode {word:#034x}: {err}")]
    Decode { at: usize, word: u128, err: decode::DecodeError },
    #[error("code offset {at:#x}: unsupported: {what}")]
    Unsupported { at: usize, what: &'static str },
}

/// Translates one shader whose header starts at `bytes[0]`, assuming the Ampere header
/// size [`AMPERE_SPH_SIZE`].
pub fn translate(stage: ShaderStage, bytes: &[u8]) -> Result<TranslatedShader, XlateError> {
    translate_with_header(stage, bytes, AMPERE_SPH_SIZE)
}

/// As [`translate`], with an explicit header size (bytes from header start to code).
pub fn translate_with_header(stage: ShaderStage, bytes: &[u8], header_size: usize) -> Result<TranslatedShader, XlateError> {
    let (sph, code) = split_header(stage, bytes, header_size)?;
    finish(stage, ir::lower(stage, code)?, &sph)
}

/// Translates one Maxwell (64-bit SASS) shader whose 0x50-byte header starts at `bytes[0]`.
pub fn translate_maxwell(stage: ShaderStage, bytes: &[u8]) -> Result<TranslatedShader, XlateError> {
    let (sph, code) = split_header(stage, bytes, MAXWELL_SPH_SIZE)?;
    finish(stage, maxwell::lower(stage, code)?, &sph)
}

/// Picks the front end from the 3D class the draw was issued on (`DrawRecord.class`):
/// Maxwell SASS for `MAXWELL_B`, Ampere SASS otherwise.
pub fn translate_for_class(class: u32, stage: ShaderStage, bytes: &[u8]) -> Result<TranslatedShader, XlateError> {
    if class == crate::gpu::ampere::methods::MAXWELL_B { translate_maxwell(stage, bytes) } else { translate(stage, bytes) }
}

fn finish(stage: ShaderStage, f: ir::Function, sph: &Sph) -> Result<TranslatedShader, XlateError> {
    let (wgsl, interface) = wgsl::emit(&f, sph)?;
    Ok(TranslatedShader { stage, wgsl, interface, listing: f.listing })
}

fn split_header(stage: ShaderStage, bytes: &[u8], header_size: usize) -> Result<(Sph, &[u8]), XlateError> {
    let sph = Sph::parse(bytes)?;
    let ok = match stage {
        ShaderStage::Vertex => sph.sph_type == SphType::Vtg && sph.shader_type == 1,
        ShaderStage::Fragment => sph.sph_type == SphType::Ps && sph.shader_type == 5,
    };
    if !ok {
        return Err(XlateError::StageMismatch(stage));
    }
    if bytes.len() < header_size {
        return Err(XlateError::Truncated { need: header_size, have: bytes.len() });
    }
    Ok((sph, &bytes[header_size..]))
}
