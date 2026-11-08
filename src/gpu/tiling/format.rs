//! Element sizes for the surface formats the tiling code has to place.
//!
//! Tiling only cares about how many bytes one element occupies and how many texels an
//! element covers. For plain formats an element is one texel; for BCn formats it is one
//! 4x4 texel block (8 bytes for BC1/BC4, 16 bytes for the rest), as defined by the
//! public BCn format descriptions (Khronos Data Format Specification, section "BC1" to
//! "BC7").

/// A surface format, reduced to what the tiler needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    R8Unorm,
    R8G8Unorm,
    R5G6B5Unorm,
    R16Float,
    R8G8B8A8Unorm,
    R8G8B8A8Srgb,
    B8G8R8A8Unorm,
    B8G8R8A8Srgb,
    A2B10G10R10Unorm,
    R11G11B10Float,
    R16G16Float,
    R32Float,
    R16G16B16A16Float,
    R32G32Float,
    R32G32B32A32Float,
    // Depth and stencil.
    S8Uint,
    Z16Unorm,
    Z24UnormS8Uint,
    Z32Float,
    Z32FloatS8X24Uint,
    // Block compressed, 4x4 texels per element.
    Bc1RgbaUnorm,
    Bc2Unorm,
    Bc3Unorm,
    Bc4Unorm,
    Bc5Unorm,
    Bc6hUfloat,
    Bc6hSfloat,
    Bc7Unorm,
}

impl Format {
    /// Every format, for exhaustive tests.
    pub const ALL: [Format; 28] = [
        Format::R8Unorm,
        Format::R8G8Unorm,
        Format::R5G6B5Unorm,
        Format::R16Float,
        Format::R8G8B8A8Unorm,
        Format::R8G8B8A8Srgb,
        Format::B8G8R8A8Unorm,
        Format::B8G8R8A8Srgb,
        Format::A2B10G10R10Unorm,
        Format::R11G11B10Float,
        Format::R16G16Float,
        Format::R32Float,
        Format::R16G16B16A16Float,
        Format::R32G32Float,
        Format::R32G32B32A32Float,
        Format::S8Uint,
        Format::Z16Unorm,
        Format::Z24UnormS8Uint,
        Format::Z32Float,
        Format::Z32FloatS8X24Uint,
        Format::Bc1RgbaUnorm,
        Format::Bc2Unorm,
        Format::Bc3Unorm,
        Format::Bc4Unorm,
        Format::Bc5Unorm,
        Format::Bc6hUfloat,
        Format::Bc6hSfloat,
        Format::Bc7Unorm,
    ];

    /// Bytes per element (per texel, or per 4x4 block for BCn).
    pub const fn bytes_per_element(self) -> u32 {
        match self {
            Format::R8Unorm | Format::S8Uint => 1,
            Format::R8G8Unorm | Format::R5G6B5Unorm | Format::R16Float | Format::Z16Unorm => 2,
            Format::R8G8B8A8Unorm
            | Format::R8G8B8A8Srgb
            | Format::B8G8R8A8Unorm
            | Format::B8G8R8A8Srgb
            | Format::A2B10G10R10Unorm
            | Format::R11G11B10Float
            | Format::R16G16Float
            | Format::R32Float
            | Format::Z24UnormS8Uint
            | Format::Z32Float => 4,
            Format::R16G16B16A16Float
            | Format::R32G32Float
            | Format::Z32FloatS8X24Uint
            | Format::Bc1RgbaUnorm
            | Format::Bc4Unorm => 8,
            Format::R32G32B32A32Float
            | Format::Bc2Unorm
            | Format::Bc3Unorm
            | Format::Bc5Unorm
            | Format::Bc6hUfloat
            | Format::Bc6hSfloat
            | Format::Bc7Unorm => 16,
        }
    }

    pub const fn is_compressed(self) -> bool {
        matches!(
            self,
            Format::Bc1RgbaUnorm
                | Format::Bc2Unorm
                | Format::Bc3Unorm
                | Format::Bc4Unorm
                | Format::Bc5Unorm
                | Format::Bc6hUfloat
                | Format::Bc6hSfloat
                | Format::Bc7Unorm
        )
    }

    pub const fn is_depth_stencil(self) -> bool {
        matches!(
            self,
            Format::S8Uint
                | Format::Z16Unorm
                | Format::Z24UnormS8Uint
                | Format::Z32Float
                | Format::Z32FloatS8X24Uint
        )
    }

    /// Texels covered by one element, horizontally and vertically.
    pub const fn element_extent(self) -> (u32, u32) {
        if self.is_compressed() {
            (4, 4)
        } else {
            (1, 1)
        }
    }
}
