use super::decode::{DecodeError, FSrc, Instr, IpaFreq, MufuOp, Src, RZ};
use super::maxwell::{decode, instr_offset};
use super::sph::{PixelImap, SphBuilder};
use super::tests::{golden, passthrough_vs, validate};
use super::*;

/// Test-only Maxwell encoder, same field positions as the decoder. The `maxwell_cuasm_*`
/// tests check the decoder against real sm_61 words independently of it.
mod mx {
    use super::super::decode::RZ;

    const PT: u64 = 7 << 16;

    fn op(top: u64) -> u64 {
        (top << 48) | PT
    }
    pub fn exit() -> u64 {
        op(0xe300) | 0xf
    }
    pub fn nop() -> u64 {
        op(0x50b0) | 0xf00
    }
    pub fn mov32i(d: u8, imm: u32) -> u64 {
        op(0x0100) | ((imm as u64) << 20) | (0xf << 12) | d as u64
    }
    fn amem(op_: u64, r: u8, addr: u16, comps: u8) -> u64 {
        op(op_) | (((comps - 1) as u64) << 47) | ((RZ as u64) << 39) | ((addr as u64) << 20) | ((RZ as u64) << 8) | r as u64
    }
    pub fn ald(d: u8, addr: u16, comps: u8) -> u64 {
        amem(0xefd8, d, addr, comps)
    }
    pub fn ast(s: u8, addr: u16, comps: u8) -> u64 {
        amem(0xeff0, s, addr, comps)
    }
    /// `freq`: 0 PASS, 1 multiply, 2 CONSTANT.
    pub fn ipa(d: u8, addr: u16, freq: u8, mul: u8) -> u64 {
        op(0xe000)
            | ((freq as u64) << 54)
            | (7 << 47)
            | ((RZ as u64) << 39)
            | ((addr as u64) << 28)
            | ((mul as u64) << 20)
            | ((RZ as u64) << 8)
            | d as u64
    }
    pub fn mufu_rcp(d: u8, s: u8) -> u64 {
        op(0x5080) | (4 << 20) | ((s as u64) << 8) | d as u64
    }
}

/// A plausible scheduling control word; it must never be decoded as an instruction.
const CTRL: u64 = 0x001f_8000_fc00_07e0;

fn program_mx(header: &SphBuilder, code: &[u64]) -> Vec<u8> {
    let mut b = header.bytes(MAXWELL_SPH_SIZE);
    for chunk in code.chunks(3) {
        b.extend_from_slice(&CTRL.to_le_bytes());
        for i in 0..3 {
            b.extend_from_slice(&chunk.get(i).copied().unwrap_or(mx::nop()).to_le_bytes());
        }
    }
    b
}

fn reg(r: u8) -> FSrc {
    FSrc { src: Src::Reg(r), neg: false, abs: false }
}

fn neg(s: Src) -> FSrc {
    FSrc { src: s, neg: true, abs: false }
}

fn plain(s: Src) -> FSrc {
    FSrc { src: s, neg: false, abs: false }
}

fn d(w: u64) -> Instr {
    decode(w).unwrap().instr
}

// Real sm_61 words from CuAssembler's DefaultInsAsmRepos.sm_61.txt.
#[test]
fn maxwell_cuasm_mov_exit_nop() {
    assert_eq!(d(0xe30000000007000f), Instr::Exit);
    assert_eq!(d(0x50b0000000070f00), Instr::Nop);
    // @P0 EXIT / @!P0 EXIT CC.NEU: predicate decoded, CC test rejected.
    let p = decode(0xe30000000000000f).unwrap();
    assert_eq!((p.pred, p.pred_neg), (0, false));
    assert!(decode(0xe30000000008000d).is_err());
    assert_eq!(d(0x4c98078005170003), Instr::Mov { dst: 3, src: Src::Cbuf { bank: 0, offset: 0x144 } });
    assert_eq!(d(0x4c98078800170007), Instr::Mov { dst: 7, src: Src::Cbuf { bank: 2, offset: 0x4 } });
    assert_eq!(d(0x5c98078000270000), Instr::Mov { dst: 0, src: Src::Reg(2) });
    assert_eq!(d(0x0103f8000007f009), Instr::Mov { dst: 9, src: Src::Imm(0x3f80_0000) });
    let p = decode(0x010ffffffc08f007).unwrap();
    assert_eq!((p.pred, p.pred_neg), (0, true));
    assert_eq!(p.instr, Instr::Mov { dst: 7, src: Src::Imm(0xffff_ffc0) });
}

