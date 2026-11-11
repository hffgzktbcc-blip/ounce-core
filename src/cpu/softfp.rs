//! Integer soft-float for the AArch64 FPCR modes the host FPU cannot give us: rounding mode (`RMode`),
//! flush-to-zero (`FZ`) and default-NaN (`DN`). Safe Rust, no host floating point, so results do not depend on the
//! machine the emulator runs on.
//!
//! The interpreter keeps its fast host-float paths for the default FPCR (RN, FZ=0, DN=0) and calls in here only when
//! one of the three controls is set. Semantics follow the Arm ARM pseudocode (`FPAdd`, `FPMul`, `FPDiv`, `FPSqrt`,
//! `FPMulAdd`, `FPRound`, `FPProcessNaNs`, `FPUnpack`). The cumulative exception flags (FPSR.IOC, DZC, OFC, UFC, IXC,
//! IDC) are raised as the pseudocode's `FPProcessException` does, into a per-thread accumulator that the interpreter
//! drains into `FPSR` after each instruction ([`take_flags`]); trap enables (FPCR.IOE..IDE) are not modelled.
//! Operands and results are raw bit patterns in the low bits of a `u64`.

use std::cell::Cell;

/// FPSR cumulative exception bits.
pub const IOC: u32 = 1;
pub const DZC: u32 = 1 << 1;
pub const OFC: u32 = 1 << 2;
pub const UFC: u32 = 1 << 3;
pub const IXC: u32 = 1 << 4;
pub const IDC: u32 = 1 << 7;

thread_local! {
    static FLAGS: Cell<u32> = const { Cell::new(0) };
}

/// Records exception flags (`FPProcessException`).
pub fn raise(flags: u32) {
    FLAGS.with(|c| c.set(c.get() | flags));
}

/// Returns and clears the flags raised on this thread since the last call.
pub fn take_flags() -> u32 {
    FLAGS.with(|c| c.replace(0))
}

/// FPCR bits used here.
pub const FPCR_FZ: u32 = 1 << 24;
pub const FPCR_DN: u32 = 1 << 25;
pub const FPCR_RMODE_SHIFT: u32 = 22;
/// FZ, DN and RMode: any of these set means the soft path is needed.
pub const FPCR_ARITH_MASK: u32 = FPCR_FZ | FPCR_DN | (3 << FPCR_RMODE_SHIFT);

/// FPCR.RMode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Round {
    /// Round to nearest, ties to even.
    Nearest,
    /// Towards +infinity.
    Up,
    /// Towards -infinity.
    Down,
    /// Towards zero.
    Zero,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Env {
    pub round: Round,
    pub fz: bool,
    pub dn: bool,
}

impl Env {
    pub const DEFAULT: Env = Env { round: Round::Nearest, fz: false, dn: false };

    pub fn from_fpcr(fpcr: u32) -> Env {
        let round = match (fpcr >> FPCR_RMODE_SHIFT) & 3 {
            0 => Round::Nearest,
            1 => Round::Up,
            2 => Round::Down,
            _ => Round::Zero,
        };
        Env { round, fz: fpcr & FPCR_FZ != 0, dn: fpcr & FPCR_DN != 0 }
    }

    pub fn with_round(self, round: Round) -> Env {
        Env { round, ..self }
    }
}

/// An IEEE binary format: 32 or 64 bit here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fmt {
    ebits: u32,
    mbits: u32,
}

pub const F32: Fmt = Fmt { ebits: 8, mbits: 23 };
pub const F64: Fmt = Fmt { ebits: 11, mbits: 52 };

impl Fmt {
    pub fn from_esize(esize: u32) -> Fmt {
        if esize == 64 { F64 } else { F32 }
    }
    fn bias(self) -> i32 {
        (1 << (self.ebits - 1)) - 1
    }
    fn max_field(self) -> u64 {
        (1 << self.ebits) - 1
    }
    fn sign_bit(self) -> u64 {
        1 << (self.ebits + self.mbits)
    }
    fn frac_mask(self) -> u64 {
        (1 << self.mbits) - 1
    }
    fn quiet_bit(self) -> u64 {
        1 << (self.mbits - 1)
    }
    pub fn default_nan(self) -> u64 {
        (self.max_field() << self.mbits) | self.quiet_bit()
    }
    pub fn infinity(self, sign: bool) -> u64 {
        (self.max_field() << self.mbits) | if sign { self.sign_bit() } else { 0 }
    }
    fn zero(self, sign: bool) -> u64 {
        if sign { self.sign_bit() } else { 0 }
    }
    fn max_finite(self, sign: bool) -> u64 {
        ((self.max_field() - 1) << self.mbits) | self.frac_mask() | if sign { self.sign_bit() } else { 0 }
    }
    fn field(self, x: u64) -> u64 {
        (x >> self.mbits) & self.max_field()
    }
    fn frac(self, x: u64) -> u64 {
        x & self.frac_mask()
    }
    fn sign(self, x: u64) -> bool {
        x & self.sign_bit() != 0
    }
    pub fn is_nan(self, x: u64) -> bool {
        self.field(x) == self.max_field() && self.frac(x) != 0
    }
    pub fn is_snan(self, x: u64) -> bool {
        self.is_nan(x) && x & self.quiet_bit() == 0
    }
    pub fn is_inf(self, x: u64) -> bool {
        self.field(x) == self.max_field() && self.frac(x) == 0
    }
    pub fn is_zero(self, x: u64) -> bool {
        self.field(x) == 0 && self.frac(x) == 0
    }
    pub fn is_denormal(self, x: u64) -> bool {
        self.field(x) == 0 && self.frac(x) != 0
    }
    pub fn neg(self, x: u64) -> u64 {
        x ^ self.sign_bit()
    }
    pub fn abs(self, x: u64) -> u64 {
        x & !self.sign_bit()
    }
}

