//! Control flow, predicates, integer ops and conversions (both front ends).
//!
//! `*_cuasm_*` tests decode real `nvdisasm` words from CuAssembler's
//! `DefaultInsAsmRepos.sm_61.txt` / `.sm_86.txt` (low 64 bits for sm_61; the extra top byte
//! there is reuse flags). The program tests run the lowered IR through [`eval`] to check
//! the if-conversion, and validate the emitted WGSL with naga.

use super::decode::{BoolOp, Compare, DecodeError, FCmp, FSrc, ICmp, Instr, PSrc, Round, Src, XmadMode, PT, RZ};
use super::ir::{Function, Op};
use super::maxwell::{decode, instr_offset};
use super::sph::SphBuilder;
use super::tests::validate;
use super::*;

/// Runs `f` with `cbuf(bank, offset)` as the only input; returns every value's bits.
pub(super) fn run(f: &Function, cbuf: &dyn Fn(u8, u16) -> u32) -> Vec<u32> {
    let mut v = Vec::new();
    for op in &f.values {
        let bits = match *op {
            Op::Cbuf { bank, offset } => cbuf(bank, offset),
            Op::CbufIndexed { bank, offset, reg } => {
                let off = offset.wrapping_add(v[reg.0 as usize] as u16);
                cbuf(bank, off)
            }
            ref op => eval::op(op, &v).unwrap_or(0),
        };
        v.push(bits);
    }
    v
}

pub(super) fn exit_reg(f: &Function, v: &[u32], r: usize) -> u32 {
    f.exit_regs[r].map_or(0, |x| v[x.0 as usize])
}

fn r(n: u8) -> Src {
    Src::Reg(n)
}

fn f(src: Src) -> FSrc {
    FSrc { src, neg: false, abs: false }
}

fn p(n: u8) -> PSrc {
    PSrc { p: n, neg: false }
}

fn np(n: u8) -> PSrc {
    PSrc { p: n, neg: true }
}

fn d(w: u64) -> Instr {
    decode(w).unwrap_or_else(|e| panic!("{w:#018x}: {e}")).instr
}

