//! ASTC 2D, LDR profile (Khronos Data Format Specification 1.3, "ASTC" section).
//!
//! Every 128-bit block is decoded in full for each texel asked for; callers that sample many
//! texels of the same block pay that again. HDR endpoint modes, HDR void-extent blocks and every
//! block the specification calls illegal decode to the LDR error colour, opaque magenta.

use super::bits_at;

const ERROR: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

/// Decodes texel `i` (row-major) of a `bw x bh` block.
pub(super) fn decode(b: &[u8], bw: u32, bh: u32, i: usize) -> [f32; 4] {
    decode_block(b, bw, bh, i).unwrap_or(ERROR)
}

/// A quantization range as (trits, quints, plain bits).
#[derive(Clone, Copy)]
struct Range {
    trits: bool,
    quints: bool,
    bits: u32,
}

impl Range {
    const fn new(trits: bool, quints: bool, bits: u32) -> Self {
        Range { trits, quints, bits }
    }

    fn levels(&self) -> u32 {
        (1 << self.bits) * if self.trits { 3 } else if self.quints { 5 } else { 1 }
    }

    /// Bits that `n` values take in the integer sequence encoding.
    fn ise_bits(&self, n: u32) -> u32 {
        n * self.bits
            + if self.trits {
                (8 * n + 4) / 5
            } else if self.quints {
                (7 * n + 2) / 3
            } else {
                0
            }
    }
}

/// Colour ranges from 2 to 256 levels, in increasing order.
const RANGES: [Range; 21] = [
    Range::new(false, false, 1),
    Range::new(true, false, 0),
    Range::new(false, false, 2),
    Range::new(false, true, 0),
    Range::new(true, false, 1),
    Range::new(false, false, 3),
    Range::new(false, true, 1),
    Range::new(true, false, 2),
    Range::new(false, false, 4),
    Range::new(false, true, 2),
    Range::new(true, false, 3),
    Range::new(false, false, 5),
    Range::new(false, true, 3),
    Range::new(true, false, 4),
    Range::new(false, false, 6),
    Range::new(false, true, 4),
    Range::new(true, false, 5),
    Range::new(false, false, 7),
    Range::new(false, true, 5),
    Range::new(true, false, 6),
    Range::new(false, false, 8),
];

/// Weight range from the block mode's `R` and `H` fields.
fn weight_range(r: u32, high: bool) -> Option<Range> {
    let low = [
        Range::new(false, false, 1), // 2
        Range::new(true, false, 0),  // 3
        Range::new(false, false, 2), // 4
        Range::new(false, true, 0),  // 5
        Range::new(true, false, 1),  // 6
        Range::new(false, false, 3), // 8
    ];
    let hi = [
        Range::new(false, true, 1), // 10
        Range::new(true, false, 2), // 12
        Range::new(false, false, 4), // 16
        Range::new(false, true, 2), // 20
        Range::new(true, false, 3), // 24
        Range::new(false, false, 5), // 32
    ];
    let idx = r.checked_sub(2)? as usize;
    (if high { hi } else { low }).get(idx).copied()
}

/// Weight grid width, height, `R`, high precision and dual plane from the 11-bit block mode.
fn block_mode(m: u32) -> Option<(u32, u32, u32, bool, bool)> {
    let bit = |n: u32| (m >> n) & 1;
    if m & 3 != 0 {
        let r = ((m & 3) << 1) | bit(4);
        let (a, b) = ((m >> 5) & 3, (m >> 7) & 3);
        let (w, h) = match (m >> 2) & 3 {
            0 => (b + 4, a + 2),
            1 => (b + 8, a + 2),
            2 => (a + 2, b + 8),
            _ if bit(8) == 0 => (a + 2, (b & 1) + 6),
            _ => ((b & 1) + 2, a + 2),
        };
        Some((w, h, r, bit(9) == 1, bit(10) == 1))
    } else {
        let r = (((m >> 2) & 3) << 1) | bit(4);
        if (m >> 2) & 3 == 0 {
            return None;
        }
        let a = (m >> 5) & 3;
        match (m >> 7) & 3 {
            0 => Some((12, a + 2, r, bit(9) == 1, bit(10) == 1)),
            1 => Some((a + 2, 12, r, bit(9) == 1, bit(10) == 1)),
            2 => Some((a + 6, ((m >> 9) & 3) + 6, r, false, false)),
            _ => match a {
                0 => Some((6, 10, r, bit(9) == 1, bit(10) == 1)),
                1 => Some((10, 6, r, bit(9) == 1, bit(10) == 1)),
                _ => None,
            },
        }
    }
}

