//! Texture instructions: decoding against real `nvdisasm` words (sm_86 for Ampere, sm_61 for
//! Maxwell, from CuAssembler's instruction repositories), lowering, and WGSL that validates.

use super::decode::{decode, DecodeError, Instr, Tex, TexHandle, TexLod, TexOp, RZ};
use super::ir::{Lod, Op};
use super::sph::{PixelImap, SphBuilder};
use super::*;

fn w(lo: u64, hi: u64) -> u128 {
    ((hi as u128) << 64) | lo as u128
}

fn unsupported(r: Result<decode::Decoded, DecodeError>) -> &'static str {
    match r {
        Err(DecodeError::Unsupported { what, .. }) => what,
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

// --- Ampere (sm_86 words) ----------------------------------------------------------------

#[test]
fn cuasm_tex_bound_two_destinations() {
    // @P2 TEX.SCR.LL R6, R8, R8, R10, 0x0, 0x63, 2D (mask 0xf)
    let d = decode(w(0x3000_630a_0808_2b60, 0x0000_0000_019e_0f06)).unwrap();
    assert_eq!((d.pred, d.pred_neg), (2, false));
    assert_eq!(
        d.instr,
        Instr::Tex(Tex {
            op: TexOp::Sample,
            handle: TexHandle::Cbuf { bank: 0, offset: 0x63 * 4 },
            lod: TexLod::Level(10),
            x: 8,
            y: 9,
            dst: [8, 9, 6, 7],
        })
    );
    // @!P1 TEX.SCR.LL RZ, R11, R10, R29, 0x0, 0x64, 2D, 0x1
    let d = decode(w(0x3000_641d_0a0b_9b60, 0x0000_0000_019e_01ff)).unwrap();
    assert_eq!((d.pred, d.pred_neg), (1, true));
    let Instr::Tex(t) = d.instr else { panic!("{d:?}") };
    assert_eq!((t.x, t.y, t.lod, t.dst), (10, 11, TexLod::Level(29), [11, RZ, RZ, RZ]));
    assert_eq!(t.handle, TexHandle::Cbuf { bank: 0, offset: 0x190 });
}

#[test]
fn cuasm_tex_rejects_what_is_not_modelled() {
    // TEX.SCR.LL RZ, R4, R4, R8, 0x0, 0x5e, ARRAY_2D, 0x1
    assert_eq!(unsupported(decode(w(0xb000_5e08_0404_7b60, 0x0000_0000_019e_01ff))), "texture dimension other than 2D");
    // TLD.SCR.LZ R12, R10, R4, 0x0, 0x5e, 1D
    assert_eq!(unsupported(decode(w(0x1000_5eff_040a_7b66, 0x0000_0000_009e_0f0c))), "texture dimension other than 2D");
    // TEX.B.LL.DC R10, R26, R10, R28, 2D (bindless)
    assert_eq!(unsupported(decode(w(0x2800_001c_0a1a_7361, 0x0000_0000_019e_4f0a))), "bindless texture handle");
    // TLD.SCR.B.LZ R12, R10, R4, R25, 1D (bindless)
    assert_eq!(unsupported(decode(w(0x1800_0019_040a_7367, 0x0000_0000_009e_0f0c))), "bindless texture handle");
}

#[test]
fn cuasm_tld_fields_in_two_d_form() {
    // TLD.SCR.LZ R12, R10, R4, 0x0, 0x5e with the dimension switched from 1D to 2D (bit 61).
    let d = decode(w(0x1000_5eff_040a_7b66 | (1 << 61), 0x0000_0000_009e_0f0c)).unwrap();
    assert_eq!(
        d.instr,
        Instr::Tex(Tex {
            op: TexOp::Fetch,
            handle: TexHandle::Cbuf { bank: 0, offset: 0x5e * 4 },
            lod: TexLod::Zero,
            x: 4,
            y: 5,
            dst: [10, 11, 12, 13],
        })
    );
}

// --- Maxwell (sm_61 words; CuAssembler prints the control bits above bit 64) ---------------

fn mx(word: u64) -> Result<decode::Decoded, DecodeError> {
    maxwell::decode(word)
}

#[test]
fn maxwell_cuasm_texs_component_sets() {
    // TEXS.P R28, R2, R34, R38, 0x50, 2D, RGBA
    assert_eq!(
        mx(0xd830_0501_c267_2202).unwrap().instr,
        Instr::Tex(Tex {
            op: TexOp::Sample,
            handle: TexHandle::Bound { index: 0x50 },
            lod: TexLod::Auto,
            x: 34,
            y: 38,
            dst: [2, 3, 28, 29],
        })
    );
    // TEXS.P RZ, R12, R9, R12, 0x52, 2D, RG
    let Instr::Tex(t) = mx(0xd830_052f_f0c7_090c).unwrap().instr else { panic!() };
    assert_eq!((t.x, t.y, t.dst, t.handle), (9, 12, [12, 13, RZ, RZ], TexHandle::Bound { index: 0x52 }));
    // TEXS.P RZ, R3, R4, R5, 0x5a, 2D, R
    let Instr::Tex(t) = mx(0xd820_05af_f057_0403).unwrap().instr else { panic!() };
    assert_eq!(t.dst, [3, RZ, RZ, RZ]);
}

#[test]
fn maxwell_cuasm_tlds_two_d_level_zero() {
    // TLDS.LZ.T R22, R20, R6, R6, 0x50, 2D, RGBA
    assert_eq!(
        mx(0xda50_0501_6067_0614).unwrap().instr,
        Instr::Tex(Tex {
            op: TexOp::Fetch,
            handle: TexHandle::Bound { index: 0x50 },
            lod: TexLod::Zero,
            x: 6,
            y: 6,
            dst: [20, 21, 22, 23],
        })
    );
}

#[test]
fn maxwell_cuasm_rejects_what_is_not_modelled() {
    // TEX.AOFFI.T R12, R4, R42, 0x50, 2D, 0xf
    assert_eq!(unsupported(mx(0xc078_0507_a2a7_040c)), "TEX .DC/.AOFFI");
    // TLD.LZ.AOFFI.T R20, R46, R50, 0x50, 1D, 0xf
    assert_eq!(unsupported(mx(0xdc38_050f_8327_2e14)), "texture dimension other than 2D");
    // TEXS.P RZ, R14, R14, R25, 0x54, CUBE, R
    assert_eq!(unsupported(mx(0xd980_054f_f197_0e0e)), "texture target other than 2D (.LZ for TLDS)");
    // TLDS.LZ.P RZ, R8, R15, 0x5e, 1D, RG
    assert_eq!(unsupported(mx(0xda10_05ef_fff7_0f08)), "texture target other than 2D (.LZ for TLDS)");
    // TEX.B.LL.P R4, R6, R4, 0x0, 2D, 0xf
    assert_eq!(unsupported(mx(0xdeb8_0067_a047_0604)), "bindless texture handle");
    // TLD.B.LZ.T R4, R10, R4, 0x0, 1D, 0xf
    assert_eq!(unsupported(mx(0xdd38_0007_8047_0a04)), "bindless texture handle");
    // TEXS.P RZ, R3, R4, R5, 0x5a, 2D, R with bit 59 cleared (.F16 on SM60+).
    assert_eq!(unsupported(mx(0xd820_05af_f057_0403 & !(1 << 59))), "16-bit texture result");
}

#[test]
fn maxwell_tex_full_form_writes_consecutive_registers() {
    // TEX.AOFFI.T R12, R4, R42, 0x50, 2D, 0xf without .AOFFI and its offset register, mask 0b1010.
    let word = (0xc078_0507_a2a7_040c & !(1 << 54) & !(0xff << 20) & !(0xf << 31)) | ((RZ as u64) << 20) | (0b1010 << 31);
    assert_eq!(
        mx(word).unwrap().instr,
        Instr::Tex(Tex {
            op: TexOp::Sample,
            handle: TexHandle::Bound { index: 0x50 },
            lod: TexLod::Auto,
            x: 4,
            y: 5,
            dst: [RZ, 12, RZ, 13],
        })
    );
}

// --- Lowering and WGSL -------------------------------------------------------------------

mod asm {
    pub fn op(full: u16) -> u128 {
        full as u128 | (7 << 12)
    }
    pub fn exit() -> u128 {
        op(0x94d)
    }
    /// `freq`: 0 PASS, 1 PassMulW.
    pub fn ipa(d: u8, addr: u16, freq: u8, mul: u8) -> u128 {
        op(0x326) | ((d as u128) << 16) | (0xff << 24) | ((mul as u128) << 32) | (((addr >> 2) as u128) << 64) | ((freq as u128) << 78)
    }
    pub fn mufu_rcp(d: u8, s: u8) -> u128 {
        op(0x308) | ((d as u128) << 16) | ((s as u128) << 32) | (4 << 74)
    }
    pub fn mov_imm(d: u8, imm: u32) -> u128 {
        op(0x802) | ((d as u128) << 16) | ((imm as u128) << 32) | (0xf << 72)
    }
    /// 2D `TEX` (or `TLD` when `fetch`) with handle `c[bank][4 * idx]`; `lod`: 0 auto, 1 LZ, 3 LL.
    #[allow(clippy::too_many_arguments)]
    pub fn tex(fetch: bool, d: u8, d2: u8, ra: u8, rb: u8, bank: u8, idx: u16, mask: u8, lod: u8) -> u128 {
        op(if fetch { 0xb66 } else { 0xb60 })
            | ((d as u128) << 16)
            | ((ra as u128) << 24)
            | ((rb as u128) << 32)
            | ((idx as u128) << 40)
            | ((bank as u128) << 54)
            | (1 << 60)
            | (1 << 61)
            | ((d2 as u128) << 64)
            | ((mask as u128) << 72)
            | ((lod as u128) << 87)
    }
}

fn program(header: &SphBuilder, code: &[u128]) -> Vec<u8> {
    let mut b = header.bytes(AMPERE_SPH_SIZE);
    for w in code {
        b.extend_from_slice(&w.to_le_bytes());
    }
    b
}

/// Perspective (u, v) from varying 0, then `TEX` into R0..R3 for target 0.
fn textured_fragment() -> Vec<u8> {
    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..2 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    let code = [
        asm::ipa(8, 0x7c, 0, RZ),
        asm::mufu_rcp(8, 8),
        asm::ipa(4, 0x80, 1, 8),
        asm::ipa(5, 0x84, 1, 8),
        asm::tex(false, 0, 2, 4, RZ, 1, 0, 0xf, 0),
        asm::exit(),
    ];
    program(&h, &code)
}

#[test]
fn textured_fragment_lowers_and_validates() {
    let t = translate(ShaderStage::Fragment, &textured_fragment()).unwrap();
    validate_wgsl(&t.wgsl);
    assert!(t.wgsl.contains("textureSample(t0, s0, vec2<f32>("), "{}", t.wgsl);
    assert_eq!(
        t.interface.textures,
        vec![TextureBinding { group: 1, binding: 32, sampler_binding: Some(33), handle: TexHandle::Cbuf { bank: 1, offset: 0 } }]
    );
}

#[test]
fn texture_results_survive_overlapping_destinations() {
    // TEX R4, R4 (coords in R4, R5; results R4..R7) then copy out: the coordinate reads happen
    // before the writes.
    let h = SphBuilder::pixel().ps_target(0, 0xf).ps_imap(0, 0, PixelImap::Perspective).ps_imap(0, 1, PixelImap::Perspective);
    let code = [asm::ipa(4, 0x80, 0, RZ), asm::ipa(5, 0x84, 0, RZ), asm::tex(false, 4, 6, 4, RZ, 0, 3, 0xf, 1), asm::exit()];
    let f = ir::lower(ShaderStage::Fragment, &program(&h, &code)[AMPERE_SPH_SIZE..]).unwrap();
    let tex = f.values.iter().position(|op| matches!(op, Op::Tex { .. })).unwrap();
    let Op::Tex { x, y, lod, .. } = f.values[tex] else { unreachable!() };
    assert!(matches!(f.op(x), Op::Interp { addr: 0x80, .. }) && matches!(f.op(y), Op::Interp { addr: 0x84, .. }));
    assert_eq!(lod, Lod::Zero);
    let comps: Vec<Op> = (0..4).map(|r| f.op(f.exit_regs[4 + r].unwrap())).collect();
    assert_eq!(comps, (0..4).map(|c| Op::Comp(ir::Value(tex as u32), c)).collect::<Vec<_>>());
}

#[test]
fn vertex_fetch_and_explicit_level_validate() {
    // A vertex shader may not use implicit LOD; TLD with constant texel coordinates and a
    // TEX.LL with a register level both validate.
    let h = SphBuilder::vertex().omap(0x70).omap(0x74).omap(0x78).omap(0x7c);
    let code = [
        asm::mov_imm(0, 3),
        asm::mov_imm(1, 5),
        asm::tex(true, 4, 6, 0, RZ, 0, 1, 0xf, 1),
        asm::mov_imm(2, 0.5f32.to_bits()),
        asm::mov_imm(3, 0.25f32.to_bits()),
        asm::mov_imm(9, 1.0f32.to_bits()),
        asm::tex(false, 10, RZ, 2, 9, 0, 2, 0x3, 3),
        vertex_store(4, 0x70, 2),
        vertex_store(10, 0x78, 2),
        asm::exit(),
    ];
    let t = translate(ShaderStage::Vertex, &program(&h, &code)).unwrap();
    validate_wgsl(&t.wgsl);
    assert!(t.wgsl.contains("textureLoad(t0, vec2<i32>(3i, 5i), 0i)"), "{}", t.wgsl);
    assert!(t.wgsl.contains("textureSampleLevel(t1, s1, vec2<f32>(0.5f, 0.25f), 1.0f)"), "{}", t.wgsl);
    let fetch_only = t.interface.textures[0];
    assert_eq!((fetch_only.binding, fetch_only.sampler_binding), (32, None));
    assert_eq!(t.interface.textures[1].sampler_binding, Some(35));
}

fn vertex_store(s: u8, addr: u16, comps: u8) -> u128 {
    asm::op(0x322) | (0xff << 24) | ((s as u128) << 32) | ((addr as u128) << 40) | (0xff << 64) | (((comps - 1) as u128) << 74)
}

#[test]
fn maxwell_texs_fragment_validates() {
    let mut h = SphBuilder::pixel().ps_target(0, 0xf);
    for c in 0..2 {
        h = h.ps_imap(0, c, PixelImap::Perspective);
    }
    const PT: u64 = 7 << 16;
    let ipa = |d: u8, addr: u16, freq: u64, mul: u8| {
        (0xe000u64 << 48) | PT | (freq << 54) | (7 << 47) | ((RZ as u64) << 39) | ((addr as u64) << 28) | ((mul as u64) << 20) | ((RZ as u64) << 8) | d as u64
    };
    let rcp = (0x5080u64 << 48) | PT | (4 << 20) | (8 << 8) | 8;
    // TEXS R0, R2 <- (R4, R5), handle 2, 2D, RGBA.
    let texs = (0xd8u64 << 56) | (1 << 53) | (4 << 50) | (2 << 36) | (2 << 28) | (5 << 20) | PT | (4 << 8);
    let exit = (0xe300u64 << 48) | PT | 0xf;
    let nop = (0x50b0u64 << 48) | PT | 0xf00;
    let code = [ipa(8, 0x7c, 0, 0), rcp, ipa(4, 0x80, 1, 8), ipa(5, 0x84, 1, 8), texs, exit];
    let mut b = h.bytes(MAXWELL_SPH_SIZE);
    for chunk in code.chunks(3) {
        b.extend_from_slice(&0x001f_8000_fc00_07e0u64.to_le_bytes());
        for i in 0..3 {
            b.extend_from_slice(&chunk.get(i).copied().unwrap_or(nop).to_le_bytes());
        }
    }
    let t = translate_maxwell(ShaderStage::Fragment, &b).unwrap();
    validate_wgsl(&t.wgsl);
    assert_eq!(t.interface.textures[0].handle, TexHandle::Bound { index: 2 });
    assert!(t.listing.iter().any(|l| l.starts_with("TEX.2D {R0, R1, R2, R3}, R4, R5, 0x2")), "{:?}", t.listing);
}

fn validate_wgsl(src: &str) {
    super::tests::validate(src);
}
