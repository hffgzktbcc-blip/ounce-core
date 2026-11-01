//! BC1-BC7 (Khronos Data Format Specification 1.3, "S3TC", "RGTC" and "BPTC" sections).
//!
//! `i` is the texel index inside the 4x4 block, row-major (`y * 4 + x`).

use super::{bits_at, half_to_f32};

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// RGB565 to 8-bit channels by bit replication.
fn rgb565(c: u16) -> [u32; 3] {
    let (r, g, b) = ((c >> 11) as u32, ((c >> 5) & 0x3f) as u32, (c & 0x1f) as u32);
    [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
}

/// BC1 colour block (8 bytes). `punch` enables the 3-colour + transparent mode when
/// `c0 <= c1`; BC2/BC3 colour blocks always use four colours.
pub(super) fn bc1(b: &[u8], i: usize, punch: bool) -> [f32; 4] {
    let (c0, c1) = (u16_at(b, 0), u16_at(b, 2));
    let (e0, e1) = (rgb565(c0), rgb565(c1));
    let sel = (u32::from_le_bytes([b[4], b[5], b[6], b[7]]) >> (2 * i)) & 3;
    let mix = |num0: u32, num1: u32, den: u32| [0, 1, 2].map(|k| (num0 * e0[k] + num1 * e1[k]) / den);
    let four = c0 > c1 || !punch;
    let rgb = match (sel, four) {
        (0, _) => e0,
        (1, _) => e1,
        (2, true) => mix(2, 1, 3),
        (3, true) => mix(1, 2, 3),
        (2, false) => mix(1, 1, 2),
        _ => return [0.0; 4],
    };
    [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0]
}

/// BC2: explicit 4-bit alpha, then a BC1 colour block.
pub(super) fn bc2(b: &[u8], i: usize) -> [f32; 4] {
    let mut c = bc1(&b[8..], i, false);
    c[3] = ((u64::from_le_bytes(b[..8].try_into().unwrap()) >> (4 * i)) & 0xf) as f32 / 15.0;
    c
}

/// BC3: a BC4 alpha block, then a BC1 colour block.
pub(super) fn bc3(b: &[u8], i: usize) -> [f32; 4] {
    let mut c = bc1(&b[8..], i, false);
    c[3] = bc4(b, i, false);
    c
}

/// One BC4 channel (8 bytes): two endpoints and 3-bit selectors.
pub(super) fn bc4(b: &[u8], i: usize, signed: bool) -> f32 {
    let sel = (bits_at(b, 16 + 3 * i as u32, 3)) as i32;
    let (a0, a1, scale) = if signed {
        // -128 decodes as -127.
        ((b[0] as i8).max(-127) as i32, (b[1] as i8).max(-127) as i32, 127.0)
    } else {
        (b[0] as i32, b[1] as i32, 255.0)
    };
    let v = match sel {
        0 => a0 as f32,
        1 => a1 as f32,
        s if a0 > a1 => ((8 - s) * a0 + (s - 1) * a1) as f32 / 7.0,
        6 => if signed { -127.0 } else { 0.0 },
        7 => scale,
        s => ((6 - s) * a0 + (s - 1) * a1) as f32 / 5.0,
    };
    v / scale
}

const W2: [u32; 4] = [0, 21, 43, 64];
const W3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const W4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

fn weights(bits: u32) -> &'static [u32] {
    match bits {
        2 => &W2,
        3 => &W3,
        _ => &W4,
    }
}

/// Two-subset partitions: bit `i` is texel `i`'s subset.
const P2: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800, 0xffe8, 0xff00, 0xfff0, 0xf000,
    0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce, 0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c,
    0xaaaa, 0xf0f0, 0x5a5a, 0x33cc, 0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c, 0x9966, 0x0660,
    0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718, 0xccf0, 0x0fcc, 0x7744, 0xee22,
];

