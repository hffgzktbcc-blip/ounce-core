//! WGSL emitter for the SSA IR.
//!
//! Interface conventions (shared with the Ampere 3D decoder and the wgpu side):
//! - vertex input generic `i` -> `@location(i) a{i}: vec4<f32>`;
//! - varying generic `i` -> `@location(i)`, interpolation from the fragment SPH IMAP;
//! - fragment colour target `t` -> `@location(t) c{t}: vec4<f32>`;
//! - constant buffer bank `b` -> `@group(0)` (vertex) or `@group(1)` (fragment),
//!   `@binding(b)`, `var<uniform> cb{b}: array<vec4<u32>, N>`;
//! - texture handle `k` (in [`ShaderInterface::textures`] order) -> same group,
//!   `@binding(32 + 2k) t{k}: texture_2d<f32>` and, when the shader samples it,
//!   `@binding(33 + 2k) s{k}: sampler`.
//!
//! Semantics notes (approximations, not hardware behaviour):
//! - `.FTZ` is dropped; WGSL leaves denormal handling to the implementation.
//! - `FFMA` -> `fma()`, which WGSL does not promise to fuse on every backend.
//! - `.SAT` -> `saturate()`; hardware maps NaN to 0, WGSL leaves NaN unspecified.
//! - `MUFU.SIN`/`COS`: UNVERIFIED (recalled from Mesa NAK, which pre-scales by 1/2pi
//!   before the op on SM70+), so we emit `sin(x * 2pi)`.
//! - Fragment `@builtin(position).w` is 1/w_clip (WebGPU/Vulkan semantics), which is what
//!   `IPA.PASS a[0x7c]` yields. A perspective varying read with `IPA.PASS` is
//!   screen-linear `attr/w`, so we emit `in.l.c * position.w`; a following
//!   `IPA ... , R(rcp w)` multiplies that back to the perspective-correct value.
//! - Fragment colours are taken from R0.. at `EXIT`, packed over the enabled components
//!   of the enabled targets in order (our reading of Mesa nvc0's output slot assignment).
//!   Disabled components are written as 0.0. Depth/sample-mask output is unsupported.
//! - Textures are declared `texture_2d<f32>`: integer formats are not modelled. An implicit
//!   LOD in a vertex shader becomes level 0 (WGSL has no derivatives there).

use std::collections::BTreeMap;
use std::fmt::Write;

use super::decode::{IpaFreq, MufuOp, TexHandle, TexOp};
use super::ir::{attr_slot, AttrSlot, Function, Lod, Op, Ty, Value};
use super::sph::{PixelImap, Sph};
use super::{ConstantBuffer, Interpolation, ShaderInterface, ShaderStage, TextureBinding, Varying, XlateError};

const COMP: [char; 4] = ['x', 'y', 'z', 'w'];

fn f32_lit(bits: u32) -> String {
    let f = f32::from_bits(bits);
    if bits == 0 {
        "0.0f".into()
    } else if f.is_normal() {
        let s = format!("{:?}", f.abs());
        if f < 0.0 { format!("(-{s}f)") } else { format!("{s}f") }
    } else {
        format!("bitcast<f32>({bits:#010x}u)")
    }
}

struct Ctx<'a> {
    f: &'a Function,
    sph: &'a Sph,
    /// Varying location -> interpolation (fragment) for the input struct.
    interp: BTreeMap<u32, Interpolation>,
    /// Texture handle -> binding slot `k`.
    textures: BTreeMap<TexHandle, u32>,
}

