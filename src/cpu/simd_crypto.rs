//! FEAT_AES, FEAT_SHA1, FEAT_SHA2 and FEAT_SHA512 vector instructions, written from the Arm ARM
//! pseudocode. Every function takes and returns whole 128-bit `V` registers (element 0 in the low
//! bits) and is pure, so the interpreter and the tests share one definition.
use super::instructions::CryptoOp;

// ---------------------------------------------------------------- AES

const fn gen_sbox() -> [u8; 256] {
    let mut sbox = [0u8; 256];
    let (mut p, mut q) = (1u8, 1u8);
    loop {
        // p *= 3 and q /= 3 walk the multiplicative group of GF(2^8) in lock step.
        p = p ^ (p << 1) ^ if p & 0x80 != 0 { 0x1b } else { 0 };
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4);
        sbox[p as usize] = x ^ 0x63;
        if p == 1 {
            break;
        }
    }
    sbox[0] = 0x63;
    sbox
}

const fn invert(sbox: &[u8; 256]) -> [u8; 256] {
    let mut inv = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        inv[sbox[i] as usize] = i as u8;
        i += 1;
    }
    inv
}

const SBOX: [u8; 256] = gen_sbox();
const INV_SBOX: [u8; 256] = invert(&SBOX);

/// Source byte for each output byte of `ShiftRows` / `InvShiftRows` (state byte `i` is column `i / 4`, row `i % 4`).
const SHIFT_ROWS: [usize; 16] = [0, 5, 10, 15, 4, 9, 14, 3, 8, 13, 2, 7, 12, 1, 6, 11];
const INV_SHIFT_ROWS: [usize; 16] = [0, 13, 10, 7, 4, 1, 14, 11, 8, 5, 2, 15, 12, 9, 6, 3];

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0 }
}

/// GF(2^8) multiply by a small constant.
fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut r = 0;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    r
}

fn mix_columns(state: [u8; 16], m: [u8; 4]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for c in 0..4 {
        let col = &state[c * 4..c * 4 + 4];
        for r in 0..4 {
            // Row r of the circulant matrix [m0 m1 m2 m3] rotated right by r.
            out[c * 4 + r] = (0..4).fold(0, |acc, k| acc ^ gmul(col[k], m[(4 + k - r) % 4]));
        }
    }
    out
}

/// `AESE Vd.16B, Vn.16B`: `AddRoundKey` (EOR), `ShiftRows`, `SubBytes`.
pub fn aese(d: u128, n: u128) -> u128 {
    let s = (d ^ n).to_le_bytes();
    u128::from_le_bytes(std::array::from_fn(|i| SBOX[s[SHIFT_ROWS[i]] as usize]))
}

/// `AESD Vd.16B, Vn.16B`: `AddRoundKey` (EOR), `InvShiftRows`, `InvSubBytes`.
pub fn aesd(d: u128, n: u128) -> u128 {
    let s = (d ^ n).to_le_bytes();
    u128::from_le_bytes(std::array::from_fn(|i| INV_SBOX[s[INV_SHIFT_ROWS[i]] as usize]))
}

/// `AESMC Vd.16B, Vn.16B`: `MixColumns`.
pub fn aesmc(n: u128) -> u128 {
    u128::from_le_bytes(mix_columns(n.to_le_bytes(), [2, 3, 1, 1]))
}

/// `AESIMC Vd.16B, Vn.16B`: `InvMixColumns`.
pub fn aesimc(n: u128) -> u128 {
    u128::from_le_bytes(mix_columns(n.to_le_bytes(), [14, 11, 13, 9]))
}

// ---------------------------------------------------------------- SHA-1 / SHA-256

fn word(x: u128, i: u32) -> u32 {
    (x >> (32 * i)) as u32
}

fn from_words(w: [u32; 4]) -> u128 {
    w.iter().rev().fold(0, |acc, &x| (acc << 32) | u128::from(x))
}

fn choose(x: u32, y: u32, z: u32) -> u32 {
    (x & (y ^ z)) ^ z
}

fn parity(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}

fn majority(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | ((x | y) & z)
}

/// `SHA1C`, `SHA1P` and `SHA1M` (`Qd`, `Sn`, `Vm.4S`): four SHA-1 rounds with the chosen boolean function.
fn sha1_hash(d: u128, n: u128, m: u128, f: fn(u32, u32, u32) -> u32) -> u128 {
    let mut x = d;
    let mut y = word(n, 0);
    for e in 0..4 {
        let t = f(word(x, 1), word(x, 2), word(x, 3));
        y = y
            .wrapping_add(word(x, 0).rotate_left(5))
            .wrapping_add(t)
            .wrapping_add(word(m, e));
        let w1 = word(x, 1).rotate_left(30);
        x = (x & !(0xffff_ffffu128 << 32)) | (u128::from(w1) << 32);
        // <Y, X> = ROL(Y:X, 32)
        let carry = word(x, 3);
        x = (x << 32) | u128::from(y);
        y = carry;
    }
    x
}