/// Three-subset partitions: two bits per texel, texel 0 in bits 1:0.
const P3: [u32; 64] = {
    const T: [[u8; 16]; 64] = [
        [0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2],
        [0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1],
        [0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1],
        [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1, 1, 1],
        [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2],
        [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 2, 2],
        [0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 1, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1],
        [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2],
        [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
        [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2],
        [0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2],
        [0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2],
        [0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2],
        [0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2, 1, 2, 2, 2],
        [0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0, 2, 2, 2, 0],
        [0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2],
        [0, 1, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0],
        [0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2],
        [0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1],
        [0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2, 0, 2, 2, 2],
        [0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 1, 2, 2, 2, 1],
        [0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2],
        [0, 0, 0, 0, 1, 1, 0, 0, 2, 2, 1, 0, 2, 2, 1, 0],
        [0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0],
        [0, 0, 1, 2, 0, 0, 1, 2, 1, 1, 2, 2, 2, 2, 2, 2],
        [0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0],
        [0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1],
        [0, 0, 2, 2, 1, 1, 0, 2, 1, 1, 0, 2, 0, 0, 2, 2],
        [0, 1, 1, 0, 0, 1, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2],
        [0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1],
        [0, 0, 0, 0, 2, 0, 0, 0, 2, 2, 1, 1, 2, 2, 2, 1],
        [0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 2, 2, 2],
        [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1],
        [0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 2, 0, 2, 2, 2],
        [0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0],
        [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0],
        [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0],
        [0, 1, 2, 0, 2, 0, 1, 2, 1, 2, 0, 1, 0, 1, 2, 0],
        [0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2, 0, 0, 1, 1],
        [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 1, 1],
        [0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2],
        [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1],
        [0, 0, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2, 1, 1, 2, 2],
        [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1],
        [0, 2, 2, 0, 1, 2, 2, 1, 0, 2, 2, 0, 1, 2, 2, 1],
        [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 0, 1],
        [0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1],
        [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2],
        [0, 2, 2, 2, 0, 1, 1, 1, 0, 2, 2, 2, 0, 1, 1, 1],
        [0, 0, 0, 2, 1, 1, 1, 2, 0, 0, 0, 2, 1, 1, 1, 2],
        [0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2],
        [0, 2, 2, 2, 0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2],
        [0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 2],
        [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2],
        [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2],
        [0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2],
        [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2],
        [0, 0, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2],
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2],
        [0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1],
        [0, 2, 2, 2, 1, 2, 2, 2, 0, 2, 2, 2, 1, 2, 2, 2],
        [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
        [0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 1, 2, 2, 2, 0],
    ];
    let mut out = [0u32; 64];
    let mut p = 0;
    while p < 64 {
        let mut t = 0;
        while t < 16 {
            out[p] |= (T[p][t] as u32) << (2 * t);
            t += 1;
        }
        p += 1;
    }
    out
};

/// Anchor texel of subset 1 for two-subset partitions.
const A2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2,
    15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2, 2, 15, 15, 15, 15, 15, 2, 2, 15,
];
/// Anchors of subsets 1 and 2 for three-subset partitions.
const A3A: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5, 15, 15,
    8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5, 10, 8, 13, 15, 12, 3, 3,
];
const A3B: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6, 10, 15, 15, 10, 8,
    15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];

/// Subset of texel `i` and whether `i` is its subset's anchor.
fn subset(subsets: u32, partition: usize, i: usize) -> (usize, bool) {
    match subsets {
        1 => (0, i == 0),
        2 => {
            let s = ((P2[partition] >> i) & 1) as usize;
            (s, i == [0, A2[partition] as usize][s])
        }
        _ => {
            let s = ((P3[partition] >> (2 * i)) & 3) as usize;
            (s, i == [0, A3A[partition] as usize, A3B[partition] as usize][s])
        }
    }
}

/// Bit offset of texel `i`'s index in an index section that starts at `base`, with `bits` per index
/// and one bit less for each anchor before or at `i`.
fn index_of(data: &[u8], base: u32, bits: u32, subsets: u32, partition: usize, i: usize) -> u32 {
    let mut pos = base;
    for t in 0..i {
        pos += bits - subset(subsets, partition, t).1 as u32;
    }
    let n = bits - subset(subsets, partition, i).1 as u32;
    bits_at(data, pos, n) as u32
}

