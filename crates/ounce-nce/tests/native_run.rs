//! Runs rewritten AArch64 code on the host CPU: trampoline exits, context round trips, resume
//! stubs, the launch slot and fault recovery. AArch64 macOS / Linux only.
#![cfg(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]

use ounce_nce::patch::{Patcher, SiteKind};
use ounce_nce::{enter, Arena, Exit};

struct Vm { arena: Arena, patcher: Patcher, code: u64, stack_top: u64 }

fn words_to_bytes(w: &[u32]) -> Vec<u8> { w.iter().flat_map(|w| w.to_le_bytes()).collect() }

fn vm(mut program: Vec<u32>) -> Vm {
    let mut arena = Arena::reserve(1 << 32).unwrap();
    let base = arena.base();
    let (ctx, code, stack) = (base + 0x10_0000, base + 0x100_0000, base + 0x200_0000);
    let mut patcher = Patcher::new(ctx, 0x1_0000).unwrap();
    patcher.rewrite(code, &mut program).unwrap();
    arena.commit(ctx, 0x2_0000 + 0x1_0000).unwrap();
    arena.commit(code, 0x1_0000).unwrap();
    arena.commit(stack, 0x1_0000).unwrap();
    arena.write(code, &words_to_bytes(&program)).unwrap();
    arena.write(patcher.code_base(), &words_to_bytes(&patcher.areas()[0].code)).unwrap();
    arena.seal_exec(code, 0x1_0000).unwrap();
    arena.seal_exec(patcher.code_base(), 0x1_0000).unwrap();
    Vm { arena, patcher, code, stack_top: stack + 0x1_0000 }
}

fn launch(vm: &mut Vm, pc: u64) -> Exit {
    let slot = vm.patcher.launch_slot(pc).unwrap();
    let words = Patcher::launch_words(slot, pc).unwrap();
    vm.arena.write(slot, &words_to_bytes(&words)).unwrap();
    enter(&mut vm.arena, vm.patcher.context(), slot).unwrap()
}

#[test]
fn svc_tls_x18_resume_and_fault() {
    let mut vm = vm(vec![
        0xD28000A0, // mov x0, #5
        0xD53BD061, // mrs x1, tpidrro_el0
        0xD28000F2, // mov x18, #7
        0x8B000242, // add x2, x18, x0
        0xD2800550, // mov x16, #42
        0xD2800571, // mov x17, #43
        0x1E601000, // fmov d0, #2.0
        0xD40000E1, // svc #0x7
        0x91000400, // add x0, x0, #1
        0x8B120003, // add x3, x0, x18
        0x8B110063, // add x3, x3, x17
        0xF940001F, // ldr xzr, [x0]  (x0 = 7 after the host edit: faults)
    ]);
    {
        let ctx = vm.arena.context_mut(vm.patcher.context()).unwrap();
        ctx.sp = vm.stack_top;
        ctx.tpidrro = 0x1234_5678;
    }
    let code = vm.code;
    let exit = launch(&mut vm, code);
    let Exit::Trampoline(id) = exit else { panic!("{exit:?}") };
    let site = vm.patcher.sites()[id as usize];
    assert_eq!((site.kind, site.at), (SiteKind::Svc(7), code + 28));
    let resume = vm.patcher.resume_for(site.at + 4).unwrap();
    {
        let ctx = vm.arena.context_mut(vm.patcher.context()).unwrap();
        assert_eq!((ctx.x[0], ctx.x[1], ctx.x[2], ctx.x[18]), (5, 0x1234_5678, 12, 7));
        assert_eq!((ctx.x[16], ctx.x[17], ctx.sp), (42, 43, vm.stack_top));
        assert_eq!(ctx.v[0] as u64, 2.0f64.to_bits());
        ctx.x[0] = 6; // the host's SVC result
        ctx.x[17] = 100;
    }
    let exit = enter(&mut vm.arena, vm.patcher.context(), resume).unwrap();
    let Exit::Fault { pc, addr, .. } = exit else { panic!("{exit:?}") };
    assert_eq!((pc, addr), (code + 44, 7));
    let ctx = vm.arena.context_mut(vm.patcher.context()).unwrap();
    assert_eq!((ctx.x[0], ctx.x[3], ctx.pc), (7, 7 + 7 + 100, code + 44));
}

#[test]
fn fault_keeps_simd_registers() {
    let mut vm = vm(vec![
        0x1E60100E, // fmov d14, #2.0
        0xF940001F, // ldr xzr, [x0]  (x0 = 0: faults)
    ]);
    vm.arena.context_mut(vm.patcher.context()).unwrap().sp = vm.stack_top;
    let code = vm.code;
    let exit = launch(&mut vm, code);
    let Exit::Fault { pc, .. } = exit else { panic!("{exit:?}") };
    assert_eq!(pc, code + 4);
    let ctx = vm.arena.context_mut(vm.patcher.context()).unwrap();
    assert_eq!(ctx.v[14] as u64, 2.0f64.to_bits());
}