/// `SHA1H Sd, Sn`: rotate the low word left by 30; the rest of `Vd` is zeroed.
pub fn sha1h(n: u128) -> u128 {
    u128::from(word(n, 0).rotate_left(30))
}

/// `SHA1SU0 Vd.4S, Vn.4S, Vm.4S`.
pub fn sha1su0(d: u128, n: u128, m: u128) -> u128 {
    let t = (n << 64) | (d >> 64);
    t ^ d ^ m
}

/// `SHA1SU1 Vd.4S, Vn.4S`.
pub fn sha1su1(d: u128, n: u128) -> u128 {
    let t = d ^ (n >> 32);
    from_words([
        word(t, 0).rotate_left(1),
        word(t, 1).rotate_left(1),
        word(t, 2).rotate_left(1),
        word(t, 3).rotate_left(1) ^ word(t, 0).rotate_left(2),
    ])
}

fn sigma0_big(x: u32) -> u32 {
    x.rotate_right(2) ^ x.rotate_right(13) ^ x.rotate_right(22)
}

fn sigma1_big(x: u32) -> u32 {
    x.rotate_right(6) ^ x.rotate_right(11) ^ x.rotate_right(25)
}

/// `SHA256hash(X, Y, W, part1)`: four SHA-256 rounds. `SHA256H` calls it with `(Qd, Qn)`, `SHA256H2` with `(Qn, Qd)`.
fn sha256_hash(x: u128, y: u128, w: u128, part1: bool) -> u128 {
    let (mut x, mut y) = (x, y);
    for e in 0..4 {
        let chs = choose(word(y, 0), word(y, 1), word(y, 2));
        let maj = majority(word(x, 0), word(x, 1), word(x, 2));
        let t = word(y, 3)
            .wrapping_add(sigma1_big(word(y, 0)))
            .wrapping_add(chs)
            .wrapping_add(word(w, e));
        let new_x3 = t.wrapping_add(word(x, 3));
        let new_y3 = t.wrapping_add(sigma0_big(word(x, 0))).wrapping_add(maj);
        x = (x & !(0xffff_ffffu128 << 96)) | (u128::from(new_x3) << 96);
        y = (y & !(0xffff_ffffu128 << 96)) | (u128::from(new_y3) << 96);
        // The new A lands in Y<127:96> and the new E in X<127:96>: shift both registers up one word and
        // swap the words that fall off (<Y, X> = ROL(Y:X, 32)).
        let (xh, yh) = (word(x, 3), word(y, 3));
        x = (x << 32) | u128::from(yh);
        y = (y << 32) | u128::from(xh);
    }
    if part1 { x } else { y }
}

/// `SHA256SU0 Vd.4S, Vn.4S`.
pub fn sha256su0(d: u128, n: u128) -> u128 {
    let t = (u128::from(word(n, 0)) << 96) | (d >> 32);
    from_words(std::array::from_fn(|e| {
        let x = word(t, e as u32);
        (x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)).wrapping_add(word(d, e as u32))
    }))
}

/// `SHA256SU1 Vd.4S, Vn.4S, Vm.4S`.
pub fn sha256su1(d: u128, n: u128, m: u128) -> u128 {
    let t0 = (u128::from(word(m, 0)) << 96) | (n >> 32);
    let small = |x: u32| x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10);
    let mut r = [0u32; 4];
    let mut src = [word(m, 2), word(m, 3), 0, 0];
    for e in 0..4 {
        if e >= 2 {
            src[e] = r[e - 2];
        }
        r[e] = small(src[e])
            .wrapping_add(word(d, e as u32))
            .wrapping_add(word(t0, e as u32));
    }
    from_words(r)
}

// ---------------------------------------------------------------- SHA-512

fn lanes(x: u128) -> [u64; 2] {
    [x as u64, (x >> 64) as u64]
}

fn pair(lo: u64, hi: u64) -> u128 {
    u128::from(lo) | (u128::from(hi) << 64)
}

fn ch64(x: u64, y: u64, z: u64) -> u64 {
    (x & y) ^ (!x & z)
}

fn maj64(x: u64, y: u64, z: u64) -> u64 {
    (x & y) ^ (x & z) ^ (y & z)
}

fn big_sigma0(x: u64) -> u64 {
    x.rotate_right(28) ^ x.rotate_right(34) ^ x.rotate_right(39)
}

fn big_sigma1(x: u64) -> u64 {
    x.rotate_right(14) ^ x.rotate_right(18) ^ x.rotate_right(41)
}

/// `SHA512H Qd, Qn, Vm.2D`.
pub fn sha512h(d: u128, n: u128, m: u128) -> u128 {
    let ([d0, d1], [n0, n1], [m0, m1]) = (lanes(d), lanes(n), lanes(m));
    let r1 = d1.wrapping_add(big_sigma1(m1)).wrapping_add(ch64(m1, n0, n1));
    let t = r1.wrapping_add(m0);
    let r0 = d0.wrapping_add(big_sigma1(t)).wrapping_add(ch64(t, m1, n0));
    pair(r0, r1)
}

