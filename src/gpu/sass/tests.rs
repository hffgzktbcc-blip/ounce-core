use super::decode::{decode, DecodeError, FSrc, Instr, IpaFreq, MufuOp, Src, RZ};
use super::sph::{PixelImap, SphBuilder};
use super::*;

/// Test-only encoder for synthetic words, using the same field positions as the decoder.
/// The `cuasm_*` tests check the decoder against real sm_86 words independently of it.
mod asm {
    use super::super::decode::RZ;

    fn op(full: u16) -> u128 {
        full as u128 | (7 << 12)
    }
    fn rd(r: u8) -> u128 {
        (r as u128) << 16
    }
    fn ra(r: u8) -> u128 {
        (r as u128) << 24
    }
    fn cb(bank: u8, off: u16) -> u128 {
        ((off as u128) << 38) | ((bank as u128) << 54)
    }
    pub fn exit() -> u128 {
        op(0x94d)
    }
    pub fn nop() -> u128 {
        op(0x918)
    }
    pub fn mov_imm(d: u8, imm: u32) -> u128 {
        op(0x802) | rd(d) | ((imm as u128) << 32) | (0xf << 72)
    }
    pub fn mov_cb(d: u8, bank: u8, off: u16) -> u128 {
        op(0xa02) | rd(d) | cb(bank, off) | (0xf << 72)
    }
    pub fn fmul_cb(d: u8, a: u8, bank: u8, off: u16) -> u128 {
        op(0xa20) | rd(d) | ra(a) | cb(bank, off) | (4 << 84)
    }
    pub fn ffma_cb(d: u8, a: u8, bank: u8, off: u16, c: u8) -> u128 {
        op(0xa23) | rd(d) | ra(a) | cb(bank, off) | ((c as u128) << 64)
    }
    pub fn fadd_rr(d: u8, a: u8, b: u8) -> u128 {
        op(0x221) | rd(d) | ra(a) | ((b as u128) << 32)
    }
    pub fn mufu_rcp(d: u8, s: u8) -> u128 {
        op(0x308) | rd(d) | ((s as u128) << 32) | (4 << 74)
    }
    pub fn ldc(d: u8, bank: u8, off: u16) -> u128 {
        op(0xb82) | rd(d) | ra(RZ) | cb(bank, off) | (4 << 73)
    }
    pub fn ald(d: u8, addr: u16, comps: u8) -> u128 {
        op(0x321) | rd(d) | ra(RZ) | ((RZ as u128) << 32) | ((addr as u128) << 40) | (((comps - 1) as u128) << 74)
    }
    pub fn ast(s: u8, addr: u16, comps: u8) -> u128 {
        op(0x322) | ra(RZ) | ((s as u128) << 32) | ((addr as u128) << 40) | ((RZ as u128) << 64) | (((comps - 1) as u128) << 74)
    }
    /// `freq`: 0 PASS, 1 PassMulW, 2 CONSTANT.
    pub fn ipa(d: u8, addr: u16, freq: u8, mul: u8) -> u128 {
        op(0x326) | rd(d) | ra(RZ) | ((mul as u128) << 32) | (((addr >> 2) as u128) << 64) | ((freq as u128) << 78)
    }
    pub fn pred(w: u128, p: u8, neg: bool) -> u128 {
        (w & !(0xf << 12)) | ((p as u128) << 12) | ((neg as u128) << 15)
    }
}

fn program(header: &SphBuilder, code: &[u128]) -> Vec<u8> {
    let mut b = header.bytes(AMPERE_SPH_SIZE);
    for w in code {
        b.extend_from_slice(&w.to_le_bytes());
    }
    b
}

pub(super) fn validate(src: &str) {
    let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}\n{src}", e.emit_to_string(src)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{src}"));
}

