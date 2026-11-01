//! ETC2 and EAC (Khronos Data Format Specification 1.3, "ETC2" and "EAC" sections).
//!
//! Blocks are 64-bit big-endian words. Texel indices inside a block run down columns, so pixel
//! `p = x * 4 + y`; `i` coming in is row-major (`y * 4 + x`) like every other format here.

fn word(b: &[u8]) -> u64 {
    u64::from_be_bytes(b[..8].try_into().unwrap())
}

fn field(w: u64, hi: u32, lo: u32) -> u32 {
    ((w >> lo) & ((1u64 << (hi - lo + 1)) - 1)) as u32
}

fn column_major(i: usize) -> u32 {
    ((i % 4) * 4 + i / 4) as u32
}

const MODIFIERS: [[i32; 2]; 8] = [[2, 8], [5, 17], [9, 29], [13, 42], [18, 60], [24, 80], [33, 106], [47, 183]];
const DISTANCES: [i32; 8] = [3, 6, 11, 16, 23, 32, 41, 64];

fn ext4(c: u32) -> i32 {
    ((c << 4) | c) as i32
}

fn ext5(c: u32) -> i32 {
    ((c << 3) | (c >> 2)) as i32
}

fn rgb(c: [i32; 3], a: f32) -> [f32; 4] {
    let f = |v: i32| v.clamp(0, 255) as f32 / 255.0;
    [f(c[0]), f(c[1]), f(c[2]), a]
}

/// ETC2 RGB, or RGB with punch-through alpha (`punch`).
pub(super) fn etc2_rgb(b: &[u8], i: usize, punch: bool) -> [f32; 4] {
    let w = word(b);
    let p = column_major(i);
    let (x, y) = ((i % 4) as i32, (i / 4) as i32);
    let msb = field(w, 16 + p, 16 + p);
    let lsb = field(w, p, p);
    let sel = (msb << 1) | lsb;
    let bit33 = field(w, 33, 33) == 1;
    // With punch-through, bit 33 is the opaque flag and the block is always differential.
    let (diff, opaque) = if punch { (true, bit33) } else { (bit33, true) };
    let transparent = !opaque && sel == 2;

    let base_and_table = |base: [i32; 3], cw: u32| -> [f32; 4] {
        if transparent {
            return [0.0; 4];
        }
        let m = MODIFIERS[cw as usize];
        let d = match sel {
            0 if !opaque => 0,
            0 => m[0],
            1 => m[1],
            2 => -m[0],
            _ => -m[1],
        };
        rgb(base.map(|c| c + d), 1.0)
    };
    let flip = field(w, 32, 32) == 1;
    let second = if flip { y >= 2 } else { x >= 2 };

    if !diff {
        let base = if second {
            [ext4(field(w, 59, 56)), ext4(field(w, 51, 48)), ext4(field(w, 43, 40))]
        } else {
            [ext4(field(w, 63, 60)), ext4(field(w, 55, 52)), ext4(field(w, 47, 44))]
        };
        let cw = if second { field(w, 36, 34) } else { field(w, 39, 37) };
        return base_and_table(base, cw);
    }

    let r = field(w, 63, 59) as i32;
    let g = field(w, 55, 51) as i32;
    let bl = field(w, 47, 43) as i32;
    let sx = |v: u32| ((v << 29) as i32) >> 29;
    let (dr, dg, db) = (sx(field(w, 58, 56)), sx(field(w, 50, 48)), sx(field(w, 42, 40)));

    if !(0..32).contains(&(r + dr)) {
        // T mode.
        let c0 = [ext4((field(w, 60, 59) << 2) | field(w, 57, 56)), ext4(field(w, 55, 52)), ext4(field(w, 51, 48))];
        let c1 = [ext4(field(w, 47, 44)), ext4(field(w, 43, 40)), ext4(field(w, 39, 36))];
        let d = DISTANCES[((field(w, 35, 34) << 1) | field(w, 32, 32)) as usize];
        if transparent {
            return [0.0; 4];
        }
        return rgb(
            match sel {
                0 => c0,
                1 => c1.map(|c| c + d),
                2 => c1,
                _ => c1.map(|c| c - d),
            },
            1.0,
        );
    }
    if !(0..32).contains(&(g + dg)) {
        // H mode.
        let r1 = field(w, 62, 59);
        let g1 = (field(w, 58, 56) << 1) | field(w, 52, 52);
        let b1 = (field(w, 51, 51) << 3) | field(w, 49, 47);
        let (r2, g2, b2) = (field(w, 46, 43), field(w, 42, 39), field(w, 38, 35));
        let order = ((r1 << 8) | (g1 << 4) | b1) >= ((r2 << 8) | (g2 << 4) | b2);
        let d = DISTANCES[((field(w, 34, 34) << 2) | (field(w, 32, 32) << 1) | order as u32) as usize];
        let (c0, c1) = ([ext4(r1), ext4(g1), ext4(b1)], [ext4(r2), ext4(g2), ext4(b2)]);
        if transparent {
            return [0.0; 4];
        }
        return rgb(
            match sel {
                0 => c0.map(|c| c + d),
                1 => c0.map(|c| c - d),
                2 => c1.map(|c| c + d),
                _ => c1.map(|c| c - d),
            },
            1.0,
        );
    }
    if !(0..32).contains(&(bl + db)) {
        // Planar mode: always opaque.
        let e6 = |c: u32| ((c << 2) | (c >> 4)) as i32;
        let e7 = |c: u32| ((c << 1) | (c >> 6)) as i32;
        let o = [
            e6(field(w, 62, 57)),
            e7((field(w, 56, 56) << 6) | field(w, 54, 49)),
            e6((field(w, 48, 48) << 5) | (field(w, 44, 43) << 3) | field(w, 41, 39)),
        ];
        let h = [e6((field(w, 38, 34) << 1) | field(w, 32, 32)), e7(field(w, 31, 25)), e6(field(w, 24, 19))];
        let v = [e6(field(w, 18, 13)), e7(field(w, 12, 6)), e6(field(w, 5, 0))];
        let c = [0, 1, 2].map(|k| (x * (h[k] - o[k]) + y * (v[k] - o[k]) + 4 * o[k] + 2) >> 2);
        return rgb(c, 1.0);
    }
    let base = if second { [ext5((r + dr) as u32), ext5((g + dg) as u32), ext5((bl + db) as u32)] } else { [ext5(r as u32), ext5(g as u32), ext5(bl as u32)] };
    let cw = if second { field(w, 36, 34) } else { field(w, 39, 37) };
    base_and_table(base, cw)
}

