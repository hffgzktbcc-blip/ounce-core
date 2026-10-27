//! Shader Program Header (SPH) reader.
//!
//! Source: NVIDIA open-gpu-doc, `Shader-Program-Header/Shader-Program-Header.html`
//! (public, Maxwell era, 20 words). Field positions below are taken from its tables:
//! CommonWord0..4 (words 0..4), then the VTG layout (IMAP bitmap from bit 160, OMAP
//! bitmap from bit 400) or the PS layout (2-bit `PixelImap` per generic component from
//! bit 192, `OmapTarget[8]` from bit 576, `OmapSampleMask`/`OmapDepth` at bits 608/609).
//! We assume those fields keep their meaning on Ampere; that is not confirmed by any
//! NVIDIA document. Turing and later carry a longer header (see
//! [`AMPERE_SPH_SIZE`]); the extra words are ignored here.

use super::XlateError;

/// Words described by the public spec.
pub const SPH_WORDS: usize = 20;
/// Bytes between the header start and the first instruction on Ampere.
/// UNVERIFIED: recalled from Mesa NVK/NAK (a 32-word header for Turing and later), not
/// fetched; the public spec documents only the first 0x50 bytes. Callers can override it
/// via [`super::translate_with_header`].
pub const AMPERE_SPH_SIZE: usize = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SphType {
    /// Vertex / tessellation / geometry (spec value 1).
    Vtg,
    /// Pixel (spec value 2).
    Ps,
}

/// `PixelImap` enum (spec table 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelImap {
    Unused,
    Constant,
    Perspective,
    ScreenLinear,
}

#[derive(Debug, Clone)]
pub struct Sph {
    pub sph_type: SphType,
    pub version: u8,
    /// Spec `ShaderType`: 1 vertex, 2 tess init, 3 tess, 4 geometry, 5 pixel.
    pub shader_type: u8,
    pub words: [u32; SPH_WORDS],
}

impl Sph {
    pub fn parse(bytes: &[u8]) -> Result<Self, XlateError> {
        if bytes.len() < SPH_WORDS * 4 {
            return Err(XlateError::Truncated { need: SPH_WORDS * 4, have: bytes.len() });
        }
        let mut words = [0u32; SPH_WORDS];
        for (i, w) in words.iter_mut().enumerate() {
            *w = u32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]);
        }
        let sph_type = match words[0] & 0x1f {
            1 => SphType::Vtg,
            2 => SphType::Ps,
            t => return Err(XlateError::BadHeader(format!("SphType {t}"))),
        };
        Ok(Sph {
            sph_type,
            version: ((words[0] >> 5) & 0x1f) as u8,
            shader_type: ((words[0] >> 10) & 0xf) as u8,
            words,
        })
    }

    fn field(&self, bit: usize, width: usize) -> u32 {
        let mut v = 0u32;
        for i in 0..width {
            let b = bit + i;
            v |= ((self.words[b / 32] >> (b % 32)) & 1) << i;
        }
        v
    }

    /// VTG: is the attribute word at byte address `addr` read? (IMAP bitmap)
    pub fn vtg_imap(&self, addr: u16) -> bool {
        let b = 160 + addr as usize / 4;
        b < 400 && self.field(b, 1) != 0
    }

    /// VTG: is the attribute word at byte address `addr` written? (OMAP bitmap)
    pub fn vtg_omap(&self, addr: u16) -> bool {
        let b = 400 + addr as usize / 4;
        b < SPH_WORDS * 32 && self.field(b, 1) != 0
    }

    /// PS: interpolation of generic input `loc`, component `comp`.
    pub fn ps_imap_generic(&self, loc: u32, comp: u32) -> PixelImap {
        match self.field(192 + ((loc * 4 + comp) * 2) as usize, 2) {
            0 => PixelImap::Unused,
            1 => PixelImap::Constant,
            2 => PixelImap::Perspective,
            _ => PixelImap::ScreenLinear,
        }
    }

    /// PS: RGBA write mask of colour target `t` (bit 0 red .. bit 3 alpha).
    pub fn ps_omap_target(&self, t: u32) -> u8 {
        self.field(576 + 4 * t as usize, 4) as u8
    }

    pub fn ps_omap_depth(&self) -> bool {
        self.field(609, 1) != 0
    }

    pub fn ps_omap_sample_mask(&self) -> bool {
        self.field(608, 1) != 0
    }
}

/// Builds headers for synthetic tests (same field positions as [`Sph`]).
#[derive(Debug, Clone)]
pub struct SphBuilder {
    words: [u32; SPH_WORDS],
}

impl SphBuilder {
    pub fn vertex() -> Self {
        let mut s = SphBuilder { words: [0; SPH_WORDS] };
        s.words[0] = 1 | (3 << 5) | (1 << 10);
        s
    }

    pub fn pixel() -> Self {
        let mut s = SphBuilder { words: [0; SPH_WORDS] };
        s.words[0] = 2 | (3 << 5) | (5 << 10);
        s
    }

    fn set(&mut self, bit: usize, width: usize, val: u32) {
        for i in 0..width {
            let b = bit + i;
            let m = 1u32 << (b % 32);
            if (val >> i) & 1 != 0 {
                self.words[b / 32] |= m;
            } else {
                self.words[b / 32] &= !m;
            }
        }
    }

    pub fn imap(mut self, addr: u16) -> Self {
        self.set(160 + addr as usize / 4, 1, 1);
        self
    }

    pub fn omap(mut self, addr: u16) -> Self {
        self.set(400 + addr as usize / 4, 1, 1);
        self
    }

    pub fn ps_imap(mut self, loc: u32, comp: u32, mode: PixelImap) -> Self {
        let v = match mode {
            PixelImap::Unused => 0,
            PixelImap::Constant => 1,
            PixelImap::Perspective => 2,
            PixelImap::ScreenLinear => 3,
        };
        self.set(192 + ((loc * 4 + comp) * 2) as usize, 2, v);
        self
    }

    pub fn ps_target(mut self, t: u32, mask: u8) -> Self {
        self.set(576 + 4 * t as usize, 4, mask as u32);
        self
    }

    /// Header bytes padded to `size` (>= 0x50).
    pub fn bytes(&self, size: usize) -> Vec<u8> {
        let mut out: Vec<u8> = self.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        out.resize(size.max(out.len()), 0);
        out
    }
}
