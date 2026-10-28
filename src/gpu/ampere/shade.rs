//! Runs `gpu::sass` IR on the CPU: one call per vertex or per fragment.
//!
//! The IR is straight-line SSA (no control flow in step 1), so a run is one pass over the values
//! in program order. Semantics match what `gpu::sass::wgsl` emits for the same IR, including the
//! `MUFU.SIN/COS` 2*pi pre-scale, so the software and wgpu paths agree.

use crate::gpu::sass::decode::{IpaFreq, MufuOp};
use crate::gpu::sass::ir::{Function, Op};

/// What a shader reads from outside its registers.
pub(crate) trait ShaderInputs {
    /// Constant buffer word (`c[bank][offset]`), 0 when unbound or out of range.
    fn cbuf(&self, bank: u8, offset: u16) -> u32;
    /// Vertex input word at attribute address `addr`.
    fn attr(&self, addr: u16) -> u32;
    /// Fragment input at attribute address `addr`, before any `PassMulW` multiply.
    fn interp(&self, addr: u16, freq: IpaFreq) -> f32;
}

/// Evaluates every value of `f`; the result is indexed by `Value.0` and holds raw bits.
pub(crate) fn run(f: &Function, inputs: &dyn ShaderInputs) -> Vec<u32> {
    let mut v: Vec<u32> = Vec::with_capacity(f.values.len());
    for op in &f.values {
        let x = |val: crate::gpu::sass::ir::Value| f32::from_bits(v[val.0 as usize]);
        let bits = match *op {
            Op::Const(b) => b,
            Op::Cbuf { bank, offset } => inputs.cbuf(bank, offset),
            Op::Attr { addr } => inputs.attr(addr),
            Op::Interp { addr, freq, mul } => {
                let raw = inputs.interp(addr, freq);
                match mul {
                    Some(m) => (raw * x(m)).to_bits(),
                    None => raw.to_bits(),
                }
            }
            Op::FNeg(a) => (-x(a)).to_bits(),
            Op::FAbs(a) => x(a).abs().to_bits(),
            Op::FSat(a) => {
                let t = x(a);
                // saturate(NaN) is 0 in WGSL's clamp-based definition.
                (if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }).to_bits()
            }
            Op::FAdd(a, b) => (x(a) + x(b)).to_bits(),
            Op::FMul(a, b) => (x(a) * x(b)).to_bits(),
            Op::FFma(a, b, c) => x(a).mul_add(x(b), x(c)).to_bits(),
            Op::Mufu(op, a) => {
                let t = x(a);
                match op {
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
        };
        v.push(bits);
    }
    v
}

/// Attribute stores of a vertex shader run, last store per address wins.
pub(crate) fn vertex_outputs(f: &Function, values: &[u32]) -> Vec<(u16, u32)> {
    let mut out: Vec<(u16, u32)> = Vec::new();
    for &(addr, val) in &f.stores {
        let bits = values[val.0 as usize];
        match out.iter_mut().find(|(a, _)| *a == addr) {
            Some(slot) => slot.1 = bits,
            None => out.push((addr, bits)),
        }
    }
    out
}

/// Register `reg` at `EXIT` as a float (0 if never written).
pub(crate) fn exit_reg(f: &Function, values: &[u32], reg: usize) -> f32 {
    f.exit_regs.get(reg).copied().flatten().map_or(0.0, |v| f32::from_bits(values[v.0 as usize]))
}
