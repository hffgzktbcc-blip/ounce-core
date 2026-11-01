//! Runs `gpu::sass` IR on the CPU: one call per vertex or per fragment.
//!
//! The IR is straight-line SSA (branches are if-converted during lowering), so a run is one pass
//! over the values in program order. Semantics match what `gpu::sass::wgsl` emits for the same IR, including the
//! `MUFU.SIN/COS` 2*pi pre-scale, so the software and wgpu paths agree.

use crate::gpu::sass::decode::{IpaFreq, TexHandle, TexOp};
use crate::gpu::sass::ir::{Function, Op};

/// What a shader reads from outside its registers.
pub(crate) trait ShaderInputs {
    /// Constant buffer word (`c[bank][offset]`), 0 when unbound or out of range.
    fn cbuf(&self, bank: u8, offset: u16) -> u32;
    /// Vertex input word at attribute address `addr`.
    fn attr(&self, addr: u16) -> u32;
    /// Fragment input at attribute address `addr`, before any `PassMulW` multiply.
    fn interp(&self, addr: u16, freq: IpaFreq) -> f32;
    /// 2D texture read: `x`, `y` are float bits for [`TexOp::Sample`] and integer texel
    /// coordinates for [`TexOp::Fetch`]. Level 0 only; zeros when the texture is not resolved.
    fn texture(&self, op: TexOp, handle: TexHandle, x: u32, y: u32) -> [f32; 4] {
        let _ = (op, handle, x, y);
        [0.0; 4]
    }
}

/// Evaluates every value of `f`; the result is indexed by `Value.0` and holds raw bits.
pub(crate) fn run(f: &Function, inputs: &dyn ShaderInputs) -> Vec<u32> {
    let mut v: Vec<u32> = Vec::with_capacity(f.values.len());
    // Texture results, by the value index of their `Op::Tex`.
    let mut texels: Vec<(u32, [f32; 4])> = Vec::new();
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
            Op::Tex { op, handle, x: tx, y: ty, .. } => {
                // The level is ignored: textures are read at level 0.
                texels.push((v.len() as u32, inputs.texture(op, handle, v[tx.0 as usize], v[ty.0 as usize])));
                0
            }
            Op::Comp(t, c) => texels.iter().find(|(i, _)| *i == t.0).map_or(0, |(_, rgba)| rgba[c as usize].to_bits()),
            // Pure ops: one definition shared with the translator's tests.
            ref op => crate::gpu::sass::eval::op(op, &v).expect("every other IR op is pure"),
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