/// Decodes `n` values from the integer sequence at bit `start` of `data`.
fn ise(data: &[u8; 16], start: u32, n: usize, range: Range, end: u32) -> Vec<(u32, u32)> {
    // (high part: trit or quint, low bits)
    let read = |pos: &mut u32, k: u32| -> u32 {
        let mut v = 0;
        for j in 0..k {
            let p = *pos + j;
            if p < end {
                v |= (bits_at(data, p, 1) as u32) << j;
            }
        }
        *pos += k;
        v
    };
    let mut out = Vec::with_capacity(n);
    let mut pos = start;
    let b = range.bits;
    if range.trits {
        while out.len() < n {
            let mut m = [0u32; 5];
            let mut t = 0u32;
            for (k, (lo, tb)) in [(0, 2), (2, 2), (4, 1), (5, 2), (7, 1)].into_iter().enumerate() {
                m[k] = read(&mut pos, b);
                t |= read(&mut pos, tb) << lo;
            }
            for (k, tv) in trits(t).into_iter().enumerate() {
                if out.len() < n {
                    out.push((tv, m[k]));
                }
            }
        }
    } else if range.quints {
        while out.len() < n {
            let mut m = [0u32; 3];
            let mut q = 0u32;
            for (k, (lo, qb)) in [(0, 3), (3, 2), (5, 2)].into_iter().enumerate() {
                m[k] = read(&mut pos, b);
                q |= read(&mut pos, qb) << lo;
            }
            for (k, qv) in quints(q).into_iter().enumerate() {
                if out.len() < n {
                    out.push((qv, m[k]));
                }
            }
        }
    } else {
        for _ in 0..n {
            out.push((0, read(&mut pos, b)));
        }
    }
    out
}

fn trits(t: u32) -> [u32; 5] {
    let bit = |v: u32, n: u32| (v >> n) & 1;
    let field = |v: u32, hi: u32, lo: u32| (v >> lo) & ((1 << (hi - lo + 1)) - 1);
    let (c, t4, t3);
    if field(t, 4, 2) == 7 {
        c = (field(t, 7, 5) << 2) | field(t, 1, 0);
        t4 = 2;
        t3 = 2;
    } else {
        c = field(t, 4, 0);
        if field(t, 6, 5) == 3 {
            t4 = 2;
            t3 = bit(t, 7);
        } else {
            t4 = bit(t, 7);
            t3 = field(t, 6, 5);
        }
    }
    let (t2, t1, t0);
    if field(c, 1, 0) == 3 {
        t2 = 2;
        t1 = bit(c, 4);
        t0 = (bit(c, 3) << 1) | (bit(c, 2) & !bit(c, 3) & 1);
    } else if field(c, 3, 2) == 3 {
        t2 = 2;
        t1 = 2;
        t0 = field(c, 1, 0);
    } else {
        t2 = bit(c, 4);
        t1 = field(c, 3, 2);
        t0 = (bit(c, 1) << 1) | (bit(c, 0) & !bit(c, 1) & 1);
    }
    [t0, t1, t2, t3, t4]
}

fn quints(q: u32) -> [u32; 3] {
    let bit = |v: u32, n: u32| (v >> n) & 1;
    let field = |v: u32, hi: u32, lo: u32| (v >> lo) & ((1 << (hi - lo + 1)) - 1);
    if field(q, 2, 1) == 3 && field(q, 6, 5) == 0 {
        let q2 = (bit(q, 0) << 2) | ((bit(q, 4) & !bit(q, 0) & 1) << 1) | (bit(q, 3) & !bit(q, 0) & 1);
        return [4, 4, q2];
    }
    let (q2, c) = if field(q, 2, 1) == 3 {
        (4, (field(q, 4, 3) << 3) | ((!field(q, 6, 5) & 3) << 1) | bit(q, 0))
    } else {
        (field(q, 6, 5), field(q, 4, 0))
    };
    let (q1, q0) = if field(c, 2, 0) == 5 { (4, field(c, 4, 3)) } else { (field(c, 4, 3), field(c, 2, 0)) };
    [q0, q1, q2]
}

/// Replicates the low `n` bits of `v` to `to` bits.
fn replicate(v: u32, n: u32, to: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let mut out = 0;
    let mut have = 0;
    while have < to {
        out = (out << n) | v;
        have += n;
    }
    out >> (have - to)
}