#[test]
fn maxwell_cuasm_fadd_fmul_ffma() {
    let ff = |dst, a, b, c, sat| Instr::FFma { dst, a, b, c, sat, ftz: false };
    // FADD R6, -R2, -RZ
    assert_eq!(d(0x5c5920000ff70206), Instr::FAdd { dst: 6, a: neg(Src::Reg(2)), b: neg(Src::Reg(RZ)), sat: false, ftz: false });
    // FADD.FTZ.SAT R4, R4, -RZ
    assert_eq!(d(0x5c5c30000ff70404), Instr::FAdd { dst: 4, a: reg(4), b: neg(Src::Reg(RZ)), sat: true, ftz: true });
    // FADD.FTZ R6, R5, c[0x0][0x164]
    assert_eq!(
        d(0x4c58100005970506),
        Instr::FAdd { dst: 6, a: reg(5), b: plain(Src::Cbuf { bank: 0, offset: 0x164 }), sat: false, ftz: true }
    );
    // FADD.FTZ.RM: directed rounding has no WGSL equivalent.
    assert!(matches!(decode(0x5c58108002072323), Err(DecodeError::Unsupported { .. })));
    // @P1 FADD32I R3, -R3, 1.5707964
    assert_eq!(d(0x0903fc90fdb10303), Instr::FAdd { dst: 3, a: neg(Src::Reg(3)), b: plain(Src::Imm(0x3fc9_0fdb)), sat: false, ftz: false });
    assert_eq!(d(0x5c68000000570b05), Instr::FMul { dst: 5, a: reg(11), b: reg(5), sat: false, ftz: false });
    // FMUL32I R15, R21, -1.4426950 (sign lives in the 32-bit immediate).
    assert_eq!(d(0x1e0bfb8aa3b7150f), Instr::FMul { dst: 15, a: reg(21), b: plain(Src::Imm(0xbfb8_aa3b)), sat: false, ftz: false });
    // FMUL.D2 R0, R3, 0.5: the float immediate decodes to 0.5, the scale is rejected.
    assert!(matches!(decode(0x3868023f00070300), Err(DecodeError::Unsupported { what: "FMUL scale (.D2/.M2 ...)", .. })));
    assert_eq!(
        d(0x3868003f00070300),
        Instr::FMul { dst: 0, a: reg(3), b: plain(Src::Imm(0x3f00_0000)), sat: false, ftz: false }
    );
    let c2 = |o| plain(Src::Cbuf { bank: 2, offset: o });
    // FFMA R6, R2, -R0, c[0x2][0x0]
    assert_eq!(d(0x5181000800070206), ff(6, reg(2), neg(Src::Reg(0)), c2(0x0), false));
    assert_eq!(d(0x5180030800170007), ff(7, reg(0), reg(6), c2(0x4), false));
    // FFMA.SAT R20, R14, c[0x2][0x34], R20
    assert_eq!(d(0x49840a0800d70e14), ff(20, reg(14), c2(0x34), reg(20), true));
}

#[test]
fn maxwell_cuasm_mufu_ldc() {
    assert_eq!(d(0x5080000000470200), Instr::Mufu { dst: 0, op: MufuOp::Rcp, src: reg(2) });
    assert_eq!(d(0x5080000000570202), Instr::Mufu { dst: 2, op: MufuOp::Rsq, src: reg(2) });
    // MUFU.RCP64H is outside the subset.
    assert!(decode(0x5080000000670b09).is_err());
    // LDC.U8 / LDC.U16: real words, rejected for size; bank/offset fields still line up.
    assert!(matches!(decode(0xef9000001807ff00), Err(DecodeError::Unsupported { what: "LDC size other than 32-bit", .. })));
    let ldc32 = 0xef9200301047ff16 & !(7 << 48) | (4 << 48);
    assert_eq!(d(ldc32), Instr::Ldc { dst: 0x16, bank: 3, offset: 0x104 });
}

#[test]
fn maxwell_graphics_ops_roundtrip() {
    assert_eq!(d(mx::ald(0, 0x80, 3)), Instr::Ald { dst: 0, addr: 0x80, comps: 3 });
    assert_eq!(d(mx::ast(4, 0x70, 4)), Instr::Ast { src: 4, addr: 0x70, comps: 4 });
    assert_eq!(d(mx::ipa(1, 0x84, 1, 3)), Instr::Ipa { dst: 1, addr: 0x84, freq: IpaFreq::PassMulW, mul: 3 });
    assert_eq!(d(mx::ipa(1, 0x7c, 0, RZ)), Instr::Ipa { dst: 1, addr: 0x7c, freq: IpaFreq::Pass, mul: RZ });
    assert_eq!(decode(mx::ipa(2, 0x90, 2, RZ)).unwrap().to_string(), "IPA.CONSTANT R2, a[0x90]");
    assert_eq!((instr_offset(0), instr_offset(2), instr_offset(3)), (8, 24, 40));
}