/// A finite non-zero operand as `mant * 2^exp` with `mant` in `[2^63, 2^64)`.
#[derive(Clone, Copy)]
struct Fin {
    sign: bool,
    mant: u64,
    exp: i32,
}

fn unpack(f: Fmt, x: u64) -> Fin {
    let (field, frac) = (f.field(x), f.frac(x));
    let (m, e) = if field == 0 {
        (frac, 1 - f.bias() - f.mbits as i32)
    } else {
        (frac | (1 << f.mbits), field as i32 - f.bias() - f.mbits as i32)
    };
    let lz = m.leading_zeros();
    Fin { sign: f.sign(x), mant: m << lz, exp: e - lz as i32 }
}

/// FPUnpack's flush: with FZ a denormal operand (single or double) reads as a zero of the same sign.
/// A flushed denormal raises IDC.
pub fn flush_input(f: Fmt, env: Env, x: u64) -> u64 {
    if env.fz && f.is_denormal(x) {
        raise(IDC);
        x & f.sign_bit()
    } else {
        x
    }
}

/// Rounds `(-1)^sign * (m + s) * 2^e` where `0 <= s < 1` is the sticky remainder below `m`'s last bit (`sticky`).
/// `m` may be any non-zero value. Implements FPRound: flush-to-zero of tiny results, rounding mode, overflow.
fn round_pack(f: Fmt, env: Env, sign: bool, e: i32, m: u128, sticky: bool) -> u64 {
    debug_assert!(m != 0);
    let lz = m.leading_zeros();
    let m = m << lz;
    // value = m * 2^(e - lz) with m in [2^127, 2^128); exponent of the leading bit:
    let top = e - lz as i32 + 127;
    let emin = 1 - f.bias();
    let tiny = top < emin;
    if tiny && env.fz {
        // FPRound flushes a tiny result to zero: UFC, and no IXC
        raise(UFC);
        return f.zero(sign);
    }
    let p = f.mbits as i32 + 1;
    let keep = if tiny { p - (emin - top) } else { p };
    // bits of `m` kept: its top `keep` bits (keep may be <= 0 for values below the smallest denormal)
    let (q, round_bit, rest) = if keep > 0 {
        let drop = 128 - keep as u32;
        let q = m >> drop;
        let round_bit = (m >> (drop - 1)) & 1 == 1;
        let rest = m & ((1u128 << (drop - 1)) - 1) != 0 || sticky;
        (q, round_bit, rest)
    } else if keep == 0 {
        (0, true, (m << 1) != 0 || sticky)
    } else {
        (0, false, true)
    };
    let inexact = round_bit || rest;
    // tininess is detected before rounding; underflow needs an inexact result
    if inexact {
        raise(if tiny { UFC | IXC } else { IXC });
    }
    let up = match env.round {
        Round::Nearest => round_bit && (rest || q & 1 == 1),
        Round::Up => !sign && inexact,
        Round::Down => sign && inexact,
        Round::Zero => false,
    };
    let q = q as u64 + up as u64;
    let field_base = if tiny { 0i64 } else { (top + f.bias()) as i64 - 1 };
    if !tiny {
        // q in [2^mbits, 2^(mbits+1)]: the top bit of q supplies the +1 of the exponent field (and the carry case)
        let field = field_base + (q >> f.mbits) as i64;
        if field >= f.max_field() as i64 {
            raise(OFC | IXC);
            return overflow(f, env, sign);
        }
        let bits = ((field_base as u64) << f.mbits).wrapping_add(q);
        return bits | f.zero(sign);
    }
    // denormal range: q < 2^mbits is a denormal fraction; q == 2^mbits rounded up to the smallest normal
    q | f.zero(sign)
}

