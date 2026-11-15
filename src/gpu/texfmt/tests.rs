use super::vectors::SETS;
use super::*;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn desc(components: u32, ty: NumType) -> FormatDesc {
    FormatDesc { components, types: [Some(ty); 4], srgb: false }
}

/// Every reference vector, decoded texel by texel, within one 8-bit step (BC6H: only texels the
/// reference did not saturate, within two steps of its half-to-8-bit conversion).
#[test]
fn compressed_formats_match_reference_vectors() {
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    for &(name, code, bw, bh, snorm, vecs) in SETS {
        let d = desc(code, if snorm { NumType::Snorm } else { NumType::Unorm });
        assert_eq!(element(code).map(|e| (e.block_w, e.block_h)), Some((bw, bh)), "{name}");
        for (k, &(block, want)) in vecs.iter().enumerate() {
            let (block, want) = (hex(block), hex(want));
            let n = (bw * bh) as usize;
            if name.starts_with("ASTC") && (0..n).all(|i| decode_texel(&d, &block, i as u32 % bw, i as u32 / bw) == Some([1.0, 0.0, 1.0, 1.0])) {
                skipped.push(format!("{name} #{k}"));
                continue;
            }
            for i in 0..(bw * bh) as usize {
                let c = decode_texel(&d, &block, i as u32 % bw, i as u32 / bw).expect(name);
                for ch in 0..4 {
                    let w = want[4 * i + ch] as i32;
                    let got = if snorm {
                        // The reference maps signed [-1, 1] to [0, 255].
                        ((c[ch].clamp(-1.0, 1.0) + 1.0) * 127.5).round() as i32
                    } else {
                        (c[ch].clamp(0.0, 1.0) * 255.0).round() as i32
                    };
                    let bc6 = name.starts_with("BC6H");
                    // The reference decodes BC1 as opaque; the 3-colour mode's fourth entry is
                    // transparent black, as the BC1 RGBA format defines it.
                    if (name == "BC1" || name == "ETC2_RGB_PTA") && c == [0.0; 4] && want[4 * i + 3] == 0 || (name == "BC1" && ch == 3 && c == [0.0; 4]) {
                        continue;
                    }
                    // Single-channel signed EAC: the reference leaves G and B at 0.
                    if snorm && ch > 0 {
                        continue;
                    }
                    if bc6 && (w == 255 || c[ch] > 1.0) {
                        continue;
                    }
                    let tol = if bc6 { 2 } else { 1 };
                    if (got - w).abs() > tol {
                        failures.push(format!("{name} #{k} texel {i} ch {ch}: got {got} want {w}"));
                    }
                }
            }
        }
    }
    let mut per: std::collections::BTreeMap<&str, usize> = Default::default();
    for f in &failures {
        *per.entry(f.split(' ').next().unwrap()).or_default() += 1;
    }
    // Exactly the blocks astcenc rejects decode to the error colour.
    assert!(skipped.iter().all(|s| s.contains("_ERR")), "rejected legal ASTC blocks: {skipped:?}");
    assert_eq!(skipped.len(), SETS.iter().filter(|s| s.0.ends_with("_ERR")).map(|s| s.5.len()).sum::<usize>());
    assert!(failures.is_empty(), "{per:?} {} mismatches, first: {:#?}", failures.len(), &failures[..failures.len().min(20)]);
}


fn texel(components: u32, ty: NumType, bytes: &[u8]) -> [f32; 4] {
    decode_texel(&desc(components, ty), bytes, 0, 0).unwrap()
}

#[test]
fn packed_formats_place_r_in_the_low_bits() {
    // A1B5G5R5: R 4:0, G 9:5, B 14:10, A 15.
    let v: u16 = 31 | (0 << 5) | (16 << 10) | (1 << 15);
    assert_eq!(texel(0x14, NumType::Unorm, &v.to_le_bytes()), [1.0, 0.0, 16.0 / 31.0, 1.0]);
    // A4B4G4R4.
    assert_eq!(texel(0x12, NumType::Unorm, &[0x0f, 0xf0]), [1.0, 0.0, 0.0, 1.0]);
    // G4R4 and the missing channels' defaults.
    assert_eq!(texel(0x1e, NumType::Unorm, &[0xf0]), [0.0, 1.0, 0.0, 1.0]);
    // R8 snorm: -128 clamps to -1.
    assert_eq!(texel(0x1d, NumType::Snorm, &[0x80])[0], -1.0);
    assert_eq!(texel(0x1d, NumType::Snorm, &[0x81])[0], -1.0);
    // R16G16 float.
    let h = [0x00, 0x3c, 0x00, 0xc0]; // 1.0, -2.0
    assert_eq!(texel(0x0c, NumType::Float, &h)[..2], [1.0, -2.0]);
}

#[test]
fn integer_formats_return_integer_bits() {
    let c = texel(0x08, NumType::Uint, &[7, 200, 0, 255]);
    assert_eq!(c.map(f32::to_bits), [7, 200, 0, 255]);
    let c = texel(0x1b, NumType::Sint, &(-5i16).to_le_bytes());
    assert_eq!(c[0].to_bits() as i32, -5);
    // Missing alpha is integer one.
    assert_eq!(c[3].to_bits(), 1);
}