#[test]
fn maxwell_cuasm_ctl_compare_select() {
    // ISETP.NE.AND P0, PT, R0, R2, PT
    let i = Compare::I { cmp: ICmp::Ne, signed: true, a: r(0), b: r(2) };
    assert_eq!(d(0x5b6b038000270007), Instr::Setp { pd: 0, pd2: PT, cmp: i, bop: BoolOp::And, c: PSrc::TRUE });
    // ISETP.LT.OR P0, PT, R3, R25, P0
    let i = Compare::I { cmp: ICmp::Lt, signed: true, a: r(3), b: r(25) };
    assert_eq!(d(0x5b63200001970307), Instr::Setp { pd: 0, pd2: PT, cmp: i, bop: BoolOp::Or, c: p(0) });
    // ISETP.LT.AND P0, PT, RZ, c[0x0][0x174], PT
    let i = Compare::I { cmp: ICmp::Lt, signed: true, a: r(RZ), b: Src::Cbuf { bank: 0, offset: 0x174 } };
    assert_eq!(d(0x4b63038005d7ff07), Instr::Setp { pd: 0, pd2: PT, cmp: i, bop: BoolOp::And, c: PSrc::TRUE });
    // ISETP.GE.U32.X: the extended (64-bit) form is rejected.
    assert!(decode(0x5b6c0b800067ff07).is_err());
    // FSETP.NEU.FTZ.AND P0, PT, |R2|, +INF, PT
    let a = FSrc { src: r(2), neg: false, abs: true };
    let c = Compare::F { cmp: FCmp::Neu, a, b: f(Src::Imm(0x7f80_0000)) };
    assert_eq!(d(0x36bd83ff80070287), Instr::Setp { pd: 0, pd2: PT, cmp: c, bop: BoolOp::And, c: PSrc::TRUE });
    // FSETP.GT.AND P1, PT, R2, 256, P1
    let c = Compare::F { cmp: FCmp::Gt, a: f(r(2)), b: f(Src::Imm(0x4380_0000)) };
    assert_eq!(d(0x36b400c38007020f), Instr::Setp { pd: 1, pd2: PT, cmp: c, bop: BoolOp::And, c: p(1) });
    // FSETP.EQ.AND P0, PT, R12, 1, !P0
    let c = Compare::F { cmp: FCmp::Eq, a: f(r(12)), b: f(Src::Imm(0x3f80_0000)) };
    assert_eq!(d(0x36b2043f80070c07), Instr::Setp { pd: 0, pd2: PT, cmp: c, bop: BoolOp::And, c: np(0) });
    // PSETP.OR.AND P0, PT, !P0, !P1, PT
    let c = Compare::P { op: BoolOp::Or, a: np(0), b: np(1) };
    assert_eq!(d(0x5090038121078007), Instr::Setp { pd: 0, pd2: PT, cmp: c, bop: BoolOp::And, c: PSrc::TRUE });
    // PSETP.AND.AND P2, PT, PT, P1, P5
    let c = Compare::P { op: BoolOp::And, a: PSrc::TRUE, b: p(1) };
    assert_eq!(d(0x5090028020077017), Instr::Setp { pd: 2, pd2: PT, cmp: c, bop: BoolOp::And, c: p(5) });
    // FSET.GTU.FTZ.AND R7, |R4|, +INF, PT; FSET.EQ.AND RZ.CC is rejected for .CC.
    let c = Compare::F { cmp: FCmp::Gtu, a: FSrc { src: r(4), neg: false, abs: true }, b: f(Src::Imm(0x7f80_0000)) };
    assert_eq!(d(0x30cc03ff80070407), Instr::Set { dst: 7, cmp: c, bop: BoolOp::And, c: PSrc::TRUE, bf: false });
    assert!(decode(0x300283bf800702ff).is_err());
    // FSET.LT.AND R10, R11, 1, PT
    let c = Compare::F { cmp: FCmp::Lt, a: f(r(11)), b: f(Src::Imm(0x3f80_0000)) };
    assert_eq!(d(0x300103bf80070b0a), Instr::Set { dst: 10, cmp: c, bop: BoolOp::And, c: PSrc::TRUE, bf: false });
    // ISET.BF.LT.AND R6, R0, c[0x0][0x160], PT; ISET.GE.U32.AND R2, R2, c[0x0][0x144], PT
    let c = Compare::I { cmp: ICmp::Lt, signed: true, a: r(0), b: Src::Cbuf { bank: 0, offset: 0x160 } };
    assert_eq!(d(0x4b53138005870006), Instr::Set { dst: 6, cmp: c, bop: BoolOp::And, c: PSrc::TRUE, bf: true });
    let c = Compare::I { cmp: ICmp::Ge, signed: false, a: r(2), b: Src::Cbuf { bank: 0, offset: 0x144 } };
    assert_eq!(d(0x4b5c038005170202), Instr::Set { dst: 2, cmp: c, bop: BoolOp::And, c: PSrc::TRUE, bf: false });
    // SEL R0, RZ, 0x1, !P0; SEL R16, R16, -0x1, P0
    assert_eq!(d(0x38a004000017ff00), Instr::Sel { dst: 0, a: r(RZ), b: Src::Imm(1), c: np(0) });
    assert_eq!(d(0x39a0007ffff71010), Instr::Sel { dst: 16, a: r(16), b: Src::Imm(u32::MAX), c: p(0) });
    // FMNMX R6, |R4|, 1, !PT (max); @!P6 FMNMX R2, R2, 255, PT (min)
    let a = FSrc { src: r(4), neg: false, abs: true };
    assert_eq!(d(0x386047bf80070406), Instr::FMnmx { dst: 6, a, b: f(Src::Imm(0x3f80_0000)), c: np(PT) });
    assert_eq!(d(0x386003c37f0e0202), Instr::FMnmx { dst: 2, a: f(r(2)), b: f(Src::Imm(0x437f_0000)), c: PSrc::TRUE });
    // IMNMX.XHI R13.CC / IMNMX.U32.XLO: extended forms rejected.
    assert!(decode(0x5c219f8000570b0d).is_err());
    assert!(decode(0x5c200f800ff71717).is_err());
    // FCMP.NEU R5, R5, RZ, R12; FCMP.GEU R10, R11, RZ, R10
    assert_eq!(d(0x5bad06000ff70505), Instr::FCmpSel { dst: 5, cmp: FCmp::Neu, a: r(5), b: r(RZ), c: r(12) });
    assert_eq!(d(0x5bae05000ff70b0a), Instr::FCmpSel { dst: 10, cmp: FCmp::Geu, a: r(11), b: r(RZ), c: r(10) });
}

