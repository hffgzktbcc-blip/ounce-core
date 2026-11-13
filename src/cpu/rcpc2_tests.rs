//! FEAT_LRCPC2 decode and execution regressions, including Issue #414.
use super::*;
use crate::mmu::{Memory, Permission};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x8000;

fn encoding(size: u32, opc: u32, offset: i32, rn: u32, rt: u32) -> u32 {
    0x1900_0000 | (size << 30) | (opc << 22) | (((offset as u32) & 511) << 12) | (rn << 5) | rt
}

#[test]
fn rcpc2_decodes_all_variants_and_offsets() {
    for size in 0..4 {
        for opc in 0..4 {
            for offset in -256..=255 {
                for (rn, rt) in [(8, 0), (31, 31)] {
                    let raw = encoding(size, opc, offset, rn, rt);
                    let expected = if (size == 2 && opc == 3) || (size == 3 && opc >= 2) {
                        Instruction::Unimplemented { raw }
                    } else {
                        Instruction::LoadStoreImm {
                            rt: rt as u8,
                            rn: rn as u8,
                            offset: i64::from(offset),
                            size_bytes: 1 << size,
                            is_load: opc != 0,
                            is_signed: opc >= 2,
                            is_64_target: size == 3 || opc == 2,
                            writeback: false,
                            post_index: false,
                        }
                    };
                    assert_eq!(decoder::decode(raw), expected, "{raw:08x}");
                    for reserved in [1 << 21, 1 << 10, 2 << 10, 3 << 10] {
                        let raw = raw | reserved;
                        assert_eq!(
                            decoder::decode(raw),
                            Instruction::Unimplemented { raw },
                            "{raw:08x}"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(encoding(0, 0, 0xd0, 8, 31), 0x190d011f);
}

fn execute_case(raw: u32, value: u64, tier: u8) {
    let size = raw >> 30;
    let opc = (raw >> 22) & 3;
    let offset = ((((raw >> 12) & 511) as i32) << 23) >> 23;
    let rn = (raw >> 5) & 31;
    let rt = raw & 31;
    let mut cpu = Cpu::new();
    let mut memory = Memory::new();
    memory.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
    memory.map_range(DATA, 0x1000, Permission::READ_WRITE);
    memory.write_u32(CODE, raw).unwrap();
    memory.write_u32(CODE + 4, 0xd4000001).unwrap(); // svc #0
    let base = DATA + 0x800;
    let addr = base.wrapping_add_signed(i64::from(offset));
    let width = 1usize << size;
    let mut initial = vec![0xa5; 0x1000];
    if opc != 0 {
        initial[(addr - DATA) as usize..(addr - DATA) as usize + width]
            .copy_from_slice(&value.to_le_bytes()[..width]);
    }
    memory.write_bytes(DATA, &initial).unwrap();
    cpu.regs.x.fill(0x1234_5678_9abc_def0);
    cpu.regs.sp = base;
    if rn != 31 {
        cpu.regs.x[rn as usize] = base;
    }
    if rt != 31 {
        cpu.regs.x[rt as usize] = value;
    }
    let mut expected_regs = cpu.regs.x;
    if opc == 0 {
        let stored = if rt == 31 { 0 } else { value };
        initial[(addr - DATA) as usize..(addr - DATA) as usize + width]
            .copy_from_slice(&stored.to_le_bytes()[..width]);
    } else if rt != 31 {
        let bits = width * 8;
        let unsigned = value & (u64::MAX >> (64 - bits));
        let loaded = if opc >= 2 {
            ((unsigned << (64 - bits)) as i64 >> (64 - bits)) as u64
        } else {
            unsigned
        };
        expected_regs[rt as usize] = if size == 3 || opc == 2 {
            loaded
        } else {
            loaded as u32 as u64
        };
    }
    let mut tier2 = Tier2JitEngine::new();
    let mut tier3 = Tier3JitEngine::new();
    #[cfg(feature = "native-jit")]
    let mut tier4 = Tier4JitEngine::with_threshold(1);
    // Warm Tier-3 past its fusion threshold; reset the input so every run is checked.
    for _ in 0..=Tier3JitEngine::HOT_THRESHOLD {
        cpu.regs.pc = CODE;
        if rt != 31 {
            cpu.regs.x[rt as usize] = value;
        }
        match tier {
            1 => {
                assert!(matches!(cpu.step(&mut memory), Ok(StepResult::Continue)));
                assert!(matches!(cpu.step(&mut memory), Ok(StepResult::Svc(0))));
            }
            2 => assert_eq!(
                tier2.run_slice(&mut cpu, &mut memory, 8),
                (2, BlockExit::Svc { svc_id: 0 })
            ),
            3 => assert_eq!(
                tier3.run_slice(&mut cpu, &mut memory, 8),
                (2, BlockExit::Svc { svc_id: 0 })
            ),
            #[cfg(feature = "native-jit")]
            4 => assert_eq!(
                tier4.run_slice(&mut cpu, &mut memory, 8),
                (2, BlockExit::Svc { svc_id: 0 })
            ),
            _ => unreachable!(),
        }
        assert_eq!(cpu.regs.x, expected_regs, "tier {tier}, {raw:08x}");
        assert_eq!(cpu.regs.sp, base, "base must not write back");
        let mut actual = vec![0; 0x1000];
        memory.read_bytes(DATA, &mut actual).unwrap();
        assert_eq!(actual, initial, "tier {tier}, {raw:08x}");
    }
    if tier == 3 {
        assert!(
            tier3.total_super_block_executions > 0,
            "Tier-3 must execute a fused block"
        );
    }
    #[cfg(feature = "native-jit")]
    if tier == 4 {
        assert!(tier4.native_insns >= 5, "LRCPC2 must run in native code");
        assert_eq!(tier4.fallback_insns, 0, "unexpected Tier-4 fallback");
    }
}

#[test]
fn rcpc2_executes_all_variants_across_tiers() {
    let tiers = if cfg!(feature = "native-jit") { 4 } else { 3 };
    for tier in 1..=tiers {
        execute_case(0x190d011f, u64::MAX, tier);
        for size in 0..4 {
            for opc in 0..4 {
                if (size == 2 && opc == 3) || (size == 3 && opc >= 2) {
                    continue;
                }
                for offset in [-256, -1, 0, 1, 255] {
                    for rn in [8, 31] {
                        for rt in [0, 31] {
                            for value in [0xfedc_ba98_8000_80ff, 0x1234_5678_0000_7f01] {
                                execute_case(encoding(size, opc, offset, rn, rt), value, tier);
                            }
                        }
                    }
                }
            }
        }
    }
}
