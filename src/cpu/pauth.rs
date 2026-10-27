//! Pointer authentication (FEAT_PAuth) and branch target identification (FEAT_BTI) for EL0.
//!
//! Semantics follow the Arm ARM pseudocode for ARMv8.5-A: `AddPAC`, `Auth` and `Strip` without
//! FEAT_PAuth2 or FEAT_FPAC (both are v8.6). A failed authentication does not fault: it returns
//! the pointer with a two-bit error code in its extension bits, so it is non-canonical and the
//! first fetch or access through it faults.
//!
//! The PAC function is the IMPLEMENTATION DEFINED one the architecture allows (FEAT_PACIMP, the
//! `APA`/`API` = 0b0001 case of `ID_AA64ISAR1_EL1`), not QARMA: SipHash-2-4 over the 64-bit
//! pointer and the 64-bit modifier, keyed with the 128-bit key. Software can only see PAC values
//! through signed pointers, and homebrew never depends on their bits, so any keyed function with
//! good diffusion is architecturally valid. SipHash has public test vectors (checked below).
//!
//! Address layout: 39-bit user VAs (TTBR0, `T0SZ` = 25, the range `svcGetInfo` reports) with
//! top-byte-ignore off, because `Memory` does not ignore tag bytes. The PAC field is then bits
//! 63:56 and 54:39 (24 bits), and bit 55 selects the half of the address space.

/// Pointer authentication keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacKey {
    IA,
    IB,
    DA,
    DB,
}

impl PacKey {
    /// `AUT*` error code (`keynumber:NOT(keynumber)`): `01` for the A keys, `10` for the B keys.
    fn error_code(self) -> u64 {
        match self {
            PacKey::IA | PacKey::DA => 0b01,
            PacKey::IB | PacKey::DB => 0b10,
        }
    }
}

/// The 64-bit modifier of a sign or authenticate operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacModifier {
    /// `Xn|SP` (31 reads SP).
    Reg(u8),
    /// The `*Z*` forms and `LDRAA`/`LDRAB`.
    Zero,
}

/// A 128-bit key as the `AP*Key{Hi,Lo}_EL1` register pair holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key128 {
    pub hi: u64,
    pub lo: u64,
}

/// `PACIASP` and `PACIBSP`, which are also landing pads.
pub const PACIASP: u32 = 0xD503_233F;
pub const PACIBSP: u32 = 0xD503_237F;

/// Per-process pointer authentication state: the keys and the `SCTLR_EL1` controls for EL0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerAuth {
    pub ia: Key128,
    pub ib: Key128,
    pub da: Key128,
    pub db: Key128,
    pub ga: Key128,
    /// `SCTLR_EL1.{EnIA, EnIB, EnDA, EnDB}`. A disabled key makes sign and authenticate return
    /// the pointer unchanged (and the HINT forms behave as NOPs).
    pub enabled: [bool; 4],
    /// Virtual address bits (64 - `T0SZ`).
    pub va_bits: u32,
    /// `TCR_EL1.TBI0`.
    pub tbi: bool,
    /// `SCTLR_EL1.BT0`: when set, `PACIASP`/`PACIBSP` do not accept `BTYPE` = 0b11.
    pub bt0: bool,
}

impl Default for PointerAuth {
    /// Keys enabled, as for an ARMv8.3+ user process. Hardware draws keys at random per process;
    /// these are fixed so runs stay reproducible.
    fn default() -> Self {
        Self {
            ia: Key128 { hi: 0x5A1E_7C3B_9D04_E62F, lo: 0x0F1E_2D3C_4B5A_6978 },
            ib: Key128 { hi: 0xC3A5_1F87_2B6D_904E, lo: 0x8796_A5B4_C3D2_E1F0 },
            da: Key128 { hi: 0x1B2C_3D4E_5F60_7182, lo: 0x2468_ACE0_1357_9BDF },
            db: Key128 { hi: 0x9E37_79B9_7F4A_7C15, lo: 0xF39C_C060_5CED_C834 },
            ga: Key128 { hi: 0x6A09_E667_F3BC_C908, lo: 0xBB67_AE85_84CA_A73B },
            enabled: [true; 4],
            va_bits: 39,
            tbi: false,
            bt0: false,
        }
    }
}

impl PointerAuth {
    fn key(&self, key: PacKey) -> (Key128, bool) {
        match key {
            PacKey::IA => (self.ia, self.enabled[0]),
            PacKey::IB => (self.ib, self.enabled[1]),
            PacKey::DA => (self.da, self.enabled[2]),
            PacKey::DB => (self.db, self.enabled[3]),
        }
    }