#[test]
fn maxwell_cuasm_ctl_integer() {
    let add = |dst, a, b, n0, n1| Instr::IAdd { dst, a, b, c: r(RZ), neg: [n0, n1, false] };
    // IADD R2, -R2, RZ; IADD R7, R0, R7; IADD R7.CC is rejected.
    assert_eq!(d(0x5c1200000ff70202), add(2, r(2), r(RZ), true, false));
    assert_eq!(d(0x5c10000000770007), add(7, r(0), r(7), false, false));
    assert!(decode(0x5c10800000770007).is_err());
    // IADD32I R10, R3, -0x1; @!P1 IADD32I R7, R7, 0x40
    assert_eq!(d(0x1c0ffffffff7030a), add(10, r(3), Src::Imm(u32::MAX), false, false));
    assert_eq!(d(0x1c00000004090707), add(7, r(7), Src::Imm(0x40), false, false));
    // ISCADD R5, -R0, R5, 0x17; ISCADD32I R4, R3, -0x3f800000, 0x17
    assert_eq!(d(0x5c1a0b8000570005), Instr::IScAdd { dst: 5, a: r(0), b: r(5), shift: 0x17, neg: [true, false] });
    assert_eq!(
        d(0x16ec080000070304),
        Instr::IScAdd { dst: 4, a: r(3), b: Src::Imm(0xc080_0000), shift: 0x17, neg: [false, false] }
    );
    let xmad = |dst, a, b, c, a_hi, b_hi, psl, mrg, mode| Instr::Xmad {
        dst,
        a,
        b,
        c,
        a_signed: false,
        b_signed: false,
        a_hi,
        b_hi,
        psl,
        mrg,
        mode,
    };
    let c0 = |o| Src::Cbuf { bank: 0, offset: o };
    // XMAD R3, R0, c[0x0][0x8], RZ; XMAD.MRG R5, R0, c[0x0][0x8].H1, RZ
    assert_eq!(d(0x4e007f8000270003), xmad(3, r(0), c0(8), r(RZ), false, false, false, false, XmadMode::None));
    assert_eq!(d(0x4f107f8000270005), xmad(5, r(0), c0(8), r(RZ), false, true, false, true, XmadMode::None));
    // XMAD.PSL R2, R1, c[0x0][0x1c0].H1, R2; XMAD.CHI R11, R0.H1, c[0x0][0x148], R11
    assert_eq!(d(0x4e90010007070102), xmad(2, r(1), c0(0x1c0), r(2), false, true, true, false, XmadMode::None));
    assert_eq!(d(0x4e2805800527000b), xmad(11, r(0), c0(0x148), r(11), true, false, false, false, XmadMode::Chi));
    // XMAD.CLO R8, R7, c[0x0][0x198], R5
    assert_eq!(d(0x4e04028006670708), xmad(8, r(7), c0(0x198), r(5), false, false, false, false, XmadMode::Clo));
    // XMAD.U16.S16 R10, R6, c[0x0][0x15c].H1, RZ
    match d(0x4e127f800577060a) {
        Instr::Xmad { a_signed: false, b_signed: true, b_hi: true, .. } => {}
        i => panic!("{i:?}"),
    }
    // XMAD.S16.S16.CSFU is rejected.
    assert!(decode(0x4e3f7f800577060b).is_err());
    // XMAD R7, R9, R6, RZ; XMAD.MRG R16, R9, R6.H1, RZ; XMAD.PSL.CBCC R0, R0.H1, R5.H1, R3
    assert_eq!(d(0x5b007f8000670907), xmad(7, r(9), r(6), r(RZ), false, false, false, false, XmadMode::None));
    assert_eq!(d(0x5b007fa800670910), xmad(16, r(9), r(6), r(RZ), false, true, false, true, XmadMode::None));
    assert_eq!(d(0x5b30019800570000), xmad(0, r(0), r(5), r(3), true, true, true, false, XmadMode::Cbcc));
    // XMAD.CHI R19, R18.H1, R25, R19; XMAD.S16.S16 R19, R8, R29, R19
    assert_eq!(d(0x5b28098001971213), xmad(19, r(18), r(25), r(19), true, false, false, false, XmadMode::Chi));
    match d(0x5b03098001d70813) {
        Instr::Xmad { a_signed: true, b_signed: true, a_hi: false, b_hi: false, .. } => {}
        i => panic!("{i:?}"),
    }
    let lop = |dst, lut, a, b, c| Instr::Lop3 { dst, lut, a, b, c };
    // LOP32I.AND R2, R2, 0x80000000; LOP32I.OR R4, R0, 0x800000
    assert_eq!(d(0x0408000000070202), lop(2, 0xc0, r(2), Src::Imm(0x8000_0000), r(RZ)));
    assert_eq!(d(0x0420080000070004), lop(4, 0xfc, r(0), Src::Imm(0x80_0000), r(RZ)));
    // LOP.AND.NZ P0, RZ, ~R7, ~R8: predicate output rejected (inversions decode).
    assert!(decode(0x5c403180008707ff).is_err());
    // LOP3.LUT R0, R0, 0x1, R5, 0xf8; LOP3.LUT R13, R17, -0x10000, R12, 0xf8
    assert_eq!(d(0x3cf8028000170000), lop(0, 0xf8, r(0), Src::Imm(1), r(5)));
    assert_eq!(d(0x3df806700007110d), lop(13, 0xf8, r(17), Src::Imm(0xffff_0000), r(12)));
    // LOP3.LUT R9, R4, c[0x0][0x148], R9, 0x96
    assert_eq!(d(0x0296048005270409), lop(9, 0x96, r(4), c0(0x148), r(9)));
    // SHL R5, R4, R5; SHL.W R8, R8, R10; SHR.U32 R3, R4, R3
    assert_eq!(d(0x5c48000000570405), Instr::Shl { dst: 5, a: r(4), b: r(5), wrap: false });
    assert_eq!(d(0x5c48008000a70808), Instr::Shl { dst: 8, a: r(8), b: r(10), wrap: true });
    assert_eq!(d(0x5c28000000370403), Instr::Shr { dst: 3, a: r(4), b: r(3), signed: false, wrap: false });
}

