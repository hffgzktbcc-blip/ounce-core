//! FEAT_SHA3: full-vector results and warmed execution through every software tier.
use super::instructions::Sha3Op;
use super::*;
use crate::mmu::{Memory, Permission};

#[test]
fn sha3_decode() {
    assert_eq!(
        decoder::decode(0xce000e10),
        Instruction::Sha3 {
            rd: 16,
            rn: 16,
            rm: 0,
            ra: 3,
            op: Sha3Op::Eor3,
        }
    );
    for (base, op) in [
        (0xce000000, Sha3Op::Eor3),
        (0xce200000, Sha3Op::Bcax),
        (0xce608c00, Sha3Op::Rax1),
    ] {
        for r in 0..32 {
            let raw = base | r | (r << 5) | (r << 16);
            assert!(
                matches!(decoder::decode(raw), Instruction::Sha3 { rd, rn, rm, op: got, .. }
                if rd == r as u8 && rn == rd && rm == rd && got == op)
            );
        }
    }
    for imm in 0..64 {
        assert!(matches!(decoder::decode(0xce800000 | (imm << 10)),
            Instruction::Sha3 { op: Sha3Op::Xar(got), .. } if got == imm as u8));
    }
    for raw in [
        0xce008000,
        0xce208000,
        0xce608800,
        0xce608c00 ^ (1 << 22),
        0xcec00000,
    ] {
        assert_eq!(decoder::decode(raw), Instruction::Unimplemented { raw });
    }
}

fn execute(raw: u32, tier: u8) {
    let mut cpu = Cpu::new();
    let mut memory = Memory::new();
    memory.map_range(0x1000, 0x1000, Permission::READ_WRITE_EXECUTE);
    memory.write_u32(0x1000, raw).unwrap();
    memory.write_u32(0x1004, 0xd4000001).unwrap();
    let mut t2 = Tier2JitEngine::new();
    let mut t3 = Tier3JitEngine::new();
    #[cfg(feature = "native-jit")]
    let mut t4 = Tier4JitEngine::with_threshold(1);
    for iteration in 0..=Tier3JitEngine::HOT_THRESHOLD {
        cpu.regs.pc = 0x1000;
        for (i, v) in cpu.regs.v.iter_mut().enumerate() {
            *v = (0x8001020304050607fedcba9876543210u128.rotate_left(i as u32 * 3))
                ^ u128::from(iteration as u64);
        }
        cpu.regs.fpsr = 0x08000010;
        let mut expected = cpu.regs.v;
        let rd = (raw & 31) as usize;
        let n = expected[((raw >> 5) & 31) as usize];
        let m = expected[((raw >> 16) & 31) as usize];
        let a = expected[((raw >> 10) & 31) as usize];
        expected[rd] = match raw & 0xffe00000 {
            0xce000000 => n ^ m ^ a,
            0xce200000 => n ^ (m & !a),
            _ => {
                let mut bytes = [0u8; 16];
                for (lane, out) in bytes.chunks_exact_mut(8).enumerate() {
                    let n = u64::from_le_bytes(
                        n.to_le_bytes()[lane * 8..lane * 8 + 8].try_into().unwrap(),
                    );
                    let m = u64::from_le_bytes(
                        m.to_le_bytes()[lane * 8..lane * 8 + 8].try_into().unwrap(),
                    );
                    let value = if raw & 0xffe00000 == 0xce600000 {
                        n ^ ((m << 1) | (m >> 63))
                    } else {
                        let imm = (raw >> 10) & 63;
                        let x = n ^ m;
                        if imm == 0 {
                            x
                        } else {
                            (x >> imm) | (x << (64 - imm))
                        }
                    };
                    out.copy_from_slice(&value.to_le_bytes());
                }
                u128::from_le_bytes(bytes)
            }
        };
        match tier {
            1 => {
                assert!(matches!(cpu.step(&mut memory), Ok(StepResult::Continue)));
                assert!(matches!(cpu.step(&mut memory), Ok(StepResult::Svc(0))));
            }
            2 => assert_eq!(
                t2.run_slice(&mut cpu, &mut memory, 8),
                (2, BlockExit::Svc { svc_id: 0 })
            ),
            3 => assert_eq!(
                t3.run_slice(&mut cpu, &mut memory, 8),
                (2, BlockExit::Svc { svc_id: 0 })
            ),
            #[cfg(feature = "native-jit")]
            4 => assert_eq!(
                t4.run_slice(&mut cpu, &mut memory, 8),
                (2, BlockExit::Svc { svc_id: 0 })
            ),
            _ => unreachable!(),
        }
        assert_eq!(cpu.regs.v, expected, "tier {tier}, {raw:08x}");
        assert_eq!(cpu.regs.fpsr, 0x08000010);
        assert_eq!(cpu.regs.pc, 0x1008);
    }
    if tier == 3 {
        assert!(t3.total_super_block_executions > 0);
    }
    // SHA3 deliberately uses Tier-4's verified Tier-3 fallback, rather than native SHA3 host requirements.
    #[cfg(feature = "native-jit")]
    if tier == 4 {
        assert!(t4.fallback_insns > 0);
    }
}

#[test]
fn sha3_all_tiers_and_aliases() {
    for tier in 1..=if cfg!(feature = "native-jit") { 4 } else { 3 } {
        execute(0xce000e10, tier);
        for (rd, rn, rm, ra) in [
            (0, 1, 2, 3),
            (31, 31, 30, 29),
            (2, 1, 2, 3),
            (3, 1, 2, 3),
            (31, 31, 31, 31),
        ] {
            for base in [0xce000000, 0xce200000] {
                execute(base | rd | (rn << 5) | (rm << 16) | (ra << 10), tier);
            }
            execute(0xce608c00 | rd | (rn << 5) | (rm << 16), tier);
            for imm in 0..64 {
                execute(0xce800000 | rd | (rn << 5) | (rm << 16) | (imm << 10), tier);
            }
        }
    }
}