    /// The bits that hold the PAC: 54:`va_bits`, plus 63:56 without top-byte-ignore.
    fn pac_mask(&self) -> u64 {
        let low = (1u64 << 55) - (1u64 << self.va_bits);
        if self.tbi {
            low
        } else {
            low | 0xFF00_0000_0000_0000
        }
    }

    /// `Strip`: the pointer with its PAC field replaced by copies of bit 55.
    pub fn strip(&self, ptr: u64) -> u64 {
        if (ptr >> 55) & 1 != 0 {
            ptr | self.pac_mask()
        } else {
            ptr & !self.pac_mask()
        }
    }

    /// `AddPAC`.
    pub fn sign(&self, ptr: u64, modifier: u64, key: PacKey) -> u64 {
        let (k, enabled) = self.key(key);
        if !enabled {
            return ptr;
        }
        let mut pac = compute_pac(self.strip(ptr), modifier, k);
        // A non-canonical input gets a PAC that can never authenticate.
        let top = if self.tbi { 55 } else { 64 };
        let width = top - self.va_bits;
        let ext = (ptr >> self.va_bits) & ((1u64 << width) - 1);
        if ext != 0 && ext != (1u64 << width) - 1 {
            pac ^= 1u64 << (top - 2);
        }
        let mask = self.pac_mask();
        (ptr & !mask) | (pac & mask)
    }

    /// `Auth`: the stripped pointer if its PAC matches, else the stripped pointer with the
    /// key's error code in bits 62:61 (53:54 with top-byte-ignore).
    pub fn auth(&self, ptr: u64, modifier: u64, key: PacKey) -> u64 {
        let (k, enabled) = self.key(key);
        if !enabled {
            return ptr;
        }
        let original = self.strip(ptr);
        let mask = self.pac_mask();
        if compute_pac(original, modifier, k) & mask == ptr & mask {
            return original;
        }
        let err = key.error_code();
        if self.tbi {
            (ptr & 0xFF80_0000_0000_0000) | err << 53 | (original & ((1u64 << 53) - 1))
        } else {
            (original & (1u64 << 63)) | err << 61 | (original & ((1u64 << 61) - 1))
        }
    }

    /// `PACGA`: the generic key's PAC of `Xn` with modifier `Xm|SP` in the upper 32 bits.
    pub fn pacga(&self, value: u64, modifier: u64) -> u64 {
        compute_pac(value, modifier, self.ga) & 0xFFFF_FFFF_0000_0000
    }
}

/// The PAC function: SipHash-2-4 of `data || modifier` (little-endian) under `key`.
pub fn compute_pac(data: u64, modifier: u64, key: Key128) -> u64 {
    let mut msg = [0u8; 16];
    msg[..8].copy_from_slice(&data.to_le_bytes());
    msg[8..].copy_from_slice(&modifier.to_le_bytes());
    siphash24(key.lo, key.hi, &msg)
}

/// SipHash-2-4 (Aumasson and Bernstein, 2012). `k0` is key bytes 0..8, little-endian.
pub fn siphash24(k0: u64, k1: u64, msg: &[u8]) -> u64 {
    let mut v = [
        k0 ^ 0x736F_6D65_7073_6575,
        k1 ^ 0x646F_7261_6E64_6F6D,
        k0 ^ 0x6C79_6765_6E65_7261,
        k1 ^ 0x7465_6462_7974_6573,
    ];
    fn round(v: &mut [u64; 4]) {
        v[0] = v[0].wrapping_add(v[1]);
        v[1] = v[1].rotate_left(13) ^ v[0];
        v[0] = v[0].rotate_left(32);
        v[2] = v[2].wrapping_add(v[3]);
        v[3] = v[3].rotate_left(16) ^ v[2];
        v[0] = v[0].wrapping_add(v[3]);
        v[3] = v[3].rotate_left(21) ^ v[0];
        v[2] = v[2].wrapping_add(v[1]);
        v[1] = v[1].rotate_left(17) ^ v[2];
        v[2] = v[2].rotate_left(32);
    }
    let mut compress = |v: &mut [u64; 4], m: u64| {
        v[3] ^= m;
        round(v);
        round(v);
        v[0] ^= m;
    };
    let mut chunks = msg.chunks_exact(8);
    for c in &mut chunks {
        let mut word = [0u8; 8];
        word.copy_from_slice(c);
        compress(&mut v, u64::from_le_bytes(word));
    }
    let mut last = (msg.len() as u64) << 56;
    for (i, b) in chunks.remainder().iter().enumerate() {
        last |= (*b as u64) << (8 * i);
    }
    compress(&mut v, last);
    v[2] ^= 0xFF;
    for _ in 0..4 {
        round(&mut v);
    }
    v[0] ^ v[1] ^ v[2] ^ v[3]
}