/// `SHA512H2 Qd, Qn, Vm.2D`.
pub fn sha512h2(d: u128, n: u128, m: u128) -> u128 {
    let ([d0, d1], [n0, _], [m0, m1]) = (lanes(d), lanes(n), lanes(m));
    let r1 = d1.wrapping_add(big_sigma0(m0)).wrapping_add(maj64(n0, m1, m0));
    let r0 = d0.wrapping_add(big_sigma0(r1)).wrapping_add(maj64(r1, m0, m1));
    pair(r0, r1)
}

/// `SHA512SU0 Vd.2D, Vn.2D`.
pub fn sha512su0(d: u128, n: u128) -> u128 {
    let small = |x: u64| x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
    let ([d0, d1], [n0, _]) = (lanes(d), lanes(n));
    pair(d0.wrapping_add(small(d1)), d1.wrapping_add(small(n0)))
}

/// `SHA512SU1 Vd.2D, Vn.2D, Vm.2D`.
pub fn sha512su1(d: u128, n: u128, m: u128) -> u128 {
    let small = |x: u64| x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6);
    let ([d0, d1], [n0, n1], [m0, m1]) = (lanes(d), lanes(n), lanes(m));
    pair(
        d0.wrapping_add(small(n0)).wrapping_add(m0),
        d1.wrapping_add(small(n1)).wrapping_add(m1),
    )
}

/// Computes `Vd` for a crypto instruction from the current `Vd`, `Vn` and `Vm`. Two-register
/// forms ignore `m`; `AESMC`, `AESIMC` and `SHA1H` ignore `d`.
pub fn execute(op: CryptoOp, d: u128, n: u128, m: u128) -> u128 {
    match op {
        CryptoOp::Aese => aese(d, n),
        CryptoOp::Aesd => aesd(d, n),
        CryptoOp::Aesmc => aesmc(n),
        CryptoOp::Aesimc => aesimc(n),
        CryptoOp::Sha1c => sha1_hash(d, n, m, choose),
        CryptoOp::Sha1p => sha1_hash(d, n, m, parity),
        CryptoOp::Sha1m => sha1_hash(d, n, m, majority),
        CryptoOp::Sha1h => sha1h(n),
        CryptoOp::Sha1su0 => sha1su0(d, n, m),
        CryptoOp::Sha1su1 => sha1su1(d, n),
        CryptoOp::Sha256h => sha256_hash(d, n, m, true),
        CryptoOp::Sha256h2 => sha256_hash(n, d, m, false),
        CryptoOp::Sha256su0 => sha256su0(d, n),
        CryptoOp::Sha256su1 => sha256su1(d, n, m),
        CryptoOp::Sha512h => sha512h(d, n, m),
        CryptoOp::Sha512h2 => sha512h2(d, n, m),
        CryptoOp::Sha512su0 => sha512su0(d, n),
        CryptoOp::Sha512su1 => sha512su1(d, n, m),
    }
}

/// Decodes the crypto encoding space; returns `(op, rd, rn, rm)`.
pub fn decode(raw: u32) -> Option<(CryptoOp, u8, u8, u8)> {
    let two_reg = match raw & 0xFFFF_FC00 {
        0x4E28_4800 => Some(CryptoOp::Aese),
        0x4E28_5800 => Some(CryptoOp::Aesd),
        0x4E28_6800 => Some(CryptoOp::Aesmc),
        0x4E28_7800 => Some(CryptoOp::Aesimc),
        0x5E28_0800 => Some(CryptoOp::Sha1h),
        0x5E28_1800 => Some(CryptoOp::Sha1su1),
        0x5E28_2800 => Some(CryptoOp::Sha256su0),
        0xCEC0_8000 => Some(CryptoOp::Sha512su0),
        _ => None,
    };
    let rm = if two_reg.is_some() { 0 } else { ((raw >> 16) & 31) as u8 };
    let op = two_reg.or(match raw & 0xFFE0_FC00 {
        0x5E00_0000 => Some(CryptoOp::Sha1c),
        0x5E00_1000 => Some(CryptoOp::Sha1p),
        0x5E00_2000 => Some(CryptoOp::Sha1m),
        0x5E00_3000 => Some(CryptoOp::Sha1su0),
        0x5E00_4000 => Some(CryptoOp::Sha256h),
        0x5E00_5000 => Some(CryptoOp::Sha256h2),
        0x5E00_6000 => Some(CryptoOp::Sha256su1),
        0xCE60_8000 => Some(CryptoOp::Sha512h),
        0xCE60_8400 => Some(CryptoOp::Sha512h2),
        0xCE60_8800 => Some(CryptoOp::Sha512su1),
        _ => None,
    })?;
    Some((op, (raw & 31) as u8, ((raw >> 5) & 31) as u8, rm))
}