impl Ctx<'_> {
    fn as_f32(&self, v: Value) -> String {
        match self.f.op(v) {
            Op::Const(bits) => f32_lit(bits),
            op if op.ty() == Ty::U32 => format!("bitcast<f32>(v{})", v.0),
            _ => format!("v{}", v.0),
        }
    }

    fn as_i32(&self, v: Value) -> String {
        match self.f.op(v) {
            Op::Const(bits) => format!("{}i", bits as i32),
            _ => format!("bitcast<i32>(v{})", v.0),
        }
    }

    fn expr(&self, op: Op) -> String {
        match op {
            Op::Const(bits) => format!("{bits:#010x}u"),
            Op::Cbuf { bank, offset } => format!("cb{bank}[{}u][{}u]", offset / 16, (offset / 4) % 4),
            Op::Attr { addr } => match attr_slot(addr) {
                Some(AttrSlot::Generic { loc, comp }) => format!("vin.a{loc}.{}", COMP[comp as usize]),
                _ => unreachable!("lowering only emits generic vertex inputs"),
            },
            Op::Interp { addr, freq, mul } => {
                let raw = match attr_slot(addr) {
                    Some(AttrSlot::Position(c)) => format!("fin.frag_pos.{}", COMP[c as usize]),
                    Some(AttrSlot::Generic { loc, comp }) => {
                        let base = format!("fin.l{loc}.{}", COMP[comp as usize]);
                        match self.interp.get(&loc) {
                            Some(Interpolation::Perspective) if freq != IpaFreq::Constant => {
                                format!("({base} * fin.frag_pos.w)")
                            }
                            _ => base,
                        }
                    }
                    None => unreachable!("lowering checks IPA addresses"),
                };
                match mul {
                    Some(m) => format!("{raw} * {}", self.as_f32(m)),
                    None => raw,
                }
            }
            Op::FNeg(a) => format!("-{}", self.as_f32(a)),
            Op::FAbs(a) => format!("abs({})", self.as_f32(a)),
            Op::FSat(a) => format!("saturate({})", self.as_f32(a)),
            Op::FAdd(a, b) => format!("{} + {}", self.as_f32(a), self.as_f32(b)),
            Op::FMul(a, b) => format!("{} * {}", self.as_f32(a), self.as_f32(b)),
            Op::FFma(a, b, c) => format!("fma({}, {}, {})", self.as_f32(a), self.as_f32(b), self.as_f32(c)),
            Op::Mufu(m, a) => {
                let x = self.as_f32(a);
                match m {
                    MufuOp::Rcp => format!("1.0f / {x}"),
                    MufuOp::Rsq => format!("inverseSqrt({x})"),
                    MufuOp::Sqrt => format!("sqrt({x})"),
                    MufuOp::Ex2 => format!("exp2({x})"),
                    MufuOp::Lg2 => format!("log2({x})"),
                    MufuOp::Sin => format!("sin({x} * 6.2831855f)"),
                    MufuOp::Cos => format!("cos({x} * 6.2831855f)"),
                    MufuOp::Tanh => format!("tanh({x})"),
                }
            }
            Op::Tex { op, handle, lod, x, y } => {
                let k = self.textures[&handle];
                match op {
                    TexOp::Sample => {
                        let c = format!("vec2<f32>({}, {})", self.as_f32(x), self.as_f32(y));
                        match lod {
                            Lod::Auto if self.f.stage == ShaderStage::Fragment => format!("textureSample(t{k}, s{k}, {c})"),
                            Lod::Auto | Lod::Zero => format!("textureSampleLevel(t{k}, s{k}, {c}, 0.0f)"),
                            Lod::Level(l) => format!("textureSampleLevel(t{k}, s{k}, {c}, {})", self.as_f32(l)),
                        }
                    }
                    TexOp::Fetch => {
                        let level = match lod {
                            Lod::Level(l) => self.as_i32(l),
                            _ => "0i".into(),
                        };
                        format!("textureLoad(t{k}, vec2<i32>({}, {}), {level})", self.as_i32(x), self.as_i32(y))
                    }
                }
            }
            Op::Comp(t, c) => format!("v{}.{}", t.0, COMP[c as usize]),
        }
    }
}

fn pixel_interp(sph: &Sph, loc: u32) -> Result<Interpolation, XlateError> {
    let mut mode = None;
    for c in 0..4 {
        let m = match sph.ps_imap_generic(loc, c) {
            PixelImap::Unused => continue,
            PixelImap::Constant => Interpolation::Flat,
            PixelImap::Perspective => Interpolation::Perspective,
            PixelImap::ScreenLinear => Interpolation::Linear,
        };
        if mode.is_some_and(|p| p != m) {
            return Err(XlateError::Unsupported { at: 0, what: "mixed interpolation within one varying" });
        }
        mode = Some(m);
    }
    // A read the header marks unused: default to perspective (the GL default).
    Ok(mode.unwrap_or(Interpolation::Perspective))
}