#[test]
fn maxwell_cuasm_ctl_convert_branch() {
    let c0 = |o| Src::Cbuf { bank: 0, offset: o };
    // I2F.F32.U32 R17, c[0x0][0x8]; I2F.F32.S32 R9, c[0x0][0x148]; .RP and |x| rejected.
    assert_eq!(d(0x4cb8000000270a11), Instr::I2F { dst: 17, src: c0(8), signed: false });
    assert_eq!(d(0x4cb8000005272a09), Instr::I2F { dst: 9, src: c0(0x148), signed: true });
    assert!(decode(0x4cb8010005c70a02).is_err());
    assert!(decode(0x4cba010005872a09).is_err());
    // F2I.S32.F32 R4, R0 (round to nearest); F2I.FTZ.U32.F32.TRUNC R18, R18; 64-bit rejected.
    assert_eq!(d(0x5cb0000000071a04), Instr::F2I { dst: 4, src: f(r(0)), signed: true, round: Round::Nearest });
    assert_eq!(d(0x5cb0118001270a12), Instr::F2I { dst: 18, src: f(r(18)), signed: false, round: Round::Trunc });
    assert!(decode(0x5cb0018001070b10).is_err());
    // F2F.F32.F16 / F2F.F16.F32: half formats rejected.
    assert!(decode(0x5ca8018000270613).is_err());
    assert!(decode(0x5ca8000000070900).is_err());
    // Branch targets are relative to the next instruction (the CuAsm listing shows them
    // absolute): BRA 0xe0 at 0xe8; @!P0 BRA 0x130 at 0x110; SSY 0x970 at 0x30; PBK 0x20 at ...
    let at = |w: u64, pc: i32| match d(w) {
        Instr::Bra { rel } | Instr::Ssy { rel } | Instr::Pbk { rel } => pc + rel,
        i => panic!("{i:?}"),
    };
    assert_eq!(at(0xe2400fffff07000f, 0xe8), 0xe0);
    assert_eq!(at(0xe24000000188000f, 0x110), 0x130);
    assert_eq!(at(0xe24000003700000f, 0x1e8), 0x560);
    assert_eq!(at(0xe290000093800000, 0x30), 0x970);
    assert_eq!(at(0xe290000043800000, 0x1f8), 0x638);
    let pd = decode(0xe24000000188000f).unwrap();
    assert_eq!((pd.pred, pd.pred_neg), (0, true));
    // BRA CC.NEU: condition codes rejected. BRA.U is a uniformity hint and decodes.
    assert!(decode(0xe2400fffdf84000d).is_err());
    assert!(matches!(d(0xe24000002b09008f), Instr::Bra { .. }));
    assert_eq!(d(0xf0f800000007000f), Instr::Sync);
    assert_eq!(d(0xe34000000007000f), Instr::Brk);
    assert!(decode(0xe34000000008000d).is_err());
}

/// Test-only Maxwell encoders, same fields as the decoder (checked above against CuAsm).
mod mx {
    pub const NOP: u64 = 0x50b0000000070f00;
    pub const EXIT: u64 = 0xe30000000007000f;
    pub const SYNC: u64 = 0xf0f800000007000f;
    pub const KIL: u64 = 0xe33000000007000f;
    pub fn mov32i(d: u8, imm: u32) -> u64 {
        (0x0100 << 48) | (7 << 16) | ((imm as u64) << 20) | (0xf << 12) | d as u64
    }
    pub fn mov_cb(d: u8, bank: u8, off: u16) -> u64 {
        0x4c98_0780_0000_0000 | ((bank as u64) << 34) | (((off / 4) as u64) << 20) | (7 << 16) | d as u64
    }
    /// `FSETP.cmp.AND Pd, PT, Ra, imm, PT`.
    pub fn fsetp_imm(pd: u8, a: u8, cmp: u8, imm: f32) -> u64 {
        let b = imm.to_bits();
        ((0x36b0 | cmp as u64) << 48)
            | (((b >> 31) as u64) << 56)
            | (7 << 39)
            | ((((b >> 12) & 0x7ffff) as u64) << 20)
            | (7 << 16)
            | ((a as u64) << 8)
            | ((pd as u64) << 3)
            | 7
    }
    fn rel(from: usize, to: usize) -> u64 {
        let off = super::instr_offset(to) as i64 - (super::instr_offset(from) as i64 + 8);
        ((off as u64) & 0xff_ffff) << 20
    }
    pub fn bra(from: usize, to: usize) -> u64 {
        (0xe240 << 48) | rel(from, to) | (7 << 16) | 0xf
    }
    pub fn ssy(from: usize, to: usize) -> u64 {
        (0xe290 << 48) | rel(from, to)
    }
    pub fn pred(w: u64, p: u8, neg: bool) -> u64 {
        (w & !(0xf << 16)) | ((p as u64) << 16) | ((neg as u64) << 19)
    }
}

fn code_mx(code: &[u64]) -> Vec<u8> {
    let mut b = Vec::new();
    for chunk in code.chunks(3) {
        b.extend_from_slice(&0x001f_8000_fc00_07e0u64.to_le_bytes());
        for i in 0..3 {
            b.extend_from_slice(&chunk.get(i).copied().unwrap_or(mx::NOP).to_le_bytes());
        }
    }
    b
}

fn lower_ps(code: &[u64]) -> Result<Function, XlateError> {
    maxwell::lower(ShaderStage::Fragment, &code_mx(code))
}

/// Pixel shader writing R0..R3 to target 0, for WGSL validation.
fn wgsl_ps(code: &[u64]) -> String {
    let mut b = SphBuilder::pixel().ps_target(0, 0xf).bytes(MAXWELL_SPH_SIZE);
    b.extend(code_mx(code));
    let t = translate_maxwell(ShaderStage::Fragment, &b).unwrap();
    validate(&t.wgsl);
    t.wgsl
}

fn one(x: f32) -> impl Fn(u8, u16) -> u32 {
    move |_, _| x.to_bits()
}