fn interp(e0: u32, e1: u32, w: u32) -> u32 {
    ((64 - w) * e0 + w * e1 + 32) >> 6
}

/// BC7: eight modes.
pub(super) fn bc7(b: &[u8], i: usize) -> [f32; 4] {
    let Some(mode) = (0..8).find(|m| b[0] & (1 << m) != 0) else { return [0.0; 4] };
    // (subsets, partition bits, rotation bits, index selection bit, colour bits, alpha bits,
    //  per-endpoint p-bits, shared p-bits, index bits, second index bits)
    const MODES: [(u32, u32, u32, u32, u32, u32, u32, u32, u32, u32); 8] = [
        (3, 4, 0, 0, 4, 0, 1, 0, 3, 0),
        (2, 6, 0, 0, 6, 0, 0, 1, 3, 0),
        (3, 6, 0, 0, 5, 0, 0, 0, 2, 0),
        (2, 6, 0, 0, 7, 0, 1, 0, 2, 0),
        (1, 0, 2, 1, 5, 6, 0, 0, 2, 3),
        (1, 0, 2, 0, 7, 8, 0, 0, 2, 2),
        (1, 0, 0, 0, 7, 7, 1, 0, 4, 0),
        (2, 6, 0, 0, 5, 5, 1, 0, 2, 0),
    ];
    let (ns, pb, rb, isb, cb, ab, epb, spb, ib, ib2) = MODES[mode];
    let mut pos = mode as u32 + 1;
    let mut read = |n: u32| {
        let v = bits_at(b, pos, n) as u32;
        pos += n;
        v
    };
    let partition = read(pb) as usize;
    let rotation = read(rb);
    let isel = read(isb);
    let mut ep = [[0u32; 4]; 6];
    let ne = 2 * ns as usize;
    for ch in 0..3 {
        for e in ep.iter_mut().take(ne) {
            e[ch] = read(cb);
        }
    }
    for e in ep.iter_mut().take(ne) {
        e[3] = if ab > 0 { read(ab) } else { 255 };
    }
    let mut cbits = cb;
    let mut abits = ab;
    if epb != 0 || spb != 0 {
        let pbits: Vec<u32> = if epb != 0 {
            (0..ne).map(|_| read(1)).collect()
        } else {
            (0..ns).flat_map(|_| {
                let p = read(1);
                [p, p]
            }).collect()
        };
        for (e, p) in ep.iter_mut().take(ne).zip(pbits) {
            for ch in 0..3 {
                e[ch] = (e[ch] << 1) | p;
            }
            if ab > 0 {
                e[3] = (e[3] << 1) | p;
            }
        }
        cbits += 1;
        if ab > 0 {
            abits += 1;
        }
    }
    let expand = |v: u32, n: u32| (v << (8 - n)) | (v >> (2 * n - 8));
    for e in ep.iter_mut().take(ne) {
        for ch in 0..3 {
            e[ch] = expand(e[ch], cbits);
        }
        if ab > 0 {
            e[3] = expand(e[3], abits);
        }
    }
    let (s, _) = subset(ns, partition, i);
    let (e0, e1) = (ep[2 * s], ep[2 * s + 1]);
    let idx_base = pos;
    let primary = index_of(b, idx_base, ib, ns, partition, i);
    let mut c = [0u32; 4];
    if ib2 == 0 {
        let w = weights(ib)[primary as usize];
        for ch in 0..4 {
            c[ch] = interp(e0[ch], e1[ch], w);
        }
    } else {
        let secondary = index_of(b, idx_base + 16 * ib - 1, ib2, 1, 0, i);
        let (ci, cbits_i, ai, abits_i) = if isel == 0 { (primary, ib, secondary, ib2) } else { (secondary, ib2, primary, ib) };
        let (wc, wa) = (weights(cbits_i)[ci as usize], weights(abits_i)[ai as usize]);
        for ch in 0..3 {
            c[ch] = interp(e0[ch], e1[ch], wc);
        }
        c[3] = interp(e0[3], e1[3], wa);
    }
    match rotation {
        1 => c.swap(0, 3),
        2 => c.swap(1, 3),
        3 => c.swap(2, 3),
        _ => {}
    }
    c.map(|v| v as f32 / 255.0)
}