fn interp_attr(i: Interpolation) -> &'static str {
    match i {
        Interpolation::Perspective => "@interpolate(perspective)",
        Interpolation::Linear => "@interpolate(linear)",
        Interpolation::Flat => "@interpolate(flat)",
    }
}

pub fn emit(f: &Function, sph: &Sph) -> Result<(String, ShaderInterface), XlateError> {
    let group = match f.stage {
        ShaderStage::Vertex => 0,
        ShaderStage::Fragment => 1,
    };
    let mut iface = ShaderInterface::default();

    // Constant buffers: size = highest vec4 touched + 1.
    let mut cbufs: BTreeMap<u8, u32> = BTreeMap::new();
    let mut inputs: BTreeMap<u32, ()> = BTreeMap::new();
    // Texture handle -> sampled (needs a sampler binding).
    let mut textures: BTreeMap<TexHandle, bool> = BTreeMap::new();
    for op in &f.values {
        match *op {
            Op::Tex { op, handle, .. } => *textures.entry(handle).or_insert(false) |= op == TexOp::Sample,
            Op::Cbuf { bank, offset } => {
                let e = cbufs.entry(bank).or_insert(0);
                *e = (*e).max(offset as u32 / 16 + 1);
            }
            Op::Attr { addr } | Op::Interp { addr, .. } => {
                if let Some(AttrSlot::Generic { loc, .. }) = attr_slot(addr) {
                    inputs.insert(loc, ());
                }
            }
            _ => {}
        }
    }

    let mut interp = BTreeMap::new();
    if f.stage == ShaderStage::Fragment {
        for &loc in inputs.keys() {
            interp.insert(loc, pixel_interp(sph, loc)?);
        }
    }
    let slots = textures.keys().enumerate().map(|(k, &h)| (h, k as u32)).collect();
    let ctx = Ctx { f, sph, interp, textures: slots };

    let mut out = String::new();
    let stage_name = match f.stage {
        ShaderStage::Vertex => "vertex",
        ShaderStage::Fragment => "fragment",
    };
    writeln!(out, "// ounce-core SASS -> WGSL, step 1 ({stage_name}).").unwrap();
    writeln!(out, "// Community-sourced decoding, not a hardware-accurate translation.").unwrap();
    for line in &f.listing {
        writeln!(out, "//   {line}").unwrap();
    }
    out.push('\n');

    for (&bank, &len) in &cbufs {
        writeln!(out, "@group({group}) @binding({bank}) var<uniform> cb{bank}: array<vec4<u32>, {len}>;").unwrap();
        iface.constant_buffers.push(ConstantBuffer { group, binding: bank as u32, size_bytes: len * 16 });
    }
    for (k, (&handle, &sampled)) in textures.iter().enumerate() {
        let binding = 32 + 2 * k as u32;
        writeln!(out, "@group({group}) @binding({binding}) var t{k}: texture_2d<f32>;").unwrap();
        if sampled {
            writeln!(out, "@group({group}) @binding({}) var s{k}: sampler;", binding + 1).unwrap();
        }
        iface.textures.push(TextureBinding { group, binding, sampler_binding: sampled.then_some(binding + 1), handle });
    }
    if !cbufs.is_empty() || !textures.is_empty() {
        out.push('\n');
    }

    let body = |out: &mut String| {
        for (i, op) in f.values.iter().enumerate() {
            match op {
                Op::Const(_) => {}
                op => {
                    let ty = match op.ty() {
                        Ty::U32 => "u32",
                        Ty::F32 => "f32",
                        Ty::Vec4 => "vec4<f32>",
                    };
                    writeln!(out, "    let v{i}: {ty} = {};", ctx.expr(*op)).unwrap();
                }
            }
        }
    };

    match f.stage {
        ShaderStage::Vertex => {
            // Outputs: position plus every generic vector stored to.
            let mut outs: BTreeMap<u32, ()> = BTreeMap::new();
            for &(addr, _) in &f.stores {
                match attr_slot(addr) {
                    Some(AttrSlot::Generic { loc, .. }) => {
                        outs.insert(loc, ());
                    }
                    Some(AttrSlot::Position(_)) => iface.writes_position = true,
                    None => {}
                }
            }
            if !inputs.is_empty() {
                writeln!(out, "struct VsIn {{").unwrap();
                for &loc in inputs.keys() {
                    writeln!(out, "    @location({loc}) a{loc}: vec4<f32>,").unwrap();
                    iface.vertex_inputs.push(loc);
                }
                writeln!(out, "}};\n").unwrap();
            }
            writeln!(out, "struct VsOut {{").unwrap();
            writeln!(out, "    @builtin(position) position: vec4<f32>,").unwrap();
            for &loc in outs.keys() {
                writeln!(out, "    @location({loc}) o{loc}: vec4<f32>,").unwrap();
                iface.varyings.push(Varying { location: loc, interpolation: Interpolation::Perspective });
            }
            writeln!(out, "}};\n").unwrap();
            let param = if inputs.is_empty() { "" } else { "vin: VsIn" };
            writeln!(out, "@vertex\nfn main({param}) -> VsOut {{").unwrap();
            writeln!(out, "    var out: VsOut;").unwrap();
            body(&mut out);
            for &(addr, v) in &f.stores {
                let dst = match attr_slot(addr) {
                    Some(AttrSlot::Position(c)) => format!("out.position.{}", COMP[c as usize]),
                    Some(AttrSlot::Generic { loc, comp }) => format!("out.o{loc}.{}", COMP[comp as usize]),
                    None => unreachable!("lowering checks AST addresses"),
                };
                writeln!(out, "    {dst} = {};", ctx.as_f32(v)).unwrap();
            }
            writeln!(out, "    return out;\n}}").unwrap();
        }
        ShaderStage::Fragment => {
            if sph.ps_omap_depth() || sph.ps_omap_sample_mask() {
                return Err(XlateError::Unsupported { at: 0, what: "depth or sample-mask output" });
            }
            writeln!(out, "struct FsIn {{").unwrap();
            writeln!(out, "    @builtin(position) frag_pos: vec4<f32>,").unwrap();
            for (&loc, &mode) in &ctx.interp {
                writeln!(out, "    @location({loc}) {} l{loc}: vec4<f32>,", interp_attr(mode)).unwrap();
                iface.varyings.push(Varying { location: loc, interpolation: mode });
            }
            writeln!(out, "}};\n").unwrap();
            let targets: Vec<(u32, u8)> = (0..8).map(|t| (t, sph.ps_omap_target(t))).filter(|&(_, m)| m != 0).collect();
            if !targets.is_empty() {
                writeln!(out, "struct FsOut {{").unwrap();
                for &(t, _) in &targets {
                    writeln!(out, "    @location({t}) c{t}: vec4<f32>,").unwrap();
                    iface.color_targets.push(t);
                }
                writeln!(out, "}};\n").unwrap();
                writeln!(out, "@fragment\nfn main(fin: FsIn) -> FsOut {{").unwrap();
                writeln!(out, "    var out: FsOut;").unwrap();
            } else {
                writeln!(out, "@fragment\nfn main(fin: FsIn) {{").unwrap();
            }
            body(&mut out);
            let mut reg = 0usize;
            for &(t, mask) in &targets {
                let mut comps = Vec::new();
                for c in 0..4 {
                    if mask & (1 << c) != 0 {
                        comps.push(match f.exit_regs.get(reg).copied().flatten() {
                            Some(v) => ctx.as_f32(v),
                            None => "0.0f".into(),
                        });
                        reg += 1;
                    } else {
                        comps.push("0.0f".into());
                    }
                }
                writeln!(out, "    out.c{t} = vec4<f32>({});", comps.join(", ")).unwrap();
            }
            if !targets.is_empty() {
                writeln!(out, "    return out;").unwrap();
            }
            writeln!(out, "}}").unwrap();
        }
    }
    let _ = ctx.sph;
    Ok((out, iface))
}