fn overflow(f: Fmt, env: Env, sign: bool) -> u64 {
    let to_inf = match env.round {
        Round::Nearest => true,
        Round::Up => !sign,
        Round::Down => sign,
        Round::Zero => false,
    };
    if to_inf { f.infinity(sign) } else { f.max_finite(sign) }
}

/// FPProcessNaNs over the operands in order: the first signalling NaN (quieted), else the first quiet NaN.
fn process_nans(f: Fmt, ops: &[u64]) -> Option<u64> {
    if let Some(&x) = ops.iter().find(|&&x| f.is_snan(x)) {
        raise(IOC);
        return Some(x | f.quiet_bit());
    }
    ops.iter().find(|&&x| f.is_nan(x)).copied()
}

/// DN turns a NaN result into the default NaN.
fn nan_result(f: Fmt, env: Env, r: u64) -> u64 {
    if env.dn { f.default_nan() } else { r }
}

/// Sign of an exact-zero sum of opposite-signed operands or of two zeros: +0, or -0 when rounding down.
fn exact_zero_sum(f: Fmt, env: Env, sa: bool, sb: bool) -> u64 {
    if sa == sb { f.zero(sa) } else { f.zero(env.round == Round::Down) }
}

/// Exact sum of two non-zero magnitudes, both with their top bit at bit 126 (`value = m * 2^e`); rounds it.
/// `sticky_b` marks that `b` is only a lower bound of the true addend (never used by callers today, kept for clarity).
fn add_aligned(f: Fmt, env: Env, (sa, ma, ea): (bool, u128, i32), (sb, mb, eb): (bool, u128, i32)) -> u64 {
    // order by magnitude: x is the larger
    let (x, y) = if ea > eb || (ea == eb && ma >= mb) { ((sa, ma, ea), (sb, mb, eb)) } else { ((sb, mb, eb), (sa, ma, ea)) };
    let d = (x.2 - y.2) as u32;
    let (small, sticky) = if d == 0 {
        (y.1, false)
    } else if d >= 128 {
        (0, true)
    } else {
        (y.1 >> d, y.1 & ((1u128 << d) - 1) != 0)
    };
    if x.0 == y.0 {
        return round_pack(f, env, x.0, x.2, x.1 + small, sticky);
    }
    let diff = x.1 - small - sticky as u128;
    if diff == 0 && !sticky {
        return exact_zero_sum(f, env, sa, sb);
    }
    round_pack(f, env, x.0, x.2, diff, sticky)
}

/// A finite non-zero operand placed with its top bit at 126.
fn aligned(x: Fin) -> (bool, u128, i32) {
    (x.sign, (x.mant as u128) << 63, x.exp - 63)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// FMULX: like Mul, but 0 * inf is +-2.0.
    Mulx,
}

/// `FPAdd`/`FPSub`/`FPMul`/`FPDiv`/`FPMulX` of two operands.
pub fn binop(f: Fmt, env: Env, op: BinOp, a: u64, b: u64) -> u64 {
    let (a, b) = (flush_input(f, env, a), flush_input(f, env, b));
    if let Some(r) = process_nans(f, &[a, b]) {
        return nan_result(f, env, r);
    }
    let b_eff = if op == BinOp::Sub { f.neg(b) } else { b };
    let (a_inf, b_inf, a_zero, b_zero) = (f.is_inf(a), f.is_inf(b), f.is_zero(a), f.is_zero(b));
    let (sa, sb) = (f.sign(a), f.sign(b_eff));
    match op {
        BinOp::Add | BinOp::Sub => {
            if a_inf && b_inf {
                if sa == sb {
                    return a;
                }
                raise(IOC);
                return f.default_nan();
            }
            if a_inf {
                return a;
            }
            if b_inf {
                return b_eff;
            }
            if a_zero && b_zero {
                return exact_zero_sum(f, env, sa, sb);
            }
            if a_zero {
                return b_eff;
            }
            if b_zero {
                return a;
            }
            add_aligned(f, env, aligned(unpack(f, a)), aligned(unpack(f, b_eff)))
        }
        BinOp::Mul | BinOp::Mulx => {
            let sign = sa != sb;
            if (a_inf && b_zero) || (a_zero && b_inf) {
                if op == BinOp::Mulx {
                    return f.neg_if(f.two(), sign);
                }
                raise(IOC);
                return f.default_nan();
            }
            if a_inf || b_inf {
                return f.infinity(sign);
            }
            if a_zero || b_zero {
                return f.zero(sign);
            }
            let (x, y) = (unpack(f, a), unpack(f, b));
            round_pack(f, env, sign, x.exp + y.exp, x.mant as u128 * y.mant as u128, false)
        }
        BinOp::Div => {
            let sign = sa != sb;
            if (a_inf && b_inf) || (a_zero && b_zero) {
                raise(IOC);
                return f.default_nan();
            }
            if a_inf || b_zero {
                if !a_inf {
                    raise(DZC);
                }
                return f.infinity(sign);
            }
            if a_zero || b_inf {
                return f.zero(sign);
            }
            let (x, y) = (unpack(f, a), unpack(f, b));
            let num = (x.mant as u128) << 64;
            let (q, r) = (num / y.mant as u128, num % y.mant as u128);
            round_pack(f, env, sign, x.exp - y.exp - 64, q, r != 0)
        }
    }
}

