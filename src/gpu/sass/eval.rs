//! Reference semantics of the pure IR ops, on raw 32-bit register bits.
//!
//! The CPU draw path (`src/gpu/ampere/shade.rs`) evaluates shaders with this, and the WGSL
//! emitter is written to match it, so the software and wgpu paths agree. Predicates are
//! 0 or 1. Ops that read shader inputs (`Cbuf`, `Attr`, `Interp`, `Tex`, `Comp`) are not
//! pure and give `None`.

use super::decode::{FCmp, ICmp, MufuOp, Round};
use super::ir::Op;

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

pub fn fcmp(cmp: FCmp, a: f32, b: f32) -> bool {
    let unordered = a.is_nan() || b.is_nan();
    match cmp {
        FCmp::F => false,
        FCmp::Lt => a < b,
        FCmp::Eq => a == b,
        FCmp::Le => a <= b,
        FCmp::Gt => a > b,
        FCmp::Ne => !unordered && a != b,
        FCmp::Ge => a >= b,
        FCmp::Num => !unordered,
        FCmp::Nan => unordered,
        FCmp::Ltu => unordered || a < b,
        FCmp::Equ => unordered || a == b,
        FCmp::Leu => unordered || a <= b,
        FCmp::Gtu => unordered || a > b,
        FCmp::Neu => unordered || a != b,
        FCmp::Geu => unordered || a >= b,
        FCmp::T => true,
    }
}

pub fn icmp(cmp: ICmp, signed: bool, a: u32, b: u32) -> bool {
    let (x, y) = if signed { (a as i32 as i64, b as i32 as i64) } else { (a as i64, b as i64) };
    match cmp {
        ICmp::F => false,
        ICmp::Lt => x < y,
        ICmp::Eq => x == y,
        ICmp::Le => x <= y,
        ICmp::Gt => x > y,
        ICmp::Ne => x != y,
        ICmp::Ge => x >= y,
        ICmp::T => true,
    }
}

/// Bitwise three-input truth table: bit `4a + 2b + c` of `lut` gives the result.
pub fn lop3(lut: u8, a: u32, b: u32, c: u32) -> u32 {
    let mut r = 0;
    for i in 0..8 {
        if lut & (1 << i) != 0 {
            let pick = |set: bool, v: u32| if set { v } else { !v };
            r |= pick(i & 4 != 0, a) & pick(i & 2 != 0, b) & pick(i & 1 != 0, c);
        }
    }
    r
}

pub fn round(r: Round, x: f32) -> f32 {
    match r {
        Round::Nearest => x.round_ties_even(),
        Round::Floor => x.floor(),
        Round::Ceil => x.ceil(),
        Round::Trunc => x.trunc(),
    }
}

/// Evaluates `op` given the bits of every earlier value.
pub fn op(op: &Op, v: &[u32]) -> Option<u32> {
    let x = |val: super::ir::Value| v[val.0 as usize];
    let fx = |val: super::ir::Value| f(v[val.0 as usize]);
    let b = |t: bool| t as u32;
    Some(match *op {
        Op::Cbuf { .. } | Op::CbufIndexed { .. } | Op::Attr { .. } | Op::Interp { .. } | Op::Tex { .. } | Op::Comp(..) => return None,
        Op::Const(k) => k,
        Op::FNeg(a) => (-fx(a)).to_bits(),
        Op::FAbs(a) => fx(a).abs().to_bits(),
        Op::FSat(a) => {
            let t = fx(a);
            // saturate(NaN) is 0 in WGSL's clamp-based definition.
            (if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }).to_bits()
        }
        Op::FAdd(a, c) => (fx(a) + fx(c)).to_bits(),
        Op::FMul(a, c) => (fx(a) * fx(c)).to_bits(),
        Op::FFma(a, c, d) => fx(a).mul_add(fx(c), fx(d)).to_bits(),
        Op::Mufu(m, a) => {
            let t = fx(a);
            match m {
                MufuOp::Rcp => 1.0 / t,
                MufuOp::Rsq => 1.0 / t.sqrt(),
                MufuOp::Sqrt => t.sqrt(),
                MufuOp::Ex2 => t.exp2(),
                MufuOp::Lg2 => t.log2(),
                MufuOp::Sin => (t * std::f32::consts::TAU).sin(),
                MufuOp::Cos => (t * std::f32::consts::TAU).cos(),
                MufuOp::Tanh => t.tanh(),
            }
            .to_bits()
        }
        Op::Bool(t) => b(t),
        Op::FCmp(c, a, d) => b(fcmp(c, fx(a), fx(d))),
        Op::ICmp(c, s, a, d) => b(icmp(c, s, x(a), x(d))),
        Op::BAnd(a, c) => b(x(a) & x(c) != 0),
        Op::BOr(a, c) => b(x(a) | x(c) != 0),
        Op::BXor(a, c) => b((x(a) != 0) != (x(c) != 0)),
        Op::BNot(a) => b(x(a) == 0),
        Op::BSel(c, a, d) | Op::Sel(c, a, d) => {
            if x(c) != 0 { x(a) } else { x(d) }
        }
        Op::IAdd(a, c) => x(a).wrapping_add(x(c)),
        Op::INeg(a) => x(a).wrapping_neg(),
        Op::IMul(a, c) => x(a).wrapping_mul(x(c)),
        Op::Shl(a, c) => x(a) << (x(c) & 31),
        Op::ShrU(a, c) => x(a) >> (x(c) & 31),
        Op::ShrS(a, c) => ((x(a) as i32) >> (x(c) & 31)) as u32,
        Op::And(a, c) => x(a) & x(c),
        Op::Or(a, c) => x(a) | x(c),
        Op::Xor(a, c) => x(a) ^ x(c),
        Op::Not(a) => !x(a),
        Op::Lop3(l, a, c, d) => lop3(l, x(a), x(c), x(d)),
        Op::FMin(a, c) => fx(a).min(fx(c)).to_bits(),
        Op::FMax(a, c) => fx(a).max(fx(c)).to_bits(),
        Op::IMin(true, a, c) => (x(a) as i32).min(x(c) as i32) as u32,
        Op::IMin(false, a, c) => x(a).min(x(c)),
        Op::IMax(true, a, c) => (x(a) as i32).max(x(c) as i32) as u32,
        Op::IMax(false, a, c) => x(a).max(x(c)),
        Op::I2F(true, a) => (x(a) as i32 as f32).to_bits(),
        Op::I2F(false, a) => (x(a) as f32).to_bits(),
        // Rust `as` truncates, saturates and maps NaN to 0, like PTX `cvt.rzi.sat`.
        Op::F2I(true, a) => fx(a) as i32 as u32,
        Op::F2I(false, a) => fx(a) as u32,
        Op::FRound(r, a) => round(r, fx(a)).to_bits(),
    })
}