/// Compares against `src/gpu/sass/golden/<name>`; `OUNCE_BLESS=1` rewrites the file.
pub(super) fn golden(name: &str, expected: &str, got: &str) {
    if std::env::var_os("OUNCE_BLESS").is_some() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gpu/sass/golden").join(name);
        std::fs::write(p, got).unwrap();
        return;
    }
    assert_eq!(got, expected, "golden {name} differs; got:\n{got}");
}

fn w(lo: u64, hi: u64) -> u128 {
    ((hi as u128) << 64) | lo as u128
}

fn reg(r: u8) -> FSrc {
    FSrc { src: Src::Reg(r), neg: false, abs: false }
}

// Real sm_86 words from CuAssembler's DefaultInsAsmRepos.sm_86.txt (control bits are
// stripped there, which our decoder ignores anyway).
#[test]
fn cuasm_mov_exit_nop() {
    assert_eq!(decode(w(0x794d, 0x0380_0000)).unwrap().instr, Instr::Exit);
    assert_eq!(decode(w(0x7918, 0)).unwrap().instr, Instr::Nop);
    assert_eq!(
        decode(w(0x0000_5800_0002_7a02, 0xf00)).unwrap().instr,
        Instr::Mov { dst: 2, src: Src::Cbuf { bank: 0, offset: 0x160 } }
    );
    assert_eq!(decode(w(0x0000_01b0_0002_7802, 0xf00)).unwrap().instr, Instr::Mov { dst: 2, src: Src::Imm(0x1b0) });
    assert_eq!(decode(w(0x0000_0004_0008_7202, 0xf00)).unwrap().instr, Instr::Mov { dst: 8, src: Src::Reg(4) });
    // @P1 MOV R6, c[0x0][0x188]
    let d = decode(w(0x0000_6200_0006_1a02, 0xf00)).unwrap();
    assert_eq!((d.pred, d.pred_neg), (1, false));
    assert_eq!(d.instr, Instr::Mov { dst: 6, src: Src::Cbuf { bank: 0, offset: 0x188 } });
}

#[test]
fn cuasm_fadd_fmul() {
    let neg = |r| FSrc { src: Src::Reg(r), neg: true, abs: false };
    let abs = |r| FSrc { src: Src::Reg(r), neg: false, abs: true };
    assert_eq!(
        decode(w(0x8000_00ff_0207_7221, 0x100)).unwrap().instr,
        Instr::FAdd { dst: 7, a: neg(2), b: neg(RZ), sat: false, ftz: false }
    );
    assert_eq!(
        decode(w(0x4000_0007_0602_7221, 0x200)).unwrap().instr,
        Instr::FAdd { dst: 2, a: abs(6), b: abs(7), sat: false, ftz: false }
    );
    assert_eq!(
        decode(w(0x0000_0002_ff02_7221, 0x12000)).unwrap().instr,
        Instr::FAdd { dst: 2, a: reg(RZ), b: reg(2), sat: true, ftz: true }
    );
    assert_eq!(
        decode(w(0xbffc_a5dc_2a10_7421, 0)).unwrap().instr,
        Instr::FAdd { dst: 16, a: reg(42), b: FSrc { src: Src::Imm(0xbffc_a5dc), neg: false, abs: false }, sat: false, ftz: false }
    );
    assert_eq!(
        decode(w(0x8000_5900_0306_7621, 0)).unwrap().instr,
        Instr::FAdd { dst: 6, a: reg(3), b: FSrc { src: Src::Cbuf { bank: 0, offset: 0x164 }, neg: true, abs: false }, sat: false, ftz: false }
    );
    // FADD.RZ: directed rounding has no WGSL equivalent.
    assert!(matches!(decode(w(0x0000_0006_0707_7221, 0xc000)), Err(DecodeError::Unsupported { .. })));
    // @!P0 FMUL R6, R12, R5
    let d = decode(w(0x0000_0005_0c06_8220, 0x40_0000)).unwrap();
    assert_eq!((d.pred, d.pred_neg), (0, true));
    assert_eq!(d.instr, Instr::FMul { dst: 6, a: reg(12), b: reg(5), sat: false, ftz: false });
    assert_eq!(
        decode(w(0x0000_5900_0400_7a20, 0x40_0000)).unwrap().instr,
        Instr::FMul { dst: 0, a: reg(4), b: FSrc { src: Src::Cbuf { bank: 0, offset: 0x164 }, neg: false, abs: false }, sat: false, ftz: false }
    );
}