impl Fmt {
    fn two(self) -> u64 {
        ((self.bias() as u64 + 1) << self.mbits) as u64
    }
    fn neg_if(self, x: u64, c: bool) -> u64 {
        if c { self.neg(x) } else { x }
    }
}

/// `FPMulAdd(a, n, m)`: `a + n * m` with one rounding. NaN order: a quiet-NaN addend with 0 * inf gives the default
/// NaN, otherwise the NaN of the addend, then `n`, then `m` (signalling first).
pub fn fma(f: Fmt, env: Env, n: u64, m: u64, a: u64) -> u64 {
    let (n, m, a) = (flush_input(f, env, n), flush_input(f, env, m), flush_input(f, env, a));
    let (n_inf, m_inf, n_zero, m_zero) = (f.is_inf(n), f.is_inf(m), f.is_zero(n), f.is_zero(m));
    let zero_times_inf = (n_inf && m_zero) || (n_zero && m_inf);
    if f.is_nan(a) && !f.is_snan(a) && zero_times_inf {
        raise(IOC);
        return nan_result(f, env, f.default_nan());
    }
    if let Some(r) = process_nans(f, &[a, n, m]) {
        return nan_result(f, env, r);
    }
    if zero_times_inf {
        raise(IOC);
        return f.default_nan();
    }
    let sp = f.sign(n) != f.sign(m);
    let (a_inf, a_zero) = (f.is_inf(a), f.is_zero(a));
    if n_inf || m_inf {
        if a_inf && f.sign(a) != sp {
            raise(IOC);
            return f.default_nan();
        }
        return f.infinity(sp);
    }
    if a_inf {
        return a;
    }
    if n_zero || m_zero {
        // exact zero product
        if a_zero {
            return exact_zero_sum(f, env, f.sign(a), sp);
        }
        return a;
    }
    let (x, y) = (unpack(f, n), unpack(f, m));
    let prod = x.mant as u128 * y.mant as u128; // in [2^126, 2^128)
    let (pm, pe) = if prod >> 127 != 0 {
        // the low bit is zero: both mantissas have trailing zeros after unpacking (mbits < 63)
        (prod >> 1, x.exp + y.exp + 1)
    } else {
        (prod, x.exp + y.exp)
    };
    if a_zero {
        return round_pack(f, env, sp, pe, pm, false);
    }
    add_aligned(f, env, (sp, pm, pe), aligned(unpack(f, a)))
}

/// `FPSqrt`.
pub fn sqrt(f: Fmt, env: Env, a: u64) -> u64 {
    let a = flush_input(f, env, a);
    if f.is_nan(a) {
        if f.is_snan(a) {
            raise(IOC);
        }
        return nan_result(f, env, a | f.quiet_bit());
    }
    if f.is_zero(a) {
        return a;
    }
    if f.sign(a) {
        raise(IOC);
        return f.default_nan();
    }
    if f.is_inf(a) {
        return a;
    }
    let x = unpack(f, a);
    let s = 62 + x.exp.rem_euclid(2) as u32;
    let big = (x.mant as u128) << s; // exponent x.exp - s is even
    let r = isqrt(big);
    let sticky = r * r != big;
    round_pack(f, env, false, (x.exp - s as i32) / 2, r, sticky)
}