/// Colour value to 0..255.
fn unquantize_color(range: Range, (d, m): (u32, u32)) -> u32 {
    let b = range.bits;
    if !range.trits && !range.quints {
        return replicate(m, b, 8);
    }
    let a = if m & 1 != 0 { 0x1ff } else { 0 };
    let bit = |n: u32| (m >> n) & 1;
    let (bb, c) = match (range.trits, b) {
        (true, 1) => (0, 204),
        (false, 1) => (0, 113),
        (true, 2) => {
            let x = bit(1);
            ((x << 8) | (x << 4) | (x << 2) | (x << 1), 93)
        }
        (false, 2) => {
            let x = bit(1);
            ((x << 8) | (x << 3) | (x << 2), 54)
        }
        (true, 3) => {
            let (x, y) = (bit(1), bit(2));
            ((y << 8) | (x << 7) | (y << 3) | (x << 2) | (y << 1) | x, 44)
        }
        (false, 3) => {
            let (x, y) = (bit(1), bit(2));
            ((y << 8) | (x << 7) | (y << 2) | (x << 1) | y, 26)
        }
        (true, 4) => {
            let (x, y, z) = (bit(1), bit(2), bit(3));
            ((z << 8) | (y << 7) | (x << 6) | (z << 2) | (y << 1) | x, 22)
        }
        (false, 4) => {
            let (x, y, z) = (bit(1), bit(2), bit(3));
            ((z << 8) | (y << 7) | (x << 6) | (z << 1) | y, 13)
        }
        (true, 5) => {
            let (x, y, z, w) = (bit(1), bit(2), bit(3), bit(4));
            ((w << 8) | (z << 7) | (y << 6) | (x << 5) | (w << 1) | z, 11)
        }
        (false, 5) => {
            let (x, y, z, w) = (bit(1), bit(2), bit(3), bit(4));
            ((w << 8) | (z << 7) | (y << 6) | (x << 5) | w, 6)
        }
        (true, 6) => {
            let (x, y, z, w, v) = (bit(1), bit(2), bit(3), bit(4), bit(5));
            ((v << 8) | (w << 7) | (z << 6) | (y << 5) | (x << 4) | v, 5)
        }
        // Ranges below 6 levels are rejected before unquantization.
        _ => return 0,
    };
    let t = (d * c + bb) ^ a;
    (a & 0x80) | (t >> 2)
}

/// Weight value to 0..64.
fn unquantize_weight(range: Range, (d, m): (u32, u32)) -> u32 {
    let b = range.bits;
    let v = if !range.trits && !range.quints {
        replicate(m, b, 6)
    } else if b == 0 {
        return if range.trits { [0, 32, 64][d as usize] } else { [0, 16, 32, 48, 64][d as usize] };
    } else {
        let a = if m & 1 != 0 { 0x7f } else { 0 };
        let bit = |n: u32| (m >> n) & 1;
        let (bb, c) = match (range.trits, b) {
            (true, 1) => (0, 50),
            (false, 1) => (0, 28),
            (true, 2) => {
                let x = bit(1);
                ((x << 6) | (x << 2) | x, 23)
            }
            (false, 2) => {
                let x = bit(1);
                ((x << 6) | (x << 1), 13)
            }
            (true, 3) => {
                let (x, y) = (bit(1), bit(2));
                ((y << 6) | (x << 5) | (y << 1) | x, 11)
            }
            _ => return 0,
        };
        let t = (d * c + bb) ^ a;
        (a & 0x20) | (t >> 2)
    };
    if v > 32 { v + 1 } else { v }
}

fn hash52(mut p: u32) -> u32 {
    p ^= p >> 15;
    p = p.wrapping_sub(p << 17);
    p = p.wrapping_add(p << 7);
    p = p.wrapping_add(p << 4);
    p ^= p >> 5;
    p = p.wrapping_add(p << 16);
    p ^= p >> 7;
    p ^= p >> 3;
    p ^= p << 6;
    p ^= p >> 17;
    p
}