#[test]
fn branch_if_else_is_if_converted() {
    // R0 = c[0][0]; R1 = R0 > 0.5 ? 2.0 : 1.0 through BRA, then the same through SSY/SYNC.
    let bra = [
        mx::mov_cb(0, 0, 0),
        mx::fsetp_imm(0, 0, 4, 0.5),
        mx::pred(mx::bra(2, 5), 0, false),
        mx::mov32i(1, 1.0f32.to_bits()),
        mx::bra(4, 6),
        mx::mov32i(1, 2.0f32.to_bits()),
        mx::EXIT,
    ];
    let ssy = [
        mx::mov_cb(0, 0, 0),
        mx::fsetp_imm(0, 0, 4, 0.5),
        mx::ssy(2, 8),
        mx::pred(mx::bra(3, 6), 0, false),
        mx::mov32i(1, 1.0f32.to_bits()),
        mx::SYNC,
        mx::mov32i(1, 2.0f32.to_bits()),
        mx::SYNC,
        mx::EXIT,
    ];
    for code in [&bra[..], &ssy[..]] {
        let f = lower_ps(code).unwrap();
        for (x, want) in [(0.7, 2.0f32), (0.2, 1.0), (0.5, 1.0)] {
            let v = run(&f, &one(x));
            assert_eq!(f32::from_bits(exit_reg(&f, &v, 1)), want, "c = {x}");
        }
        assert!(wgsl_ps(code).contains("select("));
    }
}

#[test]
fn predicated_exit_and_kill() {
    // @P0 EXIT; R1 = 3.0; @!P0 KIL; EXIT. P0 = c > 0.5.
    let code = [
        mx::mov_cb(0, 0, 0),
        mx::fsetp_imm(0, 0, 4, 0.5),
        mx::mov32i(1, 5.0f32.to_bits()),
        mx::pred(mx::EXIT, 0, false),
        mx::mov32i(1, 3.0f32.to_bits()),
        mx::pred(mx::KIL, 1, false),
        mx::EXIT,
    ];
    let f = lower_ps(&code).unwrap();
    // P1 is never written (false), so KIL never fires; R1 stays 5.0 for threads that left early.
    let v = run(&f, &one(0.7));
    assert_eq!(f32::from_bits(exit_reg(&f, &v, 1)), 5.0);
    let v = run(&f, &one(0.2));
    assert_eq!(f32::from_bits(exit_reg(&f, &v, 1)), 3.0);
    assert_eq!(f.discard, None, "a KIL under a constant-false predicate is dropped");
    // Unconditional KIL after a predicated exit: only threads that stayed are discarded.
    let mut code = code;
    code[5] = mx::KIL;
    let f = lower_ps(&code).unwrap();
    for (x, killed) in [(0.7, 0), (0.2, 1)] {
        let v = run(&f, &one(x));
        assert_eq!(v[f.discard.unwrap().0 as usize], killed, "c = {x}");
    }
    assert!(wgsl_ps(&code).contains("discard;"));
}

#[test]
fn rejects_loops_and_bad_targets() {
    let back = [mx::mov32i(0, 0), mx::bra(1, 0), mx::EXIT];
    assert!(matches!(lower_ps(&back), Err(XlateError::Unsupported { what: "backward branch (loop)", .. })));
    assert!(matches!(lower_ps(&[mx::SYNC, mx::EXIT]), Err(XlateError::Unsupported { .. })));
    // A branch into a control word.
    let w = (0xe240 << 48) | (((-16i64 as u64) & 0xff_ffff) << 20) | (7 << 16) | 0xf;
    assert!(matches!(lower_ps(&[w, mx::EXIT]), Err(XlateError::Unsupported { what: "branch target is not an instruction", .. })));
    // KIL in a vertex shader.
    assert!(matches!(
        maxwell::lower(ShaderStage::Vertex, &code_mx(&[mx::KIL, mx::EXIT])),
        Err(XlateError::Unsupported { .. })
    ));
    // Code after a jump that nothing reaches is not decoded (garbage is fine there).
    let skip = [mx::bra(0, 2), 0xffff_ffff_ffff_ffff, mx::EXIT];
    assert!(lower_ps(&skip).is_ok());
}

#[test]
fn xmad_sequence_multiplies() {
    // nvcc's 32-bit multiply, real sm_61 words for the first two:
    // XMAD R7, R9, R6, RZ; XMAD.MRG R16, R9, R6.H1, RZ; XMAD.PSL.CBCC R2, R9.H1, R16.H1, R7.
    let cbcc = (0x5b30019800570000u64 & !0xffu64 & !(0xff << 8) & !(0xff << 20) & !(0xff << 39))
        | 2
        | (9 << 8)
        | (16 << 20)
        | (7 << 39);
    for (a, b) in [(3u32, 5u32), (0x1234_5678, 0x9abc_def0), (u32::MAX, 7), (0xffff, 0x10001), (0, 9)] {
        let code = [mx::mov32i(9, a), mx::mov32i(6, b), 0x5b007f8000670907, 0x5b007fa800670910, cbcc, mx::EXIT];
        let f = lower_ps(&code).unwrap();
        let v = run(&f, &one(0.0));
        assert_eq!(exit_reg(&f, &v, 2), a.wrapping_mul(b), "{a:#x} * {b:#x}");
    }
    let code = [mx::mov_cb(9, 0, 0), mx::mov_cb(6, 0, 4), 0x5b007f8000670907, 0x5b007fa800670910, cbcc, mx::EXIT];
    wgsl_ps(&code);
}