const RW: u8 = 0;
const GW: u8 = 1;
const BW: u8 = 2;
const RX: u8 = 3;
const GX: u8 = 4;
const BX: u8 = 5;
const RY: u8 = 6;
const GY: u8 = 7;
const BY: u8 = 8;
const RZ: u8 = 9;
const GZ: u8 = 10;
const BZ: u8 = 11;

/// One BC6H mode: mode value, whether endpoints are deltas, subsets, endpoint bits, delta bits
/// (R, G, B), and its field list as (endpoint channel, first bit, count). A negative count stores
/// the field reversed (most significant bit first).
struct Bc6Mode {
    value: u8,
    mode_bits: u32,
    transformed: bool,
    subsets: u32,
    epb: u32,
    delta: [u32; 3],
    fields: &'static [(u8, u8, i8)],
}

const BC6_MODES: [Bc6Mode; 14] = [
    Bc6Mode { value: 0x00, mode_bits: 2, transformed: true, subsets: 2, epb: 10, delta: [5, 5, 5], fields: &[
        (GY, 4, 1), (BY, 4, 1), (BZ, 4, 1), (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 5), (GZ, 4, 1), (GY, 0, 4),
        (GX, 0, 5), (BZ, 0, 1), (GZ, 0, 4), (BX, 0, 5), (BZ, 1, 1), (BY, 0, 4), (RY, 0, 5), (BZ, 2, 1), (RZ, 0, 5), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x01, mode_bits: 2, transformed: true, subsets: 2, epb: 7, delta: [6, 6, 6], fields: &[
        (GY, 5, 1), (GZ, 4, 1), (GZ, 5, 1), (RW, 0, 7), (BZ, 0, 1), (BZ, 1, 1), (BY, 4, 1), (GW, 0, 7), (BY, 5, 1),
        (BZ, 2, 1), (GY, 4, 1), (BW, 0, 7), (BZ, 3, 1), (BZ, 5, 1), (BZ, 4, 1), (RX, 0, 6), (GY, 0, 4), (GX, 0, 6),
        (GZ, 0, 4), (BX, 0, 6), (BY, 0, 4), (RY, 0, 6), (RZ, 0, 6),
    ] },
    Bc6Mode { value: 0x02, mode_bits: 5, transformed: true, subsets: 2, epb: 11, delta: [5, 4, 4], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 5), (RW, 10, 1), (GY, 0, 4), (GX, 0, 4), (GW, 10, 1), (BZ, 0, 1),
        (GZ, 0, 4), (BX, 0, 4), (BW, 10, 1), (BZ, 1, 1), (BY, 0, 4), (RY, 0, 5), (BZ, 2, 1), (RZ, 0, 5), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x06, mode_bits: 5, transformed: true, subsets: 2, epb: 11, delta: [4, 5, 4], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 4), (RW, 10, 1), (GZ, 4, 1), (GY, 0, 4), (GX, 0, 5), (GW, 10, 1),
        (GZ, 0, 4), (BX, 0, 4), (BW, 10, 1), (BZ, 1, 1), (BY, 0, 4), (RY, 0, 4), (BZ, 0, 1), (BZ, 2, 1), (RZ, 0, 4),
        (GY, 4, 1), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x0a, mode_bits: 5, transformed: true, subsets: 2, epb: 11, delta: [4, 4, 5], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 4), (RW, 10, 1), (BY, 4, 1), (GY, 0, 4), (GX, 0, 4), (GW, 10, 1),
        (BZ, 0, 1), (GZ, 0, 4), (BX, 0, 5), (BW, 10, 1), (BY, 0, 4), (RY, 0, 4), (BZ, 1, 1), (BZ, 2, 1), (RZ, 0, 4),
        (BZ, 4, 1), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x0e, mode_bits: 5, transformed: true, subsets: 2, epb: 9, delta: [5, 5, 5], fields: &[
        (RW, 0, 9), (BY, 4, 1), (GW, 0, 9), (GY, 4, 1), (BW, 0, 9), (BZ, 4, 1), (RX, 0, 5), (GZ, 4, 1), (GY, 0, 4),
        (GX, 0, 5), (BZ, 0, 1), (GZ, 0, 4), (BX, 0, 5), (BZ, 1, 1), (BY, 0, 4), (RY, 0, 5), (BZ, 2, 1), (RZ, 0, 5), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x12, mode_bits: 5, transformed: true, subsets: 2, epb: 8, delta: [6, 5, 5], fields: &[
        (RW, 0, 8), (GZ, 4, 1), (BY, 4, 1), (GW, 0, 8), (BZ, 2, 1), (GY, 4, 1), (BW, 0, 8), (BZ, 3, 1), (BZ, 4, 1),
        (RX, 0, 6), (GY, 0, 4), (GX, 0, 5), (BZ, 0, 1), (GZ, 0, 4), (BX, 0, 5), (BZ, 1, 1), (BY, 0, 4), (RY, 0, 6), (RZ, 0, 6),
    ] },
    Bc6Mode { value: 0x16, mode_bits: 5, transformed: true, subsets: 2, epb: 8, delta: [5, 6, 5], fields: &[
        (RW, 0, 8), (BZ, 0, 1), (BY, 4, 1), (GW, 0, 8), (GY, 5, 1), (GY, 4, 1), (BW, 0, 8), (GZ, 5, 1), (BZ, 4, 1),
        (RX, 0, 5), (GZ, 4, 1), (GY, 0, 4), (GX, 0, 6), (GZ, 0, 4), (BX, 0, 5), (BZ, 1, 1), (BY, 0, 4), (RY, 0, 5),
        (BZ, 2, 1), (RZ, 0, 5), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x1a, mode_bits: 5, transformed: true, subsets: 2, epb: 8, delta: [5, 5, 6], fields: &[
        (RW, 0, 8), (BZ, 1, 1), (BY, 4, 1), (GW, 0, 8), (BY, 5, 1), (GY, 4, 1), (BW, 0, 8), (BZ, 5, 1), (BZ, 4, 1),
        (RX, 0, 5), (GZ, 4, 1), (GY, 0, 4), (GX, 0, 5), (BZ, 0, 1), (GZ, 0, 4), (BX, 0, 6), (BY, 0, 4), (RY, 0, 5),
        (BZ, 2, 1), (RZ, 0, 5), (BZ, 3, 1),
    ] },
    Bc6Mode { value: 0x1e, mode_bits: 5, transformed: false, subsets: 2, epb: 6, delta: [6, 6, 6], fields: &[
        (RW, 0, 6), (GZ, 4, 1), (BZ, 0, 1), (BZ, 1, 1), (BY, 4, 1), (GW, 0, 6), (GY, 5, 1), (BY, 5, 1), (BZ, 2, 1),
        (GY, 4, 1), (BW, 0, 6), (GZ, 5, 1), (BZ, 3, 1), (BZ, 5, 1), (BZ, 4, 1), (RX, 0, 6), (GY, 0, 4), (GX, 0, 6),
        (GZ, 0, 4), (BX, 0, 6), (BY, 0, 4), (RY, 0, 6), (RZ, 0, 6),
    ] },
    Bc6Mode { value: 0x03, mode_bits: 5, transformed: false, subsets: 1, epb: 10, delta: [10, 10, 10], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 10), (GX, 0, 10), (BX, 0, 10),
    ] },
    Bc6Mode { value: 0x07, mode_bits: 5, transformed: true, subsets: 1, epb: 11, delta: [9, 9, 9], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 9), (RW, 10, 1), (GX, 0, 9), (GW, 10, 1), (BX, 0, 9), (BW, 10, 1),
    ] },
    Bc6Mode { value: 0x0b, mode_bits: 5, transformed: true, subsets: 1, epb: 12, delta: [8, 8, 8], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 8), (RW, 10, -2), (GX, 0, 8), (GW, 10, -2), (BX, 0, 8), (BW, 10, -2),
    ] },
    Bc6Mode { value: 0x0f, mode_bits: 5, transformed: true, subsets: 1, epb: 16, delta: [4, 4, 4], fields: &[
        (RW, 0, 10), (GW, 0, 10), (BW, 0, 10), (RX, 0, 4), (RW, 10, -6), (GX, 0, 4), (GW, 10, -6), (BX, 0, 4), (BW, 10, -6),
    ] },
];