/// The specification's partition selection function (2D: z = 0).
fn select_partition(seed: u32, mut x: u32, mut y: u32, count: u32, small: bool) -> usize {
    if small {
        x <<= 1;
        y <<= 1;
    }
    let seed = seed + (count - 1) * 1024;
    let r = hash52(seed);
    let mut s = [0u32; 12];
    for (k, v) in s.iter_mut().enumerate().take(8) {
        *v = (r >> (4 * k)) & 0xf;
    }
    s[8] = (r >> 18) & 0xf;
    s[9] = (r >> 22) & 0xf;
    s[10] = (r >> 26) & 0xf;
    s[11] = ((r >> 30) | (r << 2)) & 0xf;
    for v in s.iter_mut() {
        *v *= *v;
    }
    let (sh1, sh2) = if seed & 1 != 0 {
        (if seed & 2 != 0 { 4 } else { 5 }, if count == 3 { 6 } else { 5 })
    } else {
        (if count == 3 { 6 } else { 5 }, if seed & 2 != 0 { 4 } else { 5 })
    };
    let sh3 = if seed & 0x10 != 0 { sh1 } else { sh2 };
    for (k, v) in s.iter_mut().enumerate() {
        *v >>= if k >= 8 { sh3 } else if k % 2 == 0 { sh1 } else { sh2 };
    }
    let a = (s[0] * x + s[1] * y + (r >> 14)) & 0x3f;
    let b = (s[2] * x + s[3] * y + (r >> 10)) & 0x3f;
    let c = if count < 3 { 0 } else { (s[4] * x + s[5] * y + (r >> 6)) & 0x3f };
    let d = if count < 4 { 0 } else { (s[6] * x + s[7] * y + (r >> 2)) & 0x3f };
    if a >= b && a >= c && a >= d {
        0
    } else if b >= c && b >= d {
        1
    } else if c >= d {
        2
    } else {
        3
    }
}

fn bit_transfer_signed(a: &mut i32, b: &mut i32) {
    *b >>= 1;
    *b |= *a & 0x80;
    *a >>= 1;
    *a &= 0x3f;
    if *a & 0x20 != 0 {
        *a -= 0x40;
    }
}

fn blue_contract(r: i32, g: i32, b: i32, a: i32) -> [i32; 4] {
    [(r + b) >> 1, (g + b) >> 1, b, a]
}

/// LDR endpoint pair for one colour endpoint mode, or `None` for HDR modes.
fn endpoints(cem: u32, v: &[i32]) -> Option<([i32; 4], [i32; 4])> {
    let clamp = |e: [i32; 4]| e.map(|c| c.clamp(0, 255));
    let (e0, e1) = match cem {
        0 => ([v[0], v[0], v[0], 255], [v[1], v[1], v[1], 255]),
        1 => {
            let l0 = (v[0] >> 2) | (v[1] & 0xc0);
            let l1 = (l0 + (v[1] & 0x3f)).min(255);
            ([l0, l0, l0, 255], [l1, l1, l1, 255])
        }
        4 => ([v[0], v[0], v[0], v[2]], [v[1], v[1], v[1], v[3]]),
        5 => {
            let (mut v0, mut v1, mut v2, mut v3) = (v[0], v[1], v[2], v[3]);
            bit_transfer_signed(&mut v1, &mut v0);
            bit_transfer_signed(&mut v3, &mut v2);
            ([v0, v0, v0, v2], [v0 + v1, v0 + v1, v0 + v1, v2 + v3])
        }
        6 => ([(v[0] * v[3]) >> 8, (v[1] * v[3]) >> 8, (v[2] * v[3]) >> 8, 255], [v[0], v[1], v[2], 255]),
        8 | 12 => {
            let (a0, a1) = if cem == 12 { (v[6], v[7]) } else { (255, 255) };
            if v[1] + v[3] + v[5] >= v[0] + v[2] + v[4] {
                ([v[0], v[2], v[4], a0], [v[1], v[3], v[5], a1])
            } else {
                (blue_contract(v[1], v[3], v[5], a1), blue_contract(v[0], v[2], v[4], a0))
            }
        }
        9 | 13 => {
            let mut w = [0i32; 8];
            w[..v.len().min(8)].copy_from_slice(&v[..v.len().min(8)]);
            let (lo, hi) = w.split_at_mut(1);
            bit_transfer_signed(&mut hi[0], &mut lo[0]);
            for k in [2usize, 4, 6] {
                let (lo, hi) = w.split_at_mut(k + 1);
                bit_transfer_signed(&mut hi[0], &mut lo[k]);
            }
            let (a0, a1) = if cem == 13 { (w[6], w[6] + w[7]) } else { (255, 255) };
            if w[1] + w[3] + w[5] >= 0 {
                ([w[0], w[2], w[4], a0], [w[0] + w[1], w[2] + w[3], w[4] + w[5], a1])
            } else {
                (blue_contract(w[0] + w[1], w[2] + w[3], w[4] + w[5], a1), blue_contract(w[0], w[2], w[4], a0))
            }
        }
        10 => ([(v[0] * v[3]) >> 8, (v[1] * v[3]) >> 8, (v[2] * v[3]) >> 8, v[4]], [v[0], v[1], v[2], v[5]]),
        _ => return None,
    };
    Some((clamp(e0), clamp(e1)))
}