#[test]
fn integer_and_conversion_ops_run_and_validate() {
    // R0 = c[0][0] (float), R1 = F2I.S32 R0, R2 = I2F.S32 R1, R3 = R1 == 2 ? 1.0 : 0 (ISET.BF).
    let code = [
        mx::mov_cb(0, 0, 0),
        0x5cb0000000071a01, // F2I.S32.F32 R1, R0 (nearest)
        0x5cb8000000172a02, // I2F.F32.S32 R2, R1
        0x3655138000270103, // ISET.BF.EQ.AND R3, R1, 0x2, PT
        0x5c48000000070104, // SHL R4, R1, R0 (amount = float bits, >= 32 gives 0)
        mx::EXIT,
    ];
    let f = lower_ps(&code).unwrap();
    let v = run(&f, &one(2.5));
    assert_eq!(exit_reg(&f, &v, 1), 2, "2.5 rounds to even");
    assert_eq!(f32::from_bits(exit_reg(&f, &v, 2)), 2.0);
    assert_eq!(f32::from_bits(exit_reg(&f, &v, 3)), 1.0);
    assert_eq!(exit_reg(&f, &v, 4), 0);
    let v = run(&f, &one(-3.5));
    assert_eq!(exit_reg(&f, &v, 1) as i32, -4);
    assert_eq!(exit_reg(&f, &v, 3), 0);
    wgsl_ps(&code);
}

#[test]
fn lop3_tables() {
    let (a, b, c) = (0xf0f0_1234u32, 0xcccc_5678, 0xaaaa_9abc);
    assert_eq!(eval::lop3(0xf8, a, b, c), a | (b & c));
    assert_eq!(eval::lop3(0x96, a, b, c), a ^ b ^ c);
    assert_eq!(eval::lop3(0xc0, a, b, c), a & b);
    let _ = DecodeError::UnknownOpcode { opcode: 0 };
}

// --- Ampere (sm_86 words; control bits 105..128 are zero in CuAsm's listing) ----------------

fn da(w: u128) -> Instr {
    super::decode::decode(w).unwrap_or_else(|e| panic!("{w:#034x}: {e}")).instr
}

fn da_err(w: u128) -> bool {
    super::decode::decode(w).is_err()
}

#[test]
fn ampere_cuasm_ctl_compare_select() {
    let setp = |pd, cmp, bop, c| Instr::Setp { pd, pd2: PT, cmp, bop, c };
    let ic = |cmp, signed, a, b| Compare::I { cmp, signed, a, b };
    // ISETP.NE.AND P2, PT, R8, RZ, PT
    assert_eq!(da(0x0000000003f45270000000ff0800720c), setp(2, ic(ICmp::Ne, true, r(8), r(RZ)), BoolOp::And, PSrc::TRUE));
    // ISETP.GE.U32.AND P0, PT, R0, c[0x0][0x170], PT
    let cb = Src::Cbuf { bank: 0, offset: 0x170 };
    assert_eq!(da(0x0000000003f0607000005c0000007a0c), setp(0, ic(ICmp::Ge, false, r(0), cb), BoolOp::And, PSrc::TRUE));
    // ISETP.GE.AND P0, PT, R8, -0x18, PT
    let k = Src::Imm(-0x18i32 as u32);
    assert_eq!(da(0x0000000003f06270ffffffe80800780c), setp(0, ic(ICmp::Ge, true, r(8), k), BoolOp::And, PSrc::TRUE));
    // ISETP.EQ.OR P1, PT, R20, R7, !P2
    assert_eq!(da(0x0000000005722670000000071400720c), setp(1, ic(ICmp::Eq, true, r(20), r(7)), BoolOp::Or, np(2)));
    // ISETP .EX (64-bit compare chain) is rejected.
    assert!(da_err(0x0000000003f06300000000021500720c));
    // FSETP.GTU.FTZ.AND P0, PT, |R0|, +INF, PT
    let a = FSrc { src: r(0), neg: false, abs: true };
    let c = Compare::F { cmp: FCmp::Gtu, a, b: f(Src::Imm(0x7f80_0000)) };
    assert_eq!(da(0x0000000003f1c2007f8000000000780b), setp(0, c, BoolOp::And, PSrc::TRUE));
    // FSETP.GT.AND P6, PT, R12, -R6, PT
    let c = Compare::F { cmp: FCmp::Gt, a: f(r(12)), b: FSrc { src: r(6), neg: true, abs: false } };
    assert_eq!(da(0x0000000003fc4000800000060c00720b), setp(6, c, BoolOp::And, PSrc::TRUE));
    // FSETP.LEU.OR P0, PT, R4, c[0x0][0x178], P0
    let c = Compare::F { cmp: FCmp::Leu, a: f(r(4)), b: f(Src::Cbuf { bank: 0, offset: 0x178 }) };
    assert_eq!(da(0x000000000070b40000005e0004007a0b), setp(0, c, BoolOp::Or, p(0)));
    // PLOP3.LUT P1, PT, P1, P2, PT, 0x2a, 0x0
    assert_eq!(da(0x0000000000f24572000000000000781c), Instr::PLop3 { pd: 1, lut: 0x2a, a: p(1), b: p(2), c: PSrc::TRUE });
    // SEL R24, R8, RZ, !P0; SEL R0, RZ, 0x1, !P0; FSEL R7, R4, R3, !P0
    assert_eq!(da(0x0000000004000000000000ff08187207), Instr::Sel { dst: 24, a: r(8), b: r(RZ), c: np(0) });
    assert_eq!(da(0x000000000400000000000001ff007807), Instr::Sel { dst: 0, a: r(RZ), b: Src::Imm(1), c: np(0) });
    assert_eq!(da(0x00000000040000000000000304077208), Instr::Sel { dst: 7, a: r(4), b: r(3), c: np(0) });
    // FMNMX R3, |R7|, 1, !PT (max); FMNMX R0, R0, c[0x0][0x190], PT (min); .NAN rejected.
    let a = FSrc { src: r(7), neg: false, abs: true };
    assert_eq!(da(0x00000000078002003f80000007037809), Instr::FMnmx { dst: 3, a, b: f(Src::Imm(0x3f80_0000)), c: np(PT) });
    let b = f(Src::Cbuf { bank: 0, offset: 0x190 });
    assert_eq!(da(0x00000000038000000000640000007a09), Instr::FMnmx { dst: 0, a: f(r(0)), b, c: PSrc::TRUE });
    assert!(da_err(0x0000000007820000400000000b177809));
    // IMNMX.U32 R2, R0, 0xf, PT; IMNMX R7, R4, 0x1, !PT
    assert_eq!(da(0x04000000038000000000000f00027817), Instr::IMnmx { dst: 2, a: r(0), b: Src::Imm(0xf), signed: false, c: PSrc::TRUE });
    assert_eq!(da(0x00000000078002000000000104077817), Instr::IMnmx { dst: 7, a: r(4), b: Src::Imm(1), signed: true, c: np(PT) });
}