#[test]
fn shared_exponent_and_small_floats() {
    // E5B9G9R9: R = 256 * 2^(15 - 15 - 9) = 0.5, exponent 15.
    let v: u32 = 256 | (15 << 27);
    assert_eq!(texel(0x20, NumType::Float, &v.to_le_bytes())[..3], [0.5, 0.0, 0.0]);
    // BF10GF11RF11: R = 1.0 (exp 15, mantissa 0), B = 2.0 (exp 16).
    let v: u32 = (15 << 6) | ((16 << 5) << 22);
    assert_eq!(texel(0x21, NumType::Float, &v.to_le_bytes())[..3], [1.0, 0.0, 2.0]);
}

#[test]
fn depth_formats() {
    // S8Z24: Z in 23:0, S in 31:24.
    let v: u32 = 0xff_ffff | (0x5a << 24);
    let c = texel(0x2b, NumType::Unorm, &v.to_le_bytes());
    assert_eq!((c[0], c[1].to_bits()), (1.0, 0x5a));
    // Z24S8: S in 7:0.
    let v: u32 = 0x5a;
    let c = texel(0x29, NumType::Unorm, &v.to_le_bytes());
    assert_eq!((c[0], c[1].to_bits()), (0.0, 0x5a));
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&0.25f32.to_le_bytes());
    b[4] = 9;
    let c = texel(0x30, NumType::Float, &b);
    assert_eq!((c[0], c[1].to_bits()), (0.25, 9));
    assert_eq!(texel(0x3a, NumType::Unorm, &[0xff, 0xff])[0], 1.0);
}

#[test]
fn srgb_applies_to_colour_channels_only() {
    let d = FormatDesc { components: 0x08, types: [Some(NumType::Unorm); 4], srgb: true };
    let c = decode_texel(&d, &[188, 0, 255, 188], 0, 0).unwrap();
    assert!((c[0] - 0.5029).abs() < 1e-3, "{c:?}");
    assert_eq!((c[1], c[2]), (0.0, 1.0));
    assert_eq!(c[3], 188.0 / 255.0);
}

#[test]
fn element_sizes() {
    assert_eq!(element(DXT1), Some(Element { block_w: 4, block_h: 4, bytes: 8 }));
    assert_eq!(element(BC7U).map(|e| e.bytes), Some(16));
    assert_eq!(element(0x54), Some(Element { block_w: 12, block_h: 10, bytes: 16 }));
    assert_eq!(element(0x30).map(|e| e.bytes), Some(8));
    assert_eq!(element(0x01).map(|e| e.bytes), Some(16));
    assert_eq!(element(0x02), None, "R32_G32_B32 has no tiling element size");
    assert_eq!(element(0x22), None, "4:2:2 formats are not decoded");
    // Texels outside the block and short input are rejected.
    assert_eq!(decode_texel(&desc(DXT1, NumType::Unorm), &[0; 8], 4, 0), None);
    assert_eq!(decode_texel(&desc(DXT1, NumType::Unorm), &[0; 7], 0, 0), None);
}

/// Flat guest memory for the header-level test.
struct Flat {
    base: u64,
    bytes: Vec<u8>,
}

impl crate::gpu::ampere::GpuMemoryWriter for Flat {
    fn write(&mut self, _: u64, _: &[u8]) {}
    fn read(&mut self, va: u64, buf: &mut [u8]) -> bool {
        let at = (va - self.base) as usize;
        match self.bytes.get(at..at + buf.len()) {
            Some(src) => {
                buf.copy_from_slice(src);
                true
            }
            None => false,
        }
    }
}

/// A 6x6 BC1 texture (2x2 blocks, the last row and column partial) read through a block-linear
/// texture header: each texel resolves to its block and decodes there.
#[test]
fn texture_header_fetches_compressed_blocks() {
    use crate::gpu::ampere::TextureHeader;
    use crate::gpu::tiling::{self, Format, Layout, Surface};
    const VA: u64 = 0x10_0000;
    // Solid blocks: c0 = c1, all selectors 0. Pure red, green, blue, white in RGB565.
    let colours: [u16; 4] = [0xf800, 0x07e0, 0x001f, 0xffff];
    let linear: Vec<u8> = colours.iter().flat_map(|c| [c.to_le_bytes(), c.to_le_bytes(), [0, 0], [0, 0]].concat()).collect();
    let surf = Surface::new_2d(6, 6, Format::Bc1RgbaUnorm, Layout::BlockLinear { height_log2: 0, depth_log2: 0 });
    let mem_bytes = tiling::tile(&surf, &linear).unwrap();
    let mut w = [0u32; 8];
    let unorm = 2;
    w[0] = DXT1 | (unorm << 7) | (unorm << 10) | (unorm << 13) | (unorm << 16) | (2 << 19) | (3 << 22) | (4 << 25) | (5 << 28);
    w[1] = VA as u32;
    w[2] = 3 << 21; // BLOCKLINEAR
    w[4] = 5 | (1 << 23); // WIDTH_MINUS_ONE, TWO_D
    w[5] = 5 | (1 << 31); // HEIGHT_MINUS_ONE, NORMALIZED_COORDS
    let bytes: Vec<u8> = w.iter().flat_map(|v| v.to_le_bytes()).collect();
    let header = TextureHeader::parse(bytes.as_slice().try_into().unwrap());
    let mut mem = Flat { base: VA, bytes: mem_bytes };
    assert_eq!(header.fetch(1, 1, &mut mem), Some([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(header.fetch(5, 0, &mut mem), Some([0.0, 1.0, 0.0, 1.0]));
    assert_eq!(header.fetch(0, 4, &mut mem), Some([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(header.fetch(5, 5, &mut mem), Some([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(header.fetch(6, 0, &mut mem), None, "past the texture's width");
    assert_eq!(header.bytes_per_texel(), None, "compressed formats have no per-texel size");
}
