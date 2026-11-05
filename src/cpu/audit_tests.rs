//! Architectural expectations independent of agreement between execution engines.
use super::*;
use crate::mmu::{Memory, Permission};

const CODE: u64 = 0x8000_0000;
const DATA: u64 = 0x9000_0000;

fn check(raw: u32, initialize: impl Fn(&mut Cpu), verify: impl Fn(&Cpu)) {
    let mut memory = Memory::new();
    memory.map_range(CODE, 4096, Permission::READ_WRITE_EXECUTE);
    memory.map_range(DATA, 4096, Permission::READ_WRITE);
    memory.write_u8(DATA, 0x80).unwrap();
    memory.write_u16(DATA + 2, 0x8000).unwrap();
    memory.write_u32(CODE, raw).unwrap();
    memory.write_u32(CODE + 4, 0xd400_00e1).unwrap();
    let mut cpu = Cpu::new();
    initialize(&mut cpu);
    cpu.regs.pc = CODE;
    assert!(matches!(cpu.step(&mut memory), Ok(StepResult::Continue)));
    verify(&cpu);

    let mut tier2 = Tier2JitEngine::new();
    let mut tier3 = CpuEngine::new(CpuTier::Tier3);
    #[cfg(feature = "native-jit")]
    let mut tier4 = CpuEngine::Tier4(Box::new(Tier4JitEngine::with_threshold(1)));
    for _ in 0..20 {
        for engine in 0..3 {
            let mut cpu = Cpu::new();
            initialize(&mut cpu);
            cpu.regs.pc = CODE;
            let exit = match engine {
                0 => tier2.run_slice(&mut cpu, &mut memory, 8).1,
                1 => tier3.run_slice(&mut cpu, &mut memory, 8).1,
                _ => {
                    #[cfg(feature = "native-jit")]
                    {
                        tier4.run_slice(&mut cpu, &mut memory, 8).1
                    }
                    #[cfg(not(feature = "native-jit"))]
                    {
                        continue;
                    }
                }
            };
            assert!(matches!(exit, BlockExit::Svc { svc_id: 7 }), "{exit:?}");
            verify(&cpu);
        }
    }
    #[cfg(feature = "native-jit")]
    if !matches!(decoder::decode(raw), Instruction::SimdVec2D { .. }) {
        if let CpuEngine::Tier4(engine) = tier4 {
            assert!(
                engine.native_block_runs > 0,
                "regression must exercise native lowering"
            );
        }
    }
}

#[test]
fn cmp_sp_immediate_uses_sp_when_setting_flags() {
    check(
        0xf100_43ff,
        |c| c.regs.sp = 16,
        |c| {
            assert!(c.regs.pstate.z);
            assert!(c.regs.pstate.c);
            assert!(!c.regs.pstate.n);
        },
    );
}

#[test]
fn signed_byte_load_preserves_destination_width() {
    for raw in [0x39c0_0020, 0x38c0_0020] {
        // LDRSB/LDURSB W0, [X1]
        check(
            raw,
            |c| c.regs.write_x(1, DATA),
            |c| {
                assert_eq!(c.regs.read_x(0), 0x0000_0000_ffff_ff80);
            },
        );
    }
    check(
        0x3980_0020,
        |c| c.regs.write_x(1, DATA),
        |c| {
            assert_eq!(c.regs.read_x(0), 0xffff_ffff_ffff_ff80);
        },
    );
    check(
        0x79c0_0420,
        |c| c.regs.write_x(1, DATA),
        |c| {
            assert_eq!(c.regs.read_x(0), 0x0000_0000_ffff_8000);
        },
    );
}

#[test]
fn vector_double_add_uses_two_double_lanes() {
    check(
        0x4e62_d420,
        |c| {
            c.regs
                .write_q(1, (1.0f64.to_bits() as u128) * ((1u128 << 64) + 1));
            c.regs
                .write_q(2, (2.0f64.to_bits() as u128) * ((1u128 << 64) + 1));
        },
        |c| {
            assert_eq!(
                c.regs.read_q(0),
                (3.0f64.to_bits() as u128) * ((1u128 << 64) + 1)
            );
        },
    );
}

#[test]
fn ordinary_fp_max_propagates_nan() {
    check(
        0x1e62_4820,
        |c| {
            c.regs.write_d(1, f64::NAN);
            c.regs.write_d(2, 1.0);
        },
        |c| assert!(c.regs.read_d(0).is_nan()),
    );
}

#[test]
fn ordinary_fp_min_max_preserve_signed_zero_rules() {
    for (raw, a, b, expected) in [
        (0x1e62_4820, -0.0f64, -0.0f64, -0.0f64),
        (0x1e62_4820, -0.0, 0.0, 0.0),
        (0x1e62_5820, 0.0, 0.0, 0.0),
        (0x1e62_5820, -0.0, 0.0, -0.0),
    ] {
        check(
            raw,
            |c| {
                c.regs.write_d(1, a);
                c.regs.write_d(2, b);
            },
            |c| assert_eq!(c.regs.read_d(0).to_bits(), expected.to_bits()),
        );
    }
}

#[test]
fn scalar_negative_multiply_from_ci_binary() {
    check(
        0x1e3c_89fc,
        |c| {
            c.regs.write_s(15, 2.0);
            c.regs.write_s(28, 3.0);
        },
        |c| assert_eq!(c.regs.read_s(28), -6.0),
    );
}