#[test]
fn ampere_cuasm_ctl_integer() {
    let add = |dst, a, b, c, neg| Instr::IAdd { dst, a, b, c, neg };
    // IADD3 R10, R5, -0x1, RZ; IADD3 R5, R4, 0x7f, -R5; IADD3 R16, -R12, RZ, RZ; IADD3 R15, RZ, -R13, RZ
    assert_eq!(da(0x0000000007ffe0ffffffffff050a7810), add(10, r(5), Src::Imm(u32::MAX), r(RZ), [false; 3]));
    assert_eq!(da(0x0400000007ffe8050000007f04057810), add(5, r(4), Src::Imm(0x7f), r(5), [false, false, true]));
    assert_eq!(da(0x0000000007ffe1ff000000ff0c107210), add(16, r(12), r(RZ), r(RZ), [true, false, false]));
    assert_eq!(da(0x0000000007ffe0ff8000000dff0f7210), add(15, r(RZ), r(13), r(RZ), [false, true, false]));
    // IADD3 with a carry-out predicate is rejected.
    assert!(da_err(0x0400000007f1e0ff0000580004027a10));
    let mad = |dst, a, b, c| Instr::IMad { dst, a, b, c };
    // IMAD R3, R16, R5, R12; IMAD R13, R5, R14, -0x1; IMAD R2, R0, R9, c[0x0][0x1a0]
    assert_eq!(da(0x00000000078e020c0000000510037224), mad(3, r(16), r(5), r(12)));
    assert_eq!(da(0x00000000078e020effffffff050d7424), mad(13, r(5), r(14), Src::Imm(u32::MAX)));
    assert_eq!(da(0x00000000078e02090000680000027624), mad(2, r(0), r(9), Src::Cbuf { bank: 0, offset: 0x1a0 }));
    // IMAD.SHL.U32 R4, R0, 0x4, RZ; IMAD.MOV.U32 R1, RZ, RZ, c[0x0][0x28]
    assert_eq!(da(0x04000000078e00ff0000000400047824), mad(4, r(0), Src::Imm(4), r(RZ)));
    assert_eq!(da(0x00000000078e00ff00000a00ff017624), mad(1, r(RZ), r(RZ), Src::Cbuf { bank: 0, offset: 0x28 }));
    // IMAD.MOV R0, RZ, RZ, -R0 and IMAD.IADD R7, R7, 0x1, -R0 become subtractions.
    assert_eq!(da(0x00000000078e0a00000000ffff007224), add(0, r(RZ), r(RZ), r(0), [false, false, true]));
    assert_eq!(da(0x00000000078e0a000000000107077824), add(7, r(7), r(RZ), r(0), [false, false, true]));
    // LOP3.LUT R5, R5, 0xff, RZ, 0xc0, !PT; LOP3.LUT R3, R0, R3, RZ, 0xfc, !PT
    assert_eq!(da(0x00000000078ec0ff000000ff05057812), Instr::Lop3 { dst: 5, lut: 0xc0, a: r(5), b: Src::Imm(0xff), c: r(RZ) });
    assert_eq!(da(0x00000000078efcff0000000300037212), Instr::Lop3 { dst: 3, lut: 0xfc, a: r(0), b: r(3), c: r(RZ) });
    // SHF.R.U32.HI R5, RZ, 0x17, R3 = R3 >> 23; SHF.R.S32.HI R2, RZ, 0x1f, R16; SHF.L.U32 R7, R4, R7, RZ
    assert_eq!(da(0x100000000001160300000017ff057819), Instr::Shr { dst: 5, a: r(3), b: Src::Imm(0x17), signed: false, wrap: false });
    assert_eq!(da(0x00000000000114100000001fff027819), Instr::Shr { dst: 2, a: r(16), b: Src::Imm(0x1f), signed: true, wrap: false });
    assert_eq!(da(0x00000000000006ff0000000704077219), Instr::Shl { dst: 7, a: r(4), b: r(7), wrap: false });
    // SHF.L.U64.HI: a real funnel shift, rejected.
    assert!(da_err(0x00000000000102070000000200057819));
}