const EAC_TABLES: [[i32; 8]; 16] = [
    [-3, -6, -9, -15, 2, 5, 8, 14],
    [-3, -7, -10, -13, 2, 6, 9, 12],
    [-2, -5, -8, -13, 1, 4, 7, 12],
    [-2, -4, -6, -13, 1, 3, 5, 12],
    [-3, -6, -8, -12, 2, 5, 7, 11],
    [-3, -7, -9, -11, 2, 6, 8, 10],
    [-4, -7, -8, -11, 3, 6, 7, 10],
    [-3, -5, -8, -11, 2, 4, 7, 10],
    [-2, -6, -8, -10, 1, 5, 7, 9],
    [-2, -5, -8, -10, 1, 4, 7, 9],
    [-2, -4, -8, -10, 1, 3, 7, 9],
    [-2, -5, -7, -10, 1, 4, 6, 9],
    [-3, -4, -7, -10, 2, 3, 6, 9],
    [-1, -2, -3, -10, 0, 1, 2, 9],
    [-4, -6, -8, -9, 3, 5, 7, 8],
    [-3, -5, -7, -9, 2, 4, 6, 8],
];

fn eac_parts(b: &[u8], i: usize) -> (u32, i32, i32) {
    let w = word(b);
    let p = column_major(i);
    let sel = field(w, 47 - 3 * p, 45 - 3 * p);
    (field(w, 63, 56), field(w, 55, 52) as i32, EAC_TABLES[field(w, 51, 48) as usize][sel as usize])
}

/// ETC2 alpha (the first half of an ETC2 RGBA block), 8-bit precision.
pub(super) fn etc2_alpha(b: &[u8], i: usize) -> f32 {
    let (base, mult, t) = eac_parts(b, i);
    (base as i32 + t * mult).clamp(0, 255) as f32 / 255.0
}

/// One EAC R11 channel, unsigned or signed.
pub(super) fn eac(b: &[u8], i: usize, signed: bool) -> f32 {
    let (base, mult, t) = eac_parts(b, i);
    let step = if mult == 0 { t } else { t * mult * 8 };
    if signed {
        let base = (base as u8 as i8).max(-127) as i32;
        (base * 8 + step).clamp(-1023, 1023) as f32 / 1023.0
    } else {
        (base as i32 * 8 + 4 + step).clamp(0, 2047) as f32 / 2047.0
    }
}