#[test]
fn cuasm_ffma_forms() {
    let imm = |v| FSrc { src: Src::Imm(v), neg: false, abs: false };
    let c = |o| FSrc { src: Src::Cbuf { bank: 0, offset: o }, neg: false, abs: false };
    let ff = |dst, a, b, c| Instr::FFma { dst, a, b, c, sat: false, ftz: false };
    assert_eq!(decode(w(0x0000_0007_0607_7223, 6)).unwrap().instr, ff(7, reg(6), reg(7), reg(6)));
    // FFMA R7, -R3, R6, 1
    assert_eq!(
        decode(w(0x3f80_0000_0307_7423, 0x106)).unwrap().instr,
        ff(7, FSrc { src: Src::Reg(3), neg: true, abs: false }, reg(6), imm(0x3f80_0000))
    );
    assert_eq!(decode(w(0x5f80_0000_0006_8823, 0xff)).unwrap().instr, ff(6, reg(0), imm(0x5f80_0000), reg(RZ)));
    // FFMA R8, R4, c[0x0][0x160], -R7
    assert_eq!(
        decode(w(0x0000_5800_0408_7a23, 0x807)).unwrap().instr,
        ff(8, reg(4), c(0x160), FSrc { src: Src::Reg(7), neg: true, abs: false })
    );
    assert_eq!(decode(w(0x0000_5c00_0202_7623, 9)).unwrap().instr, ff(2, reg(2), reg(9), c(0x170)));
}

#[test]
fn cuasm_mufu_ldc() {
    let m = |lo, hi| match decode(w(lo, hi)).unwrap().instr {
        Instr::Mufu { op, .. } => op,
        i => panic!("{i:?}"),
    };
    assert_eq!(m(0x0000_0003_0006_7308, 0x1000), MufuOp::Rcp);
    assert_eq!(m(0x0000_0010_0010_7308, 0x1400), MufuOp::Rsq);
    assert_eq!(m(0x0000_000a_000a_7308, 0x0800), MufuOp::Ex2);
    assert_eq!(m(0x0000_002a_0000_7308, 0x0c00), MufuOp::Lg2);
    assert_eq!(m(0x0000_0016_0005_7308, 0x0400), MufuOp::Sin);
    assert_eq!(m(0x0000_0016_0007_7308, 0x0000), MufuOp::Cos);
    assert_eq!(m(0x0000_001a_0019_7308, 0x2000), MufuOp::Sqrt);
    assert_eq!(m(0x0000_0000_0000_7308, 0x2400), MufuOp::Tanh);
    assert_eq!(
        decode(w(0x8000_0006_0006_7308, 0x1400)).unwrap().instr,
        Instr::Mufu { dst: 6, op: MufuOp::Rsq, src: FSrc { src: Src::Reg(6), neg: true, abs: false } }
    );
    // MUFU.RCP64H and LDC.U8 are outside the subset.
    assert!(decode(w(0x0000_000b_0009_7308, 0x1800)).is_err());
    assert!(decode(w(0x0000_6000_ff04_7b82, 0)).is_err());
}