fn isqrt(n: u128) -> u128 {
    let mut rem = n;
    let mut root = 0u128;
    let mut bit = 1u128 << 126;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if rem >= root + bit {
            rem -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

/// `SCVTF`/`UCVTF`: an integer (`neg`, magnitude) divided by `2^fbits`, rounded under `env`. No flush applies to
/// the result of a conversion from an integer in the normal range; tiny results from large `fbits` follow `FPRoundCV`.
pub fn int_to_fp(f: Fmt, env: Env, neg: bool, mag: u64, fbits: u32) -> u64 {
    if mag == 0 {
        return f.zero(false);
    }
    round_pack(f, env, neg, -(fbits as i32), mag as u128, false)
}

/// `FCVT` between single and double. Widening is exact; narrowing rounds under `env`.
pub fn convert(from: Fmt, to: Fmt, env: Env, a: u64) -> u64 {
    // hardware (and the reference) flush a denormal input under FZ
    let a = flush_input(from, env, a);
    if from.is_nan(a) {
        if from.is_snan(a) {
            raise(IOC);
        }
        // FPConvert: signalling NaNs are quieted, the payload is kept (truncated when narrowing), DN gives the default
        if env.dn {
            return to.default_nan();
        }
        let frac = from.frac(a) | from.quiet_bit();
        let frac = if to.mbits >= from.mbits { frac << (to.mbits - from.mbits) } else { frac >> (from.mbits - to.mbits) };
        return (to.max_field() << to.mbits) | frac | to.zero(from.sign(a));
    }
    if from.is_inf(a) {
        return to.infinity(from.sign(a));
    }
    if from.is_zero(a) {
        return to.zero(from.sign(a));
    }
    let x = unpack(from, a);
    round_pack(to, env, x.sign, x.exp, x.mant as u128, false)
}

/// Flags of a float-to-integer conversion (`FPToFixed`). `exact` is the operand (times `2^fbits`) as an exact `f64`
/// and `rounded` that value rounded to an integer by the instruction's rounding mode. A NaN or an out-of-range
/// result is IOC (the result saturates); an in-range result that differs from `exact` is IXC.
pub fn raise_to_int(f: Fmt, a: u64, exact: f64, rounded: f64, signed: bool, is_64: bool) {
    if f.is_nan(a) {
        raise(IOC);
        return;
    }
    let (lo, hi) = match (signed, is_64) {
        (true, true) => (-9223372036854775808.0, 9223372036854775808.0),
        (true, false) => (-2147483648.0, 2147483648.0),
        (false, true) => (0.0, 18446744073709551616.0),
        (false, false) => (0.0, 4294967296.0),
    };
    if !(rounded >= lo && rounded < hi) {
        raise(IOC);
    } else if rounded != exact {
        raise(IXC);
    }
}

/// `FCMP`/`FCMPE`: a signalling NaN is always IOC; with `signalling` (FCMPE) a quiet NaN is too.
pub fn raise_compare(f: Fmt, a: u64, b: u64, signalling: bool) {
    if f.is_snan(a) || f.is_snan(b) || (signalling && (f.is_nan(a) || f.is_nan(b))) {
        raise(IOC);
    }
}

/// FMAX/FMIN/FMAXNM/FMINNM: IOC for a signalling NaN operand (quiet NaNs raise nothing), IDC for a flushed denormal.
pub fn raise_minmax(f: Fmt, env: Env, a: u64, b: u64) {
    if f.is_snan(a) || f.is_snan(b) {
        raise(IOC);
    }
    let _ = (flush_input(f, env, a), flush_input(f, env, b));
}

/// `FRINT*`: a signalling NaN is IOC; with `inexact` (FRINTX only) a result that differs from the operand is IXC.
pub fn raise_round_int(f: Fmt, a: u64, exact: f64, rounded: f64, inexact: bool) {
    if f.is_snan(a) {
        raise(IOC);
    } else if inexact && !f.is_nan(a) && exact != rounded {
        raise(IXC);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xorshift: deterministic operand source
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// bit patterns biased to the interesting classes
        fn f64_bits(&mut self) -> u64 {
            let r = self.next();
            match r % 8 {
                0 => r & 0x800f_ffff_ffff_ffff,                  // denormal / zero
                1 => (r & 0x800f_ffff_ffff_ffff) | (0x3ff << 52), // near 1
                2 => (r & 0x800f_ffff_ffff_ffff) | ((r >> 20 & 0x7ff) << 52),
                3 => (r & 0x8000_0000_0000_0000) | (0x7fe << 52) | (r & 0xf_ffff_ffff_ffff), // huge
                4 => (r & 0x8000_0000_0000_0000) | ((self.next() % 130 + 960) << 52) | (r & 0xf_ffff_ffff_ffff),
                _ => r,
            }
        }
        fn f32_bits(&mut self) -> u32 {
            let r = self.next() as u32;
            match r % 8 {
                0 => r & 0x807f_ffff,
                1 => (r & 0x807f_ffff) | (0x7f << 23),
                2 => (r & 0x807f_ffff) | ((self.next() as u32 % 0xff) << 23),
                3 => (r & 0x8000_0000) | (0xfe << 23) | (r & 0x7f_ffff),
                _ => r,
            }
        }
    }

    fn same_f64(a: f64, b: u64) -> bool {
        if a.is_nan() { f64::from_bits(b).is_nan() } else { a.to_bits() == b }
    }
    fn same_f32(a: f32, b: u64) -> bool {
        if a.is_nan() { f32::from_bits(b as u32).is_nan() } else { a.to_bits() == b as u32 }
    }

    /// Round-to-nearest soft results must equal the host FPU bit for bit (NaNs compared as NaNs).
    #[test]
    fn nearest_matches_host_f64() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let env = Env::DEFAULT;
        for _ in 0..400_000 {
            let (a, b, c) = (rng.f64_bits(), rng.f64_bits(), rng.f64_bits());
            let (fa, fb, fc) = (f64::from_bits(a), f64::from_bits(b), f64::from_bits(c));
            assert!(same_f64(fa + fb, binop(F64, env, BinOp::Add, a, b)), "add {a:#x} {b:#x}");
            assert!(same_f64(fa - fb, binop(F64, env, BinOp::Sub, a, b)), "sub {a:#x} {b:#x}");
            assert!(same_f64(fa * fb, binop(F64, env, BinOp::Mul, a, b)), "mul {a:#x} {b:#x}");
            assert!(same_f64(fa / fb, binop(F64, env, BinOp::Div, a, b)), "div {a:#x} {b:#x}");
            assert!(same_f64(fa.mul_add(fb, fc), fma(F64, env, a, b, c)), "fma {a:#x} {b:#x} {c:#x}");
            assert!(same_f64(fa.sqrt(), sqrt(F64, env, a)), "sqrt {a:#x}");
        }
    }

    #[test]
    fn nearest_matches_host_f32() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let env = Env::DEFAULT;
        for _ in 0..400_000 {
            let (a, b, c) = (rng.f32_bits(), rng.f32_bits(), rng.f32_bits());
            let (fa, fb, fc) = (f32::from_bits(a), f32::from_bits(b), f32::from_bits(c));
            let (a, b, c) = (a as u64, b as u64, c as u64);
            assert!(same_f32(fa + fb, binop(F32, env, BinOp::Add, a, b)), "add {a:#x} {b:#x}");
            assert!(same_f32(fa - fb, binop(F32, env, BinOp::Sub, a, b)), "sub {a:#x} {b:#x}");
            assert!(same_f32(fa * fb, binop(F32, env, BinOp::Mul, a, b)), "mul {a:#x} {b:#x}");
            assert!(same_f32(fa / fb, binop(F32, env, BinOp::Div, a, b)), "div {a:#x} {b:#x}");
            assert!(same_f32(fa.mul_add(fb, fc), fma(F32, env, a, b, c)), "fma {a:#x} {b:#x} {c:#x}");
            assert!(same_f32(fa.sqrt(), sqrt(F32, env, a)), "sqrt {a:#x}");
        }
    }

    #[test]
    fn narrowing_and_widening_match_host() {
        let mut rng = Rng(0xdead_beef_cafe_f00d);
        for _ in 0..200_000 {
            let a = rng.f64_bits();
            assert!(same_f32(f64::from_bits(a) as f32, convert(F64, F32, Env::DEFAULT, a)), "narrow {a:#x}");
            let s = rng.f32_bits();
            assert!(same_f64(f32::from_bits(s) as f64, convert(F32, F64, Env::DEFAULT, s as u64)), "widen {s:#x}");
        }
    }

    #[test]
    fn int_to_fp_nearest_matches_host() {
        let mut rng = Rng(77);
        for _ in 0..100_000 {
            let v = (rng.next() >> (rng.next() % 64)) as i64;
            let (neg, mag) = (v < 0, v.unsigned_abs());
            assert_eq!(int_to_fp(F64, Env::DEFAULT, neg, mag, 0), (v as f64).to_bits(), "{v}");
            assert_eq!(int_to_fp(F32, Env::DEFAULT, neg, mag, 0), (v as f32).to_bits() as u64, "{v}");
            let u = v as u64;
            assert_eq!(int_to_fp(F64, Env::DEFAULT, false, u, 0), (u as f64).to_bits(), "{u}");
            assert_eq!(int_to_fp(F32, Env::DEFAULT, false, u, 0), (u as f32).to_bits() as u64, "{u}");
        }
    }

    /// For any finite exact result the directed modes bracket nearest: RM <= RN <= RP, RZ is the one of RM/RP closer
    /// to zero, and when RM == RP the operation was exact so all four agree. Checked on all five arithmetic ops.
    #[test]
    fn directed_modes_bracket_nearest() {
        let mut rng = Rng(0x0bad_5eed_1234_5678);
        let mk = |round| Env { round, fz: false, dn: false };
        let ordered = |x: u64, y: u64| -> bool {
            // x <= y as f64 values (finite)
            f64::from_bits(x) <= f64::from_bits(y)
        };
        for _ in 0..200_000 {
            let (a, b, c) = (rng.f64_bits(), rng.f64_bits(), rng.f64_bits());
            let results = |round| -> [u64; 5] {
                let e = mk(round);
                [
                    binop(F64, e, BinOp::Add, a, b),
                    binop(F64, e, BinOp::Mul, a, b),
                    binop(F64, e, BinOp::Div, a, b),
                    fma(F64, e, a, b, c),
                    sqrt(F64, e, a),
                ]
            };
            let (rn, rp, rm, rz) = (results(Round::Nearest), results(Round::Up), results(Round::Down), results(Round::Zero));
            for i in 0..5 {
                let (n, p, m, z) = (rn[i], rp[i], rm[i], rz[i]);
                if f64::from_bits(n).is_nan() {
                    assert!(f64::from_bits(p).is_nan() && f64::from_bits(m).is_nan() && f64::from_bits(z).is_nan());
                    continue;
                }
                assert!(ordered(m, n) && ordered(n, p), "op {i} {a:#x} {b:#x} {c:#x}: {m:#x} {n:#x} {p:#x}");
                // a result strictly between would have RM < RP; adjacent doubles differ by one ulp
                if m != p {
                    let (lo, hi) = (f64::from_bits(m), f64::from_bits(p));
                    if lo.is_finite() && hi.is_finite() && lo != 0.0 && hi != 0.0 && lo.signum() == hi.signum() {
                        assert_eq!(m.abs_diff(p), 1, "op {i} {a:#x} {b:#x} {c:#x}");
                    }
                    assert!(n == m || n == p, "RN must be one of the neighbours");
                    let zero_pick = if hi > 0.0 || (hi == 0.0 && p >> 63 == 0) { m } else { p };
                    assert_eq!(z, zero_pick, "op {i} RZ {a:#x} {b:#x} {c:#x}");
                } else {
                    assert!(n == m && z == m, "exact result: all modes agree");
                }
            }
        }
    }

    /// Flags raised by one soft operation.
    fn flags_of(f: impl FnOnce()) -> u32 {
        take_flags();
        f();
        take_flags()
    }

    /// The five cumulative exception flags against hand-derived Arm results (FPProcessException cases).
    #[test]
    fn exception_flags() {
        let d = Env::DEFAULT;
        let s = |op, a, b| flags_of(|| { binop(F32, d, op, a, b); });
        let (one, two, big, tiny, den) = (0x3f80_0000u64, 0x4000_0000u64, 0x7f7f_ffffu64, 0x0080_0000u64, 1u64);
        assert_eq!(s(BinOp::Add, one, one), 0, "exact");
        assert_eq!(s(BinOp::Add, one, 0x3380_0000), IXC, "1 + 2^-24 is inexact");
        assert_eq!(s(BinOp::Add, 0x7f80_0000, 0xff80_0000), IOC, "inf - inf");
        assert_eq!(s(BinOp::Mul, 0, 0x7f80_0000), IOC, "0 * inf");
        assert_eq!(s(BinOp::Mulx, 0, 0x7f80_0000), 0, "FMULX 0 * inf is +-2, no flag");
        assert_eq!(s(BinOp::Div, 0, 0), IOC, "0 / 0");
        assert_eq!(s(BinOp::Div, 0x7f80_0000, 0x7f80_0000), IOC, "inf / inf");
        assert_eq!(s(BinOp::Div, one, 0), DZC, "1 / 0");
        assert_eq!(s(BinOp::Div, 0x7f80_0000, 0), 0, "inf / 0 is exactly inf");
        assert_eq!(s(BinOp::Mul, big, two), OFC | IXC, "overflow is also inexact");
        assert_eq!(s(BinOp::Mul, tiny, 0x3f00_0000), 0, "min normal / 2 is an exact denormal: no UFC");
        assert_eq!(s(BinOp::Mul, tiny, 0x3eaa_aaab), UFC | IXC, "tiny and inexact");
        assert_eq!(s(BinOp::Add, 0x7fa0_0000, one), IOC, "signalling NaN operand");
        assert_eq!(s(BinOp::Add, 0x7fc0_0000, one), 0, "quiet NaN operand");
        let fz = Env { fz: true, ..d };
        assert_eq!(flags_of(|| { binop(F32, fz, BinOp::Add, den, one); }), IDC, "FZ: denormal input is IDC, 1 + 0 exact");
        assert_eq!(flags_of(|| { binop(F32, fz, BinOp::Mul, tiny, 0x3f00_0000); }), UFC, "FZ: tiny result flushed, UFC without IXC");
        assert_eq!(flags_of(|| { sqrt(F32, d, 0xbf80_0000); }), IOC, "sqrt(-1)");
        assert_eq!(flags_of(|| { sqrt(F32, d, 0x4000_0000); }), IXC, "sqrt(2)");
        assert_eq!(flags_of(|| { sqrt(F32, d, 0x8000_0000); }), 0, "sqrt(-0)");
        assert_eq!(flags_of(|| { fma(F32, d, 0, 0x7f80_0000, 0x7fc0_0000); }), IOC, "0 * inf + qNaN");
        assert_eq!(flags_of(|| { convert(F64, F32, d, 0x47f0_0000_0000_0000); }), OFC | IXC, "double to single overflow");
        assert_eq!(flags_of(|| { convert(F32, F64, d, 0x7fa0_0000); }), IOC, "widening an sNaN");
        assert_eq!(flags_of(|| { convert(F32, F64, d, 0x7fc0_0000); }), 0);
        assert_eq!(flags_of(|| { int_to_fp(F32, d, false, (1 << 24) + 1, 0); }), IXC, "2^24+1 is not a single");
        assert_eq!(flags_of(|| { int_to_fp(F32, d, false, 1 << 24, 0); }), 0);
        // flags accumulate until taken
        take_flags();
        binop(F32, d, BinOp::Div, one, 0);
        binop(F32, d, BinOp::Add, one, 0x3380_0000);
        assert_eq!(take_flags(), DZC | IXC);
        assert_eq!(take_flags(), 0);
    }

    #[test]
    fn conversion_flags() {
        let to_int = |v: f64, rounded: f64, signed, is64| flags_of(|| raise_to_int(F64, v.to_bits(), v, rounded, signed, is64));
        assert_eq!(to_int(2.0, 2.0, true, false), 0);
        assert_eq!(to_int(1.5, 2.0, true, false), IXC);
        assert_eq!(to_int(2147483648.0, 2147483648.0, true, false), IOC, "out of i32 range");
        assert_eq!(to_int(-2147483648.0, -2147483648.0, true, false), 0, "i32::MIN is in range");
        assert_eq!(to_int(-0.5, -0.0, false, true), IXC, "rounds to zero: in range for unsigned, inexact");
        assert_eq!(to_int(-1.0, -1.0, false, false), IOC, "negative to unsigned");
        assert_eq!(to_int(f64::INFINITY, f64::INFINITY, true, true), IOC);
        assert_eq!(flags_of(|| raise_to_int(F64, f64::NAN.to_bits(), f64::NAN, f64::NAN, true, true)), IOC);
        // compares: FCMP only for signalling NaNs, FCMPE for any NaN
        let (snan, qnan) = (0x7ff0_0000_0000_0001u64, 0x7ff8_0000_0000_0000u64);
        assert_eq!(flags_of(|| raise_compare(F64, qnan, 0, false)), 0);
        assert_eq!(flags_of(|| raise_compare(F64, qnan, 0, true)), IOC);
        assert_eq!(flags_of(|| raise_compare(F64, 0, snan, false)), IOC);
        // FRINTX raises IXC only when it changes the value
        assert_eq!(flags_of(|| raise_round_int(F64, 1.5f64.to_bits(), 1.5, 2.0, true)), IXC);
        assert_eq!(flags_of(|| raise_round_int(F64, 1.5f64.to_bits(), 1.5, 2.0, false)), 0);
        assert_eq!(flags_of(|| raise_round_int(F64, 2.0f64.to_bits(), 2.0, 2.0, true)), 0);
    }

    #[test]
    fn issue_363_repros() {
        let fz = Env { round: Round::Nearest, fz: true, dn: false };
        // FMUL S: 1.0 * denormal, FZ=1 -> +0 (and the denormal is flushed as an input)
        assert_eq!(binop(F32, fz, BinOp::Mul, 0x3f80_0000, 1), 0);
        // FADD S with DN=1: 0 + qNaN(payload) -> positive default NaN
        let dn = Env { round: Round::Nearest, fz: false, dn: true };
        assert_eq!(binop(F32, dn, BinOp::Add, 0, 0xffc0_0001), 0x7fc0_0000);
        // FADD S with RMode=RP: 1.0 + 2^-149 rounds up
        let rp = Env { round: Round::Up, fz: false, dn: false };
        assert_eq!(binop(F32, rp, BinOp::Add, 0x3f80_0000, 1), 0x3f80_0001);
        // overflow honours the mode
        assert_eq!(binop(F32, Env { round: Round::Zero, fz: false, dn: false }, BinOp::Mul, 0x7f7f_ffff, 0x4000_0000), 0x7f7f_ffff);
        assert_eq!(binop(F32, rp, BinOp::Mul, 0xff7f_ffff, 0x4000_0000), 0xff7f_ffff);
        assert_eq!(binop(F32, rp, BinOp::Mul, 0x7f7f_ffff, 0x4000_0000), 0x7f80_0000);
        // exact cancellation is -0 only when rounding down
        let rm = Env { round: Round::Down, fz: false, dn: false };
        assert_eq!(binop(F32, rm, BinOp::Sub, 0x3f80_0000, 0x3f80_0000), 0x8000_0000);
        assert_eq!(binop(F32, rp, BinOp::Sub, 0x3f80_0000, 0x3f80_0000), 0);
    }
}