fn decode_block(b: &[u8], bw: u32, bh: u32, i: usize) -> Option<[f32; 4]> {
    let data: &[u8; 16] = b.get(..16)?.try_into().ok()?;
    let bits = |pos: u32, n: u32| bits_at(data, pos, n) as u32;
    let mode = bits(0, 11);
    if mode & 0x1ff == 0x1fc {
        // Void extent: one constant colour. HDR (bit 9) is not part of the LDR profile.
        if mode & 0x200 != 0 {
            return None;
        }
        return Some([0, 1, 2, 3].map(|k| bits(64 + 16 * k, 16) as f32 / 65535.0));
    }
    let (gw, gh, r, high, dual) = block_mode(mode)?;
    let wrange = weight_range(r, high)?;
    let partitions = bits(11, 2) + 1;
    let planes = 1 + dual as u32;
    let nweights = gw * gh * planes;
    if gw > bw || gh > bh || nweights > 64 || (dual && partitions == 4) {
        return None;
    }
    let wbits = wrange.ise_bits(nweights);
    if !(24..=96).contains(&wbits) {
        return None;
    }

    // Colour endpoint modes.
    let mut cems = [0u32; 4];
    let mut extra = 0;
    let color_start;
    if partitions == 1 {
        cems[0] = bits(13, 4);
        color_start = 17;
    } else {
        color_start = 29;
        let sel = bits(23, 2);
        if sel == 0 {
            let c = bits(25, 4);
            cems.iter_mut().for_each(|m| *m = c);
        } else {
            extra = 3 * partitions - 4;
            let hi = bits(128 - wbits - extra, extra);
            let v = (hi << 4) | bits(25, 4);
            let base = sel - 1;
            for p in 0..partitions {
                let c = (v >> p) & 1;
                let m = (v >> (partitions + 2 * p)) & 3;
                cems[p as usize] = ((base + c) << 2) | m;
            }
        }
    }
    let ccs_pos = 128 - wbits - extra - if dual { 2 } else { 0 };
    let ccs = if dual { bits(ccs_pos, 2) as usize } else { 4 };
    if (color_start as u32) > ccs_pos {
        return None;
    }
    let ncolor: u32 = cems[..partitions as usize].iter().map(|c| 2 * ((c >> 2) + 1)).sum();
    if ncolor > 18 {
        return None;
    }
    let avail = ccs_pos - color_start;
    let crange = *RANGES.iter().rev().find(|r| r.ise_bits(ncolor) <= avail)?;
    if crange.levels() < 6 {
        return None;
    }
    let colors: Vec<i32> =
        ise(data, color_start, ncolor as usize, crange, ccs_pos).into_iter().map(|v| unquantize_color(crange, v) as i32).collect();

    // Weights are stored bit-reversed from the top of the block.
    let mut rev = [0u8; 16];
    for (k, byte) in rev.iter_mut().enumerate() {
        *byte = data[15 - k].reverse_bits();
    }
    let weights: Vec<u32> =
        ise(&rev, 0, nweights as usize, wrange, wbits).into_iter().map(|v| unquantize_weight(wrange, v)).collect();

    let (x, y) = (i as u32 % bw, i as u32 / bw);
    let part = if partitions == 1 {
        0
    } else {
        select_partition(bits(13, 10), x, y, partitions, bw * bh < 31)
    };
    let mut off = 0usize;
    for c in &cems[..part] {
        off += 2 * ((c >> 2) + 1) as usize;
    }
    let (e0, e1) = endpoints(cems[part], &colors[off..])?;

    // Weight infill from the grid to texel (x, y).
    let ds = (1024 + bw / 2) / (bw - 1);
    let dt = (1024 + bh / 2) / (bh - 1);
    let gs = (ds * x * (gw - 1) + 32) >> 6;
    let gt = (dt * y * (gh - 1) + 32) >> 6;
    let (js, fs, jt, ft) = (gs >> 4, gs & 0xf, gt >> 4, gt & 0xf);
    let w11 = (fs * ft + 8) >> 4;
    let (w10, w01) = (ft - w11, fs - w11);
    let w00 = 16 - fs - ft + w11;
    let v0 = js + jt * gw;
    let weight = |plane: u32| -> u32 {
        let at = |v: u32| weights.get((v * planes + plane) as usize).copied().unwrap_or(0);
        (w00 * at(v0) + w01 * at(v0 + 1) + w10 * at(v0 + gw) + w11 * at(v0 + gw + 1) + 8) >> 4
    };
    let (p0, p1) = (weight(0), if dual { weight(1) } else { 0 });
    Some([0, 1, 2, 3].map(|k| {
        let w = if k == ccs { p1 } else { p0 } as i32;
        let (a, c) = (e0[k] * 257, e1[k] * 257);
        ((a * (64 - w) + c * w + 32) >> 6) as f32 / 65535.0
    }))
}