#[test]
fn synthetic_roundtrip_graphics_ops() {
    assert_eq!(decode(asm::ald(0, 0x80, 4)).unwrap().instr, Instr::Ald { dst: 0, addr: 0x80, comps: 4 });
    assert_eq!(decode(asm::ast(4, 0x70, 4)).unwrap().instr, Instr::Ast { src: 4, addr: 0x70, comps: 4 });
    assert_eq!(
        decode(asm::ipa(1, 0x84, 1, 4)).unwrap().instr,
        Instr::Ipa { dst: 1, addr: 0x84, freq: IpaFreq::PassMulW, mul: 4 }
    );
    assert_eq!(decode(asm::ldc(3, 1, 0x20)).unwrap().instr, Instr::Ldc { dst: 3, bank: 1, offset: 0x20 });
    assert_eq!(decode(asm::nop()).unwrap().to_string(), "NOP");
    assert_eq!(decode(asm::ffma_cb(4, 1, 0, 0x10, 4)).unwrap().to_string(), "FFMA R4, R1, c[0x0][0x10], R4");
}

pub(super) fn passthrough_vs() -> Vec<u8> {
    let h = SphBuilder::vertex().imap(0x80).imap(0x84).imap(0x88).imap(0x8c).omap(0x70).omap(0x74).omap(0x78).omap(0x7c);
    program(
        &h,
        &[asm::ald(0, 0x80, 4), asm::ald(4, 0x90, 4), asm::ast(0, 0x70, 4), asm::ast(4, 0x80, 4), asm::exit()],
    )
}

#[test]
fn golden_passthrough_vertex() {
    let t = translate(ShaderStage::Vertex, &passthrough_vs()).unwrap();
    golden("passthrough.vert.wgsl", include_str!("golden/passthrough.vert.wgsl"), &t.wgsl);
    validate(&t.wgsl);
    assert_eq!(t.interface.vertex_inputs, vec![0, 1]);
    assert_eq!(t.interface.varyings, vec![Varying { location: 0, interpolation: Interpolation::Perspective }]);
    assert!(t.interface.writes_position);
}

#[test]
fn golden_solid_colour_fragment() {
    let h = SphBuilder::pixel().ps_target(0, 0xf);
    let one = 1.0f32.to_bits();
    let code = [asm::mov_imm(0, one), asm::mov_imm(1, 0x3f00_0000), asm::mov_imm(2, 0), asm::mov_imm(3, one), asm::exit()];
    let t = translate(ShaderStage::Fragment, &program(&h, &code)).unwrap();
    golden("solid.frag.wgsl", include_str!("golden/solid.frag.wgsl"), &t.wgsl);
    validate(&t.wgsl);
    assert_eq!(t.interface.color_targets, vec![0]);
}

#[test]
fn golden_varying_colour_fragment() {
    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..4 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    // The usual perspective-correct pattern: 1/w, its reciprocal, then IPA with mul.
    let mut code = vec![asm::ipa(4, 0x7c, 0, RZ), asm::mufu_rcp(4, 4)];
    for c in 0..4u8 {
        code.push(asm::ipa(c, 0x80 + 4 * c as u16, 1, 4));
    }
    code.push(asm::exit());
    let t = translate(ShaderStage::Fragment, &program(&h, &code)).unwrap();
    golden("varying.frag.wgsl", include_str!("golden/varying.frag.wgsl"), &t.wgsl);
    validate(&t.wgsl);
    // The vertex stage's outputs match the fragment stage's inputs.
    let vs = translate(ShaderStage::Vertex, &passthrough_vs()).unwrap();
    assert_eq!(vs.interface.varyings, t.interface.varyings);
}

#[test]
fn constant_buffer_transform_validates() {
    // x' = x * c0[0].x + c0[1].x, y' = y * c0[0].y + R(c0[1].y via LDC), w from c1.
    let h = SphBuilder::vertex();
    let code = [
        asm::ald(0, 0x80, 4),
        asm::fmul_cb(4, 0, 0, 0x0),
        asm::ffma_cb(4, 4, 0, 0x10, 4),
        asm::ldc(6, 0, 0x14),
        asm::ffma_cb(5, 1, 0, 0x4, 6),
        asm::fadd_rr(5, 5, 6),
        asm::mov_cb(7, 1, 0x0),
        asm::mov_imm(8, 0),
        asm::ast(4, 0x70, 2),
        asm::ast(8, 0x78, 1),
        asm::ast(7, 0x7c, 1),
        asm::exit(),
    ];
    let t = translate(ShaderStage::Vertex, &program(&h, &code)).unwrap();
    validate(&t.wgsl);
    assert_eq!(
        t.interface.constant_buffers,
        vec![ConstantBuffer { group: 0, binding: 0, size_bytes: 32 }, ConstantBuffer { group: 0, binding: 1, size_bytes: 16 }]
    );
    assert!(t.wgsl.contains("cb0[1u][1u]"), "{}", t.wgsl);
}