#[test]
fn ampere_cuasm_ctl_convert_branch() {
    // I2F.U32 R7, R18; I2FP.F32.S32 R28, R0; I2F.RP and I2F.S16 rejected.
    assert_eq!(da(0x00000000002010000000001200077306), Instr::I2F { dst: 7, src: r(18), signed: false });
    assert_eq!(da(0x000000000020140000000000001c7245), Instr::I2F { dst: 28, src: r(0), signed: true });
    assert!(da_err(0x00000000002094000000000600047306));
    assert!(da_err(0x000000000010140000000016000e7306));
    // F2I R0, R0 (S32, nearest); F2I.FTZ.U32.TRUNC.NTZ R19, R18; F2I.U32.FLOOR; F2I.S16 rejected.
    assert_eq!(da(0x00000000002011000000000000007305), Instr::F2I { dst: 0, src: f(r(0)), signed: true, round: Round::Nearest });
    assert_eq!(da(0x000000000021f0000000001200137305), Instr::F2I { dst: 19, src: f(r(18)), signed: false, round: Round::Trunc });
    assert_eq!(da(0x00000000002070000000000200137305), Instr::F2I { dst: 19, src: f(r(2)), signed: false, round: Round::Floor });
    assert!(da_err(0x00000000002029000000001200007305));
    // FRND.FLOOR R19, R2; FRND.TRUNC R37, c[0x0][0x248]
    assert_eq!(da(0x00000000002050000000000200137307), Instr::F2F { dst: 19, src: f(r(2)), round: Some(Round::Floor), sat: false });
    let cb = f(Src::Cbuf { bank: 0, offset: 0x248 });
    assert_eq!(da(0x000000000020d0000000920000257b07), Instr::F2F { dst: 37, src: cb, round: Some(Round::Trunc), sat: false });
    // BRA: offsets relative to the next instruction; CuAsm lists absolute targets.
    let at = |w: u128, pc: i32| match da(w) {
        Instr::Bra { rel } => pc + rel,
        i => panic!("{i:?}"),
    };
    assert_eq!(at(0x000000000383fffffffffff000007947, 0x110), 0x110);
    assert_eq!(at(0x00000000038000000000002000008947, 0x180), 0x1b0);
    assert_eq!(at(0x00000000038000000000051000000947, 0x2e0), 0x800);
    assert_eq!(at(0x0000000003800000000003e100009947, 0x5ea0), 0x6290);
    // A second branch predicate (BRA !P2, target) is rejected.
    assert!(da_err(0x00000000050000000000049000009947));
    assert_eq!(da(0x00000000038000000000090000007945), Instr::Hint("BSSY"));
    assert_eq!(da(0x00000000038000000000000000017941), Instr::Hint("BSYNC"));
    assert_eq!(da(0x0000000003800000ffffffff00007948), Instr::Hint("WARPSYNC"));
}

/// Test-only Ampere encoders.
mod am {
    pub const EXIT: u128 = 0x794d;
    pub fn mov_cb(d: u8, bank: u8, off: u16) -> u128 {
        0x7a02 | ((d as u128) << 16) | ((off as u128) << 38) | ((bank as u128) << 54) | (0xf << 72)
    }
    /// `BRA` from instruction `from` to `to`.
    pub fn bra(from: usize, to: usize) -> u128 {
        let off = (to as i64 - from as i64 - 1) * 16;
        0x7947 | ((((off >> 2) as u128) & ((1 << 48) - 1)) << 34) | (7 << 87)
    }
    pub fn pred(w: u128, p: u8, neg: bool) -> u128 {
        (w & !(0xf << 12)) | ((p as u128) << 12) | ((neg as u128) << 15)
    }
}

#[test]
fn ampere_branch_and_integer_program() {
    // R8 = c[0][0], R0 = c[0][4]; P2 = R8 != 0; @!P2 BRA end; R4 = R0 << 2 (IMAD.SHL);
    // BSSY/BSYNC hints around it; R5 = R4 >> 1 (SHF.R.U32.HI) after the join.
    let code = [
        am::mov_cb(8, 0, 0),
        am::mov_cb(0, 0, 4),
        0x0000000003f45270000000ff0800720c, // ISETP.NE.AND P2, PT, R8, RZ, PT
        0x00000000038000000000090000007945, // BSSY B0
        am::pred(am::bra(4, 6), 2, true),
        0x04000000078e00ff0000000400047824, // IMAD.SHL.U32 R4, R0, 0x4, RZ
        0x00000000038000000000000000007941, // BSYNC B0
        (0x100000000001160300000017ff057819 & !(0xffff_ffff << 32) & !(0xff << 64)) | (1 << 32) | (4 << 64), // SHF.R.U32.HI R5, RZ, 0x1, R4
        am::EXIT,
    ];
    let mut bytes = Vec::new();
    for w in code {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    let f = ir::lower(ShaderStage::Fragment, &bytes).unwrap();
    for (c0, c4, r4) in [(1u32, 5u32, 20u32), (0, 5, 0), (7, 0x4000_0001, 4)] {
        let v = run(&f, &|_, off| if off == 0 { c0 } else { c4 });
        assert_eq!(exit_reg(&f, &v, 4), r4, "c0 = {c0}");
        assert_eq!(exit_reg(&f, &v, 5), r4 >> 1);
    }
    let mut h = SphBuilder::pixel().ps_target(0, 0xf).bytes(AMPERE_SPH_SIZE);
    h.extend(bytes);
    validate(&translate(ShaderStage::Fragment, &h).unwrap().wgsl);
}