fn sign_extend(v: u32, bits: u32) -> i32 {
    ((v << (32 - bits)) as i32) >> (32 - bits)
}

/// BC6H, unsigned or signed half floats.
pub(super) fn bc6h(b: &[u8], i: usize, signed: bool) -> [f32; 4] {
    let two = (b[0] & 3) as u8;
    let five = b[0] & 0x1f;
    let Some(m) = BC6_MODES.iter().find(|m| if m.mode_bits == 2 { m.value == two && two < 2 } else { two >= 2 && m.value == five })
    else {
        return [0.0, 0.0, 0.0, 1.0];
    };
    let mut ep = [0u32; 12];
    let mut pos = m.mode_bits;
    for &(e, lo, count) in m.fields {
        let n = count.unsigned_abs() as u32;
        let v = bits_at(b, pos, n) as u32;
        pos += n;
        let v = if count < 0 { v.reverse_bits() >> (32 - n) } else { v };
        ep[e as usize] |= v << lo;
    }
    let partition = if m.subsets == 2 { bits_at(b, pos, 5) as usize } else { 0 };
    let ne = 2 * m.subsets as usize;
    // Endpoints as signed integers of `epb` bits.
    let mut e = [[0i32; 3]; 4];
    let mask = (1u32 << m.epb) - 1;
    for k in 0..ne {
        for ch in 0..3 {
            let raw = ep[3 * k + ch];
            e[k][ch] = if k == 0 || !m.transformed {
                let bits = if k == 0 { m.epb } else { m.delta[ch] };
                if signed { sign_extend(raw, bits) } else { raw as i32 }
            } else {
                let d = sign_extend(raw, m.delta[ch]);
                let v = (ep[ch] as i32).wrapping_add(d) as u32 & mask;
                if signed { sign_extend(v, m.epb) } else { v as i32 }
            };
        }
    }
    let unq = |x: i32| -> i32 {
        if signed {
            if m.epb >= 16 {
                return x;
            }
            let (neg, a) = (x < 0, x.abs());
            let q = if a == 0 {
                0
            } else if a >= (1 << (m.epb - 1)) - 1 {
                0x7fff
            } else {
                ((a << 15) + 0x4000) >> (m.epb - 1)
            };
            if neg { -q } else { q }
        } else if m.epb >= 15 {
            x
        } else if x == 0 {
            0
        } else if x == mask as i32 {
            0xffff
        } else {
            ((x << 16) + 0x8000) >> m.epb
        }
    };
    let (s, _) = subset(m.subsets, partition, i);
    let (ib, base) = if m.subsets == 2 { (3, 82) } else { (4, 65) };
    let idx = index_of(b, base, ib, m.subsets, partition, i);
    let w = weights(ib)[idx as usize] as i32;
    let mut out = [0.0, 0.0, 0.0, 1.0];
    for ch in 0..3 {
        let (a, c) = (unq(e[2 * s][ch]), unq(e[2 * s + 1][ch]));
        let v = ((64 - w) * a + w * c + 32) >> 6;
        let h = if signed {
            if v < 0 { 0x8000 | (((-v) * 31) >> 5) as u16 } else { ((v * 31) >> 5) as u16 }
        } else {
            ((v * 31) >> 6) as u16
        };
        out[ch] = half_to_f32(h);
    }
    out
}