#[test]
fn fragment_constant_buffer_colour_validates() {
    let h = SphBuilder::pixel().ps_target(0, 0xf).ps_target(1, 0x1);
    let code = [
        asm::mov_cb(0, 0, 0x0),
        asm::mov_cb(1, 0, 0x4),
        asm::mov_cb(2, 0, 0x8),
        asm::mov_cb(3, 0, 0xc),
        asm::mufu_rcp(4, 0),
        asm::exit(),
    ];
    let t = translate(ShaderStage::Fragment, &program(&h, &code)).unwrap();
    validate(&t.wgsl);
    assert_eq!(t.interface.color_targets, vec![0, 1]);
    assert_eq!(t.interface.constant_buffers, vec![ConstantBuffer { group: 1, binding: 0, size_bytes: 16 }]);
}

#[test]
fn rejects_what_step1_cannot_do() {
    let vs = SphBuilder::vertex();
    let ps = SphBuilder::pixel().ps_target(0, 0xf);
    // No EXIT.
    assert_eq!(translate(ShaderStage::Vertex, &program(&vs, &[asm::nop()])).unwrap_err(), XlateError::MissingExit);
    // Unknown opcode (IMAD 0x224) reports where.
    match translate(ShaderStage::Vertex, &program(&vs, &[asm::nop(), 0x7224, asm::exit()])).unwrap_err() {
        XlateError::Decode { at, err: DecodeError::UnknownOpcode { opcode: 0x224 }, .. } => assert_eq!(at, 0x10),
        e => panic!("{e:?}"),
    }
    // Predicated ALU ops are if-converted now (see `ctl_tests`). A predicated NOP is fine.
    let p = asm::pred(asm::mov_imm(0, 0), 0, false);
    assert!(translate(ShaderStage::Fragment, &program(&ps, &[p, asm::exit()])).is_ok());
    let pn = asm::pred(asm::nop(), 2, true);
    assert!(translate(ShaderStage::Fragment, &program(&ps, &[pn, asm::exit()])).is_ok());
    // Stage/header mismatch and stage-specific ops.
    assert_eq!(
        translate(ShaderStage::Fragment, &program(&vs, &[asm::exit()])).unwrap_err(),
        XlateError::StageMismatch(ShaderStage::Fragment)
    );
    assert!(matches!(translate(ShaderStage::Vertex, &program(&vs, &[asm::ipa(0, 0x80, 0, RZ), asm::exit()])), Err(XlateError::Unsupported { .. })));
    assert!(matches!(translate(ShaderStage::Fragment, &program(&ps, &[asm::ald(0, 0x80, 1), asm::exit()])), Err(XlateError::Unsupported { .. })));
    // Truncated header.
    assert!(matches!(translate(ShaderStage::Vertex, &[0u8; 8]), Err(XlateError::Truncated { .. })));
}

#[test]
fn maps_ampere_pipeline_types() {
    use crate::gpu::ampere::engine::ShaderType as T;
    assert_eq!(ShaderStage::from_ampere(T::Vertex), Some(ShaderStage::Vertex));
    assert_eq!(ShaderStage::from_ampere(T::VertexCullBeforeFetch), Some(ShaderStage::Vertex));
    assert_eq!(ShaderStage::from_ampere(T::Pixel), Some(ShaderStage::Fragment));
    assert_eq!(ShaderStage::from_ampere(T::Geometry), None);
}