/// The deko3d triangle: `gl_Position = vec4(inPos, 1.0); outColor = inColor;`, written
/// the way nouveau-style compilers lay it out.
fn triangle_vs() -> Vec<u8> {
    let mut h = SphBuilder::vertex();
    for a in [0x80, 0x84, 0x88, 0x90, 0x94, 0x98] {
        h = h.imap(a);
    }
    for a in [0x70, 0x74, 0x78, 0x7c, 0x80, 0x84, 0x88] {
        h = h.omap(a);
    }
    program_mx(
        &h,
        &[mx::ald(0, 0x80, 3), mx::ald(4, 0x90, 3), mx::mov32i(3, 1.0f32.to_bits()), mx::ast(0, 0x70, 4), mx::ast(4, 0x80, 3), mx::exit()],
    )
}

/// `outColor = vec4(inColor, 1.0);`
fn triangle_fs() -> Vec<u8> {
    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..3 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    program_mx(
        &h,
        &[
            mx::ipa(0, 0x7c, 0, RZ),
            mx::mufu_rcp(4, 0),
            mx::ipa(0, 0x80, 1, 4),
            mx::ipa(1, 0x84, 1, 4),
            mx::ipa(2, 0x88, 1, 4),
            mx::mov32i(3, 1.0f32.to_bits()),
            mx::exit(),
        ],
    )
}

#[test]
fn maxwell_triangle_translates_and_validates() {
    let vs = translate_maxwell(ShaderStage::Vertex, &triangle_vs()).unwrap();
    validate(&vs.wgsl);
    assert_eq!(vs.interface.vertex_inputs, vec![0, 1]);
    assert!(vs.interface.writes_position);
    let fs = translate_for_class(crate::gpu::ampere::methods::MAXWELL_B, ShaderStage::Fragment, &triangle_fs()).unwrap();
    validate(&fs.wgsl);
    assert_eq!(fs.interface.color_targets, vec![0]);
    assert_eq!(vs.interface.varyings, fs.interface.varyings);
    assert!(fs.wgsl.contains("vec4<f32>(v2, v3, v4, 1.0f)"), "{}", fs.wgsl);
}

/// The same program through either front end gives the same WGSL as the Ampere goldens.
#[test]
fn maxwell_matches_ampere_goldens() {
    let h = SphBuilder::vertex().imap(0x80).imap(0x84).imap(0x88).imap(0x8c).omap(0x70).omap(0x74).omap(0x78).omap(0x7c);
    let code = [mx::ald(0, 0x80, 4), mx::ald(4, 0x90, 4), mx::ast(0, 0x70, 4), mx::ast(4, 0x80, 4), mx::exit()];
    let mx_vs = translate_maxwell(ShaderStage::Vertex, &program_mx(&h, &code)).unwrap();
    golden("passthrough.vert.wgsl", include_str!("golden/passthrough.vert.wgsl"), &mx_vs.wgsl);
    assert_eq!(mx_vs.wgsl, translate(ShaderStage::Vertex, &passthrough_vs()).unwrap().wgsl);

    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..4 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    let mut code = vec![mx::ipa(4, 0x7c, 0, RZ), mx::mufu_rcp(4, 4)];
    for c in 0..4u8 {
        code.push(mx::ipa(c, 0x80 + 4 * c as u16, 1, 4));
    }
    code.push(mx::exit());
    let mx_fs = translate_maxwell(ShaderStage::Fragment, &program_mx(&h, &code)).unwrap();
    golden("varying.frag.wgsl", include_str!("golden/varying.frag.wgsl"), &mx_fs.wgsl);
}

#[test]
fn maxwell_rejects_and_reports_offsets() {
    let vs = SphBuilder::vertex();
    // Control words are skipped, so a missing EXIT is still missing.
    assert_eq!(translate_maxwell(ShaderStage::Vertex, &program_mx(&vs, &[mx::nop()])).unwrap_err(), XlateError::MissingExit);
    // Unknown opcode (IMAD 0x5a00) in bundle 1, slot 0: byte offset 0x28.
    let imad = 0x5a00_0000_0007_0000;
    match translate_maxwell(ShaderStage::Vertex, &program_mx(&vs, &[mx::nop(), mx::nop(), mx::nop(), imad, mx::exit()])).unwrap_err() {
        XlateError::Decode { at, err: DecodeError::UnknownOpcode { opcode: 0x5a00 }, .. } => assert_eq!(at, 0x28),
        e => panic!("{e:?}"),
    }
    // Ampere-sized header on a Maxwell shader is the caller's mistake, not ours: the
    // class picks the front end, and the B197 path uses the 0x50-byte header.
    assert_eq!(
        translate_maxwell(ShaderStage::Fragment, &program_mx(&vs, &[mx::exit()])).unwrap_err(),
        XlateError::StageMismatch(ShaderStage::Fragment)
    );
}
