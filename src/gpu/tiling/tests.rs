use super::*;
use std::collections::HashSet;

const BC: [Format; 8] = [
    Format::Bc1RgbaUnorm,
    Format::Bc2Unorm,
    Format::Bc3Unorm,
    Format::Bc4Unorm,
    Format::Bc5Unorm,
    Format::Bc6hUfloat,
    Format::Bc6hSfloat,
    Format::Bc7Unorm,
];

/// Deterministic, non-repeating-looking bytes (LCG), so misplaced bytes show up.
fn pattern(len: usize, seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

fn bl(width: u32, height: u32, depth: u32, format: Format, h: u8, d: u8) -> Surface {
    Surface { width, height, depth, format, layout: Layout::BlockLinear { height_log2: h, depth_log2: d } }
}

/// Independent reference, written straight from the TRM description with the GOB as a
/// bit interleave: offset bits = x[3:0] | y[0]<<4 | x[4]<<5 | y[2:1]<<6 | x[5]<<8.
fn reference_offset(s: &Surface, x: u32, y: u32, z: u32) -> usize {
    let Layout::BlockLinear { height_log2: h, depth_log2: d } = s.layout else { unreachable!() };
    let (_, rows, _) = s.extent_elements();
    let gobs_wide = (s.row_bytes() + 63) / 64;
    let block_rows = 8u32 << h;
    let blocks_high = (rows + block_rows - 1) / block_rows;
    let block_d = 1u32 << d;
    let block = (z / block_d) * blocks_high * gobs_wide + (y / block_rows) * gobs_wide + x / 64;
    let gob = (z % block_d) * (1 << h) + (y % block_rows) / 8;
    let (xi, yi) = (x % 64, y % 8);
    let in_gob = (xi & 0xF) | (yi & 1) << 4 | ((xi >> 4) & 1) << 5 | ((yi >> 1) & 3) << 6 | ((xi >> 5) & 1) << 8;
    block as usize * ((512usize) << (h + d)) + gob as usize * 512 + in_gob as usize
}

/// Per-byte reference detile: for each packed byte, read it from its reference offset.
fn reference_detile(s: &Surface, tiled: &[u8]) -> Vec<u8> {
    let (_, rows, depth) = s.extent_elements();
    let rb = s.row_bytes();
    let mut out = Vec::with_capacity((rb * rows * depth) as usize);
    for z in 0..depth {
        for y in 0..rows {
            for x in 0..rb {
                out.push(tiled[reference_offset(s, x, y, z)]);
            }
        }
    }
    out
}

#[test]
fn gob_known_vectors() {
    // (x byte, row) -> offset inside the 512-byte GOB, from the TRM formula.
    let cases = [
        ((0, 0), 0),
        ((15, 0), 15),
        ((16, 0), 32),
        ((31, 0), 47),
        ((0, 1), 16),
        ((31, 1), 63),
        ((0, 2), 64),
        ((0, 7), 208),
        ((32, 0), 256),
        ((48, 5), 432),
        ((63, 7), 511),
    ];
    for ((x, y), want) in cases {
        assert_eq!(gob_offset(x, y), want, "gob_offset({x}, {y})");
    }
}

#[test]
fn gob_is_a_bijection_and_matches_bit_interleave() {
    let mut seen = [false; 512];
    for y in 0..8 {
        for x in 0..64 {
            let o = gob_offset(x, y) as usize;
            let bits = (x & 0xF) | (y & 1) << 4 | ((x >> 4) & 1) << 5 | ((y >> 1) & 3) << 6 | ((x >> 5) & 1) << 8;
            assert_eq!(o, bits as usize);
            assert!(!seen[o], "offset {o} hit twice");
            seen[o] = true;
        }
    }
    assert!(seen.iter().all(|&s| s));
}

#[test]
fn gob_rows_are_16_byte_runs() {
    for y in 0..8 {
        for x in (0..64).step_by(16) {
            for i in 1..16 {
                assert_eq!(gob_offset(x + i, y), gob_offset(x, y) + i);
            }
        }
    }
}

#[test]
fn block_linear_known_vectors() {
    // 256x256 RGBA8, 16 GOBs per block: 1024 bytes/row -> 16 GOBs wide, 128 rows/block.
    let s = bl(256, 256, 1, Format::R8G8B8A8Unorm, 4, 0);
    let g = s.block_linear().unwrap();
    assert_eq!((g.gobs_wide, g.blocks_high, g.blocks_deep), (16, 2, 1));
    assert_eq!(g.block_bytes(), 8192);
    assert_eq!(s.size_bytes().unwrap(), 256 * 256 * 4);
    assert_eq!(s.byte_offset(64, 0, 0), 8192); // next GOB column = next block
    assert_eq!(s.byte_offset(0, 8, 0), 512); // next GOB down, same block
    assert_eq!(s.byte_offset(0, 127, 0), 15 * 512 + gob_offset(0, 7) as usize);
    assert_eq!(s.byte_offset(0, 128, 0), 16 * 8192); // second block row
    assert_eq!(s.byte_offset(4, 1, 0), 20); // pixel (1,1)

    // 1280x720 RGBA8 at block height 16 GOBs (a common swapchain layout).
    let s = bl(1280, 720, 1, Format::R8G8B8A8Unorm, 4, 0);
    let g = s.block_linear().unwrap();
    assert_eq!((g.gobs_wide, g.blocks_high), (80, 6));
    assert_eq!(s.size_bytes().unwrap(), 80 * 6 * 8192);
    assert_eq!(g.stride(), 5120);

    // 3D: 64x16x4 R8, block 1 GOB high, 2 deep.
    let s = bl(64, 16, 4, Format::R8Unorm, 0, 1);
    let g = s.block_linear().unwrap();
    assert_eq!((g.gobs_wide, g.blocks_high, g.blocks_deep, g.block_bytes()), (1, 2, 2, 1024));
    assert_eq!(s.byte_offset(0, 0, 1), 512);
    assert_eq!(s.byte_offset(0, 8, 0), 1024);
    assert_eq!(s.byte_offset(0, 0, 2), 2048);
    assert_eq!(s.byte_offset(0, 8, 3), 3 * 1024 + 512);
}

#[test]
fn bc_formats_address_4x4_blocks() {
    // 64x64 BC1: 16x16 blocks of 8 bytes = 128 bytes/row -> 2 GOBs wide.
    let s = bl(64, 64, 1, Format::Bc1RgbaUnorm, 1, 0);
    assert_eq!(s.extent_elements(), (16, 16, 1));
    assert_eq!(s.row_bytes(), 128);
    let g = s.block_linear().unwrap();
    assert_eq!((g.gobs_wide, g.blocks_high), (2, 1));
    // Odd sizes round up to whole blocks.
    let s = bl(13, 7, 1, Format::Bc7Unorm, 0, 0);
    assert_eq!(s.extent_elements(), (4, 2, 1));
    assert_eq!(s.row_bytes(), 64);
    for f in BC {
        assert_eq!(f.element_extent(), (4, 4));
        let expect = if matches!(f, Format::Bc1RgbaUnorm | Format::Bc4Unorm) { 8 } else { 16 };
        assert_eq!(f.bytes_per_element(), expect, "{f:?}");
    }
}

#[test]
fn offsets_are_unique_and_in_bounds() {
    let sizes = [(1, 1, 1), (3, 5, 1), (17, 9, 1), (100, 37, 1), (65, 70, 3), (33, 20, 5)];
    for &format in &[Format::R8Unorm, Format::R5G6B5Unorm, Format::R8G8B8A8Unorm, Format::R16G16B16A16Float, Format::R32G32B32A32Float, Format::Bc1RgbaUnorm, Format::Bc7Unorm] {
        for &(w, h, d) in &sizes {
            for hl in 0..=3 {
                for dl in 0..=2 {
                    let s = bl(w, h, d, format, hl, dl);
                    let size = s.size_bytes().unwrap();
                    let (_, rows, depth) = s.extent_elements();
                    let mut seen = HashSet::new();
                    for z in 0..depth {
                        for y in 0..rows {
                            for x in 0..s.row_bytes() {
                                let o = s.byte_offset(x, y, z);
                                assert_eq!(o, reference_offset(&s, x, y, z));
                                assert!(o < size);
                                assert!(seen.insert(o), "{format:?} {w}x{h}x{d} h{hl} d{dl} dup at {x},{y},{z}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn round_trip_every_format_and_block_height() {
    let sizes = [(1, 1), (7, 3), (17, 9), (64, 8), (100, 37), (256, 130)];
    let mut seed = 0;
    for format in Format::ALL {
        for (w, h) in sizes {
            for hl in 0..=MAX_BLOCK_LOG2 {
                seed += 1;
                let s = bl(w, h, 1, format, hl, 0);
                let linear = pattern(Surface::packed(w, h, 1, format).size_bytes().unwrap(), seed);
                let tiled = tile(&s, &linear).unwrap();
                assert_eq!(tiled.len(), s.size_bytes().unwrap());
                assert_eq!(reference_detile(&s, &tiled), linear, "{format:?} {w}x{h} h{hl}");
                assert_eq!(detile(&s, &tiled).unwrap(), linear, "{format:?} {w}x{h} h{hl}");
            }
        }
    }
}

#[test]
fn round_trip_3d() {
    let mut seed = 1000;
    for format in [Format::R8Unorm, Format::R8G8B8A8Unorm, Format::R32G32B32A32Float, Format::Bc3Unorm] {
        for (w, h, d) in [(9, 9, 2), (70, 20, 5), (128, 64, 9)] {
            for hl in 0..=2 {
                for dl in 0..=MAX_BLOCK_LOG2 {
                    seed += 1;
                    let s = bl(w, h, d, format, hl, dl);
                    let linear = pattern(Surface::packed(w, h, d, format).size_bytes().unwrap(), seed);
                    let tiled = tile(&s, &linear).unwrap();
                    assert_eq!(reference_detile(&s, &tiled), linear);
                    assert_eq!(detile(&s, &tiled).unwrap(), linear);
                }
            }
        }
    }
}

#[test]
fn detile_matches_reference_on_arbitrary_memory() {
    // Detile garbage (including padding) and compare against per-byte reference reads.
    for (i, format) in Format::ALL.into_iter().enumerate() {
        let s = bl(45, 50, 1, format, 2, 0);
        let tiled = pattern(s.size_bytes().unwrap(), 5000 + i as u32);
        assert_eq!(detile(&s, &tiled).unwrap(), reference_detile(&s, &tiled), "{format:?}");
    }
}

#[test]
fn pitch_with_padding_round_trips_and_keeps_padding() {
    let format = Format::R8G8B8A8Unorm;
    let pitch = Surface::new_2d(30, 11, format, Layout::Pitch { stride: 128 });
    assert_eq!(pitch.size_bytes().unwrap(), 10 * 128 + 120);
    let linear = pattern(30 * 11 * 4, 7);
    let mut padded = vec![0xAB; pitch.size_bytes().unwrap()];
    convert(&Surface::packed(30, 11, 1, format), &linear, &pitch, &mut padded).unwrap();
    for y in 0..11 {
        assert_eq!(&padded[y * 128..y * 128 + 120], &linear[y * 120..y * 120 + 120]);
        if y < 10 {
            assert!(padded[y * 128 + 120..(y + 1) * 128].iter().all(|&b| b == 0xAB));
        }
    }
    assert_eq!(detile(&pitch, &padded).unwrap(), linear);
}

#[test]
fn pitch_to_block_linear_and_back() {
    for format in [Format::B8G8R8A8Unorm, Format::Z24UnormS8Uint, Format::Z32FloatS8X24Uint, Format::Bc5Unorm] {
        let pitch = Surface::new_2d(200, 90, format, Layout::Pitch { stride: 2048 });
        let tiled = bl(200, 90, 1, format, 3, 0);
        let src = pattern(pitch.size_bytes().unwrap(), 42);
        let mut mid = vec![0; tiled.size_bytes().unwrap()];
        convert(&pitch, &src, &tiled, &mut mid).unwrap();
        let mut back = vec![0; pitch.size_bytes().unwrap()];
        convert(&tiled, &mid, &pitch, &mut back).unwrap();
        let rb = pitch.row_bytes() as usize;
        for y in 0..pitch.extent_elements().1 as usize {
            assert_eq!(&back[y * 2048..y * 2048 + rb], &src[y * 2048..y * 2048 + rb]);
        }
    }
}

#[test]
fn block_linear_to_block_linear_changes_block_height() {
    let a = bl(300, 200, 1, Format::R16G16B16A16Float, 4, 0);
    let b = bl(300, 200, 1, Format::R16G16B16A16Float, 1, 0);
    let linear = pattern(Surface::packed(300, 200, 1, a.format).size_bytes().unwrap(), 9);
    let ta = tile(&a, &linear).unwrap();
    let mut tb = vec![0; b.size_bytes().unwrap()];
    convert(&a, &ta, &b, &mut tb).unwrap();
    assert_eq!(detile(&b, &tb).unwrap(), linear);
}

#[test]
fn clamp_shrinks_blocks_for_small_surfaces() {
    assert_eq!(clamp_block_log2(720, 1, 4, 0), (4, 0));
    assert_eq!(clamp_block_log2(64, 1, 4, 0), (3, 0)); // 64 rows fit 8 GOBs
    assert_eq!(clamp_block_log2(65, 1, 4, 0), (4, 0));
    assert_eq!(clamp_block_log2(8, 1, 5, 0), (0, 0));
    assert_eq!(clamp_block_log2(1, 1, 5, 5), (0, 0));
    assert_eq!(clamp_block_log2(9, 3, 5, 5), (1, 2));
    assert_eq!(clamp_block_log2(1000, 1000, 9, 9), (5, 5));
}

#[test]
fn errors() {
    let f = Format::R8G8B8A8Unorm;
    assert_eq!(Surface::new_2d(0, 4, f, Layout::Pitch { stride: 0 }).size_bytes(), Err(TilingError::ZeroExtent));
    assert_eq!(
        Surface::new_2d(10, 4, f, Layout::Pitch { stride: 39 }).size_bytes(),
        Err(TilingError::StrideTooSmall { stride: 39, row_bytes: 40 })
    );
    assert_eq!(
        bl(8, 8, 1, f, 6, 0).size_bytes(),
        Err(TilingError::BlockLog2TooLarge { height_log2: 6, depth_log2: 0 })
    );
    let s = bl(16, 16, 1, f, 1, 0);
    assert_eq!(detile(&s, &[0; 100]), Err(TilingError::BufferTooSmall { needed: 1024, got: 100 }));
    assert_eq!(tile(&s, &[0; 10]), Err(TilingError::BufferTooSmall { needed: 1024, got: 10 }));
    let other = bl(16, 15, 1, f, 1, 0);
    let mut out = vec![0; other.size_bytes().unwrap()];
    assert_eq!(convert(&s, &[0; 1024], &other, &mut out), Err(TilingError::Mismatch));
    let other = bl(16, 16, 1, Format::R16G16B16A16Float, 1, 0);
    assert_eq!(convert(&s, &[0; 1024], &other, &mut out), Err(TilingError::Mismatch));
}

#[test]
fn format_table() {
    assert_eq!(Format::ALL.len(), 28);
    for f in Format::ALL {
        assert!(matches!(f.bytes_per_element(), 1 | 2 | 4 | 8 | 16));
        assert_eq!(RUN_BYTES % f.bytes_per_element(), 0);
    }
    assert!(Format::Z24UnormS8Uint.is_depth_stencil());
    assert!(!Format::R32Float.is_depth_stencil());
    assert_eq!(Format::Z32FloatS8X24Uint.bytes_per_element(), 8);
    assert_eq!(Format::R16G16B16A16Float.bytes_per_element(), 8);
}

#[test]
fn row_bytes_overflow_is_rejected_not_wrapped() {
    let rgba = Format::R8G8B8A8Unorm;
    // Largest width whose RGBA8 row fits u32, then the first that wraps (to 0 and to 4).
    let fits = Surface::new_2d(0x3FFF_FFFF, 1, rgba, Layout::Pitch { stride: 0xFFFF_FFFC });
    assert_eq!(fits.checked_row_bytes(), Some(0xFFFF_FFFC));
    assert_eq!(fits.size_bytes(), Ok(0xFFFF_FFFC));
    for width in [0x4000_0000, 0x4000_0001, u32::MAX] {
        let pitch = Surface::new_2d(width, 2, rgba, Layout::Pitch { stride: 4 });
        assert_eq!(pitch.checked_row_bytes(), None, "width {width:#x}");
        assert_eq!(pitch.size_bytes(), Err(TilingError::Overflow));
        let bl = Surface::new_2d(width, 8, rgba, Layout::BlockLinear { height_log2: 4, depth_log2: 0 });
        assert_eq!(bl.size_bytes(), Err(TilingError::Overflow));
        assert!(detile(&pitch, &[0; 64]).is_err());
        assert!(detile(&bl, &[]).is_err());
    }
    // BC elements: 4x4 texels, 16 bytes each. width / 4 * 16 wraps at 2^30 texels too.
    let bc = Surface::new_2d(u32::MAX, 4, Format::Bc7Unorm, Layout::Pitch { stride: 16 });
    assert_eq!(bc.size_bytes(), Err(TilingError::Overflow));
}

#[test]
fn block_linear_padded_row_must_fit_u32() {
    // 0xFFFF_FFF0 row bytes fit u32, but rounding up to whole 64-byte GOBs does not.
    let s = Surface::new_2d(0x3FFF_FFFC, 1, Format::R8G8B8A8Unorm, Layout::BlockLinear { height_log2: 0, depth_log2: 0 });
    assert_eq!(s.checked_row_bytes(), Some(0xFFFF_FFF0));
    assert_eq!(s.size_bytes(), Err(TilingError::Overflow));
    let ok = Surface::new_2d(0x3FFF_FFF0, 1, Format::R8G8B8A8Unorm, Layout::BlockLinear { height_log2: 0, depth_log2: 0 });
    assert!(ok.size_bytes().is_ok());
}

#[test]
fn pitch_size_product_overflow_is_rejected() {
    // (rows * depth - 1) * stride exceeds u64.
    let s = Surface {
        width: 1,
        height: u32::MAX,
        depth: u32::MAX,
        format: Format::R8G8B8A8Unorm,
        layout: Layout::Pitch { stride: u32::MAX },
    };
    assert_eq!(s.size_bytes(), Err(TilingError::Overflow));
}