/// `PSTATE.BTYPE` after an indirect branch: `BR` sets 0b01 (0b11 when it sits in a guarded page
/// and does not use X16/X17), `BLR` sets 0b10, returns set 0.
///
/// The value only matters when the target page is guarded, so this returns 0 for any other
/// target. EL0 cannot read `BTYPE`, so that is not observable, and unguarded workloads (the
/// harness, every fixture) never carry a non-zero `BTYPE` into a block engine.
pub fn next_btype(is_link: bool, is_ret: bool, rn: u8, source_guarded: bool, target_guarded: bool) -> u8 {
    if is_ret || !target_guarded {
        0
    } else if is_link {
        0b10
    } else if source_guarded && rn != 16 && rn != 17 {
        0b11
    } else {
        0b01
    }
}

/// Whether the instruction `raw` is a valid landing pad for `btype` (`BTypeCompatible`).
pub fn bti_compatible(raw: u32, btype: u8, bt0: bool) -> bool {
    if raw & 0xFFFF_FF3F == 0xD503_241F {
        // BTI {, c | j | jc}
        return match (raw >> 6) & 3 {
            0 => false,
            1 => btype != 0b11,
            2 => btype != 0b10,
            _ => true,
        };
    }
    if raw == PACIASP || raw == PACIBSP {
        return btype != 0b11 || !bt0;
    }
    // BRK and HLT take their own exception first.
    (raw & 0xFFE0_001F) == 0xD420_0000 || (raw & 0xFFE0_001F) == 0xD440_0000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::interpreter::{Cpu, CpuError, StepResult};
    use crate::mmu::{Memory, Permission};

    const CODE: u64 = 0x1000;
    const CODE2: u64 = 0x3000;
    const STACK: u64 = 0x7000;

    fn setup(words: &[u32]) -> (Cpu, Memory) {
        let mut mmu = Memory::new();
        mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(CODE2, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(STACK - 0x1000, 0x1000, Permission::READ_WRITE);
        for (i, w) in words.iter().enumerate() {
            mmu.write_u32(CODE + i as u64 * 4, *w).unwrap();
        }
        let mut cpu = Cpu::new();
        cpu.regs.pc = CODE;
        cpu.regs.sp = STACK;
        (cpu, mmu)
    }

    fn run(cpu: &mut Cpu, mmu: &mut Memory, steps: usize) -> Result<(), CpuError> {
        for _ in 0..steps {
            cpu.step(mmu)?;
        }
        Ok(())
    }

    #[test]
    fn siphash_reference_vectors() {
        // Key 00..0f; messages 00..(n-1). From the SipHash reference implementation.
        let (k0, k1) = (0x0706_0504_0302_0100, 0x0F0E_0D0C_0B0A_0908);
        assert_eq!(siphash24(k0, k1, &[]), 0x726F_DB47_DD0E_0E31);
        let msg: Vec<u8> = (0..15).collect();
        assert_eq!(siphash24(k0, k1, &msg), 0xA129_CA61_49BE_45E5);
    }

    #[test]
    fn sign_then_auth_round_trips_for_every_key() {
        let pa = PointerAuth::default();
        for key in [PacKey::IA, PacKey::IB, PacKey::DA, PacKey::DB] {
            for ptr in [0x0000_0000_8000_1234u64, 0x0000_0040_0000_0000 - 8, 0xFFFF_FF80_0000_1000] {
                let signed = pa.sign(ptr, 0x7000, key);
                assert_ne!(signed, ptr, "{key:?} left {ptr:#x} unsigned");
                assert_eq!(signed & ((1 << 39) - 1), ptr & ((1 << 39) - 1), "address bits changed");
                assert_eq!((signed >> 55) & 1, (ptr >> 55) & 1, "bit 55 changed");
                assert_eq!(pa.strip(signed), ptr);
                assert_eq!(pa.auth(signed, 0x7000, key), ptr);
            }
        }
    }

    #[test]
    fn auth_failure_poisons_with_key_error_code() {
        let pa = PointerAuth::default();
        let ptr = 0x8000_1234u64;
        let a = pa.sign(ptr, 1, PacKey::IA);
        assert_eq!(pa.auth(a, 2, PacKey::IA), ptr | 0b01 << 61, "wrong modifier");
        assert_eq!(pa.auth(a, 1, PacKey::IB), ptr | 0b10 << 61, "wrong key");
        assert_eq!(pa.auth(a ^ (1 << 40), 1, PacKey::IA), ptr | 0b01 << 61, "tampered PAC");
        // An unsigned pointer does not authenticate either.
        assert_eq!(pa.auth(ptr, 1, PacKey::DB), ptr | 0b10 << 61);
    }

    #[test]
    fn non_canonical_input_never_authenticates() {
        let pa = PointerAuth::default();
        let bad = 0x0000_0100_8000_0000u64; // bit 40 set in a low-half pointer
        let signed = pa.sign(bad, 0, PacKey::IA);
        assert_ne!(pa.auth(signed, 0, PacKey::IA), pa.strip(bad));
    }

    #[test]
    fn disabled_key_is_identity_and_tbi_keeps_top_byte() {
        let mut pa = PointerAuth { enabled: [false, true, true, true], ..Default::default() };
        assert_eq!(pa.sign(0x8000_0000, 0, PacKey::IA), 0x8000_0000);
        assert_eq!(pa.auth(0x1234_0000_8000_0000, 0, PacKey::IA), 0x1234_0000_8000_0000);
        pa.tbi = true;
        let tagged = 0x5A00_0000_8000_0000u64;
        let signed = pa.sign(tagged, 9, PacKey::DA);
        assert_eq!(signed >> 56, 0x5A);
        assert_eq!(pa.auth(signed, 9, PacKey::DA), tagged);
        assert_eq!(pa.auth(signed, 8, PacKey::DA), tagged | 0b01 << 53);
    }

    #[test]
    fn pacga_fills_upper_half() {
        let pa = PointerAuth::default();
        let v = pa.pacga(0x1234, 0x5678);
        assert_eq!(v & 0xFFFF_FFFF, 0);
        assert_ne!(v, 0);
        assert_ne!(v, pa.pacga(0x1234, 0x5679));
    }

    #[test]
    fn paciasp_autiasp_retaa_round_trip_in_the_interpreter() {
        // paciasp; autiasp; paciasp; retaa  -> returns to the original LR
        let (mut cpu, mut mmu) = setup(&[0xD503233F, 0xD50323BF, 0xD503233F, 0xD65F0BFF]);
        cpu.regs.write_x(30, CODE2);
        cpu.step(&mut mmu).unwrap();
        let signed = cpu.regs.read_x(30);
        assert_ne!(signed, CODE2);
        assert_eq!(cpu.pauth.auth(signed, STACK, PacKey::IA), CODE2);
        run(&mut cpu, &mut mmu, 3).unwrap();
        assert_eq!(cpu.regs.pc, CODE2);
    }

    #[test]
    fn retab_after_sp_change_faults_on_poisoned_return_address() {
        // pacibsp; sub sp, sp, #16; retab
        let (mut cpu, mut mmu) = setup(&[0xD503237F, 0xD10043FF, 0xD65F0FFF]);
        cpu.regs.write_x(30, CODE2);
        run(&mut cpu, &mut mmu, 3).unwrap();
        assert_eq!(cpu.regs.pc, CODE2 | 0b10 << 61, "IB error code in bits 62:61");
        assert!(matches!(cpu.step(&mut mmu), Err(CpuError::Memory(_))));
    }

    #[test]
    fn register_forms_sign_auth_and_strip() {
        let words = [
            0xDAC10020, // pacia  x0, x1
            0xAA0003E2, // mov    x2, x0
            0xDAC11040, // autia  x0, x2   (wrong modifier: poisons)
            0xDAC10822, // pacda  x2, x1   (x2 already signed with IA)
            0xDAC143E2, // xpaci  x2
            0xDAC12BE3, // pacdza x3
            0xDAC13BE3, // autdza x3
            0xDAC147E4, // xpacd  x4
        ];
        let (mut cpu, mut mmu) = setup(&words);
        let ptr = 0x8000_4000u64;
        cpu.regs.write_x(0, ptr);
        cpu.regs.write_x(1, 0xABCD);
        cpu.regs.write_x(3, ptr);
        let pa = cpu.pauth.clone();
        cpu.regs.write_x(4, pa.sign(ptr, 0, PacKey::DB));
        run(&mut cpu, &mut mmu, words.len()).unwrap();
        assert_eq!(cpu.regs.read_x(0), ptr | 0b01 << 61);
        assert_eq!(cpu.regs.read_x(2), ptr);
        assert_eq!(cpu.regs.read_x(3), ptr);
        assert_eq!(cpu.regs.read_x(4), ptr);
    }

    #[test]
    fn hint_1716_and_z_forms() {
        // pacia1716; autia1716; pacibz; autibz; xpaclri
        let (mut cpu, mut mmu) = setup(&[0xD503211F, 0xD503219F, 0xD503235F, 0xD50323DF, 0xD50320FF]);
        cpu.regs.write_x(17, 0x8000_0000);
        cpu.regs.write_x(16, 42);
        cpu.regs.write_x(30, 0x9000_0000);
        run(&mut cpu, &mut mmu, 2).unwrap();
        assert_eq!(cpu.regs.read_x(17), 0x8000_0000);
        run(&mut cpu, &mut mmu, 2).unwrap();
        assert_eq!(cpu.regs.read_x(30), 0x9000_0000);
        let pa = cpu.pauth.clone();
        cpu.regs.write_x(30, pa.sign(0x9000_0000, 7, PacKey::IB));
        run(&mut cpu, &mut mmu, 1).unwrap();
        assert_eq!(cpu.regs.read_x(30), 0x9000_0000);
    }

    #[test]
    fn braa_blraa_and_ldraa() {
        let pa = PointerAuth::default();
        // blraa x0, x1 -> CODE2: ldraa x3, [x2, #8]!; braaz x4
        let (mut cpu, mut mmu) = setup(&[0xD73F0801]);
        mmu.write_u32(CODE2, 0xF8201C43).unwrap(); // ldraa x3, [x2, #8]!
        mmu.write_u32(CODE2 + 4, 0xD61F089F).unwrap(); // braaz x4
        mmu.write_u64(STACK - 0x100 + 8, 0x1122_3344).unwrap();
        cpu.regs.write_x(0, pa.sign(CODE2, 0x55, PacKey::IA));
        cpu.regs.write_x(1, 0x55);
        cpu.regs.write_x(2, pa.sign(STACK - 0x100, 0, PacKey::DA));
        cpu.regs.write_x(4, pa.sign(CODE + 0x40, 0, PacKey::IA));
        cpu.step(&mut mmu).unwrap();
        assert_eq!(cpu.regs.pc, CODE2);
        assert_eq!(cpu.regs.read_x(30), CODE + 4);
        run(&mut cpu, &mut mmu, 2).unwrap();
        assert_eq!(cpu.regs.read_x(3), 0x1122_3344);
        assert_eq!(cpu.regs.read_x(2), STACK - 0x100 + 8, "writeback stores the authenticated address");
        assert_eq!(cpu.regs.pc, CODE + 0x40);
    }

    #[test]
    fn ldraa_with_bad_pointer_faults() {
        let (mut cpu, mut mmu) = setup(&[0xF8200443]); // ldraa x3, [x2]
        cpu.regs.write_x(2, STACK - 0x100); // never signed
        assert!(matches!(cpu.step(&mut mmu), Err(CpuError::Memory(_))));
    }

    #[test]
    fn decodes_pacga_and_reads_id_registers() {
        let (mut cpu, mut mmu) = setup(&[0x9AC23020, 0xD5380621, 0xD5380422]); // pacga x0, x1, x2; mrs x1, id_aa64isar1_el1; mrs x2, id_aa64pfr1_el1
        cpu.regs.write_x(1, 5);
        cpu.regs.write_x(2, 6);
        run(&mut cpu, &mut mmu, 3).unwrap();
        assert_eq!(cpu.regs.read_x(0), cpu.pauth.pacga(5, 6));
        assert_eq!((cpu.regs.read_x(1) >> 8) & 0xF, 1, "API: IMPLEMENTATION DEFINED algorithm");
        assert_eq!((cpu.regs.read_x(1) >> 28) & 0xF, 1, "GPI");
        assert_eq!(cpu.regs.read_x(2) & 0xF, 1, "BT");
    }

    fn bti_case(landing: u32, branch: u32, rn: u8, guarded: bool) -> Result<StepResult, CpuError> {
        let (mut cpu, mut mmu) = setup(&[branch]);
        mmu.write_u32(CODE2, landing).unwrap();
        if guarded {
            mmu.set_guarded(CODE, 0x1000, true);
            mmu.set_guarded(CODE2, 0x1000, true);
        }
        cpu.regs.write_x(rn, CODE2);
        cpu.step(&mut mmu).unwrap();
        cpu.step(&mut mmu)
    }

    #[test]
    fn bti_landing_pads_in_guarded_pages() {
        const BR_X0: u32 = 0xD61F0000;
        const BR_X16: u32 = 0xD61F0200;
        const BLR_X0: u32 = 0xD63F0000;
        const BTI: u32 = 0xD503241F;
        const BTI_C: u32 = 0xD503245F;
        const BTI_J: u32 = 0xD503249F;
        const BTI_JC: u32 = 0xD50324DF;
        const NOP: u32 = 0xD503201F;
        let ok = |l, b, r| bti_case(l, b, r, true).is_ok();
        let fault = |l, b, r| matches!(bti_case(l, b, r, true), Err(CpuError::BranchTarget { pc: CODE2, .. }));
        assert!(fault(NOP, BR_X0, 0));
        assert!(fault(BTI, BLR_X0, 0));
        assert!(ok(BTI_C, BLR_X0, 0));
        assert!(fault(BTI_C, BR_X0, 0), "BR from a guarded page sets BTYPE 11");
        assert!(ok(BTI_C, BR_X16, 16), "BR X16 sets BTYPE 01");
        assert!(ok(BTI_J, BR_X0, 0));
        assert!(fault(BTI_J, BLR_X0, 0));
        assert!(ok(BTI_JC, BLR_X0, 0) && ok(BTI_JC, BR_X0, 0));
        assert!(ok(PACIASP, BLR_X0, 0) && ok(PACIBSP, BR_X0, 0));
        let brk = bti_case(0xD4200000, BR_X0, 0, true);
        assert!(!matches!(brk, Err(CpuError::BranchTarget { .. })), "BRK is always a landing pad: {brk:?}");
        // Unguarded target: anything goes.
        assert!(bti_case(NOP, BR_X0, 0, false).is_ok());
        // RET never needs a landing pad.
        assert!(ok(NOP, 0xD65F0000, 0));
    }

    #[test]
    fn bt0_rejects_pacixsp_after_br() {
        let (mut cpu, mut mmu) = setup(&[0xD61F0000]);
        mmu.write_u32(CODE2, PACIASP).unwrap();
        mmu.set_guarded(CODE, 0x2000, true);
        mmu.set_guarded(CODE2, 0x1000, true);
        cpu.pauth.bt0 = true;
        cpu.regs.write_x(0, CODE2);
        cpu.step(&mut mmu).unwrap();
        assert!(matches!(cpu.step(&mut mmu), Err(CpuError::BranchTarget { btype: 0b11, .. })));
    }

    #[test]
    fn tier2_blocks_link_blr_and_check_landing_pads() {
        use crate::cpu::tier2_jit::{BlockExit, Tier2Block};
        // blr x0 -> CODE2: nop (no landing pad) in a guarded page
        let (mut cpu, mut mmu) = setup(&[0xD63F0000]);
        mmu.write_u32(CODE2, 0xD503201F).unwrap();
        mmu.set_guarded(CODE2, 0x1000, true);
        cpu.regs.write_x(0, CODE2);
        let (_, exit) = Tier2Block::compile(CODE, &mmu).unwrap().execute(&mut cpu, &mut mmu);
        assert_eq!(exit, BlockExit::Branch { target: CODE2 });
        assert_eq!(cpu.regs.read_x(30), CODE + 4, "BLR links in Tier-2/3");
        let (_, exit) = Tier2Block::compile(CODE2, &mmu).unwrap().execute(&mut cpu, &mut mmu);
        assert_eq!(exit, BlockExit::Fault { pc: CODE2 });
        // Same branch with a landing pad runs on.
        mmu.write_u32(CODE2, 0xD503245F).unwrap(); // bti c
        mmu.write_u32(CODE2 + 4, 0xD65F03C0).unwrap(); // ret
        cpu.regs.pc = CODE;
        Tier2Block::compile(CODE, &mmu).unwrap().execute(&mut cpu, &mut mmu);
        let (_, exit) = Tier2Block::compile(CODE2, &mmu).unwrap().execute(&mut cpu, &mut mmu);
        assert_eq!(exit, BlockExit::Return { target: CODE + 4 });
    }
}
