//! Headless HBLauncher UI callbacks for a bounded first-frame run.
#![allow(dead_code)]

use super::hbx::HbxImage;
use crate::{
    cpu::{tier2_jit::BlockExit, tier3_jit::Tier3JitEngine, Cpu, StepResult},
    gpu::Frame,
    mmu::{Memory, Permission},
};
use ab_glyph::{point, Font, FontRef, ScaleFont};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const GATES: u64 = 0x1000;
const CTX: u64 = 0x10_0000;
const STACK: u64 = 0x20_0000;
const HEAP: u64 = 0x40_0000;
const STACK_SIZE: usize = 1024 * 1024;
const HEAP_SIZE: usize = 4 * 1024 * 1024;
const SVC_BASE: u16 = 0x100;

pub(super) fn first_frame(
    image: &HbxImage<'_>,
    tier3: bool,
) -> Result<Frame, Box<dyn std::error::Error>> {
    let mut mem = Memory::new();
    let mut cpu = Cpu::new();
    mem.map_range(CTX, 4096, Permission::READ_WRITE);
    mem.map_range(STACK, STACK_SIZE, Permission::READ_WRITE);
    mem.map_range(HEAP, HEAP_SIZE, Permission::READ_WRITE);
    mem.map_range(GATES, 4096, Permission::READ_WRITE_EXECUTE);
    image.map(&mut mem)?;
    let (renderer, ops) = (CTX + 0x300, CTX + 0x400);
    for (offset, value) in [
        (0, 0x0000_0003_3342_5848),
        (8, 0x2b8),
        (0x40, HEAP),
        (0x48, HEAP_SIZE as u64),
        (0x60, image.code_base),
        (0x70, STACK),
        (0x78, STACK_SIZE as u64),
        (0xb0, renderer),
    ] {
        mem.write_u64(CTX + offset, value)?;
    }
    mem.write_u64(renderer, (24 << 32) | 1)?;
    mem.write_u64(renderer + 8, renderer)?;
    mem.write_u64(renderer + 16, ops)?;
    mem.write_u64(ops, (120 << 32) | 1)?;
    for slot in 0..14 {
        let gate = GATES + slot * 4;
        let svc = u32::from(SVC_BASE) + slot as u32;
        mem.write_u32(gate, 0xd400_0001 | (svc << 5))?;
        mem.write_u64(ops + 8 + slot * 8, gate)?;
    }
    cpu.regs.pc = image.entry;
    cpu.regs.sp = STACK + STACK_SIZE as u64;
    cpu.regs.write_x(0, CTX);
    let mut jit = tier3.then(Tier3JitEngine::new);
    let mut remaining = 200_000;
    let mut frame = Frame::new(WIDTH, HEIGHT);
    let font = FontRef::try_from_slice(include_bytes!("../../assets/fonts/DejaVuSans.ttf"))?;
    let mut drawing = false;
    let mut callbacks = 0;
    while remaining > 0 {
        let pc = cpu.regs.pc;
        let svc = if let Some(jit) = jit.as_mut() {
            let (count, exit) = jit.run_slice(&mut cpu, &mut mem, remaining.min(1024));
            if count == 0 || matches!(exit, BlockExit::Fault { .. }) {
                return Err(format!("HBX Tier-3 fault at {pc:#x}: {exit:?}").into());
            }
            remaining -= count;
            match exit {
                BlockExit::Svc { svc_id } => Some(svc_id),
                _ => None,
            }
        } else {
            remaining -= 1;
            match cpu.step(&mut mem)? {
                StepResult::Continue => None,
                StepResult::Svc(svc) => Some(svc),
                StepResult::Halt(message) => return Err(message.into()),
            }
        };
        let Some(svc) = svc else { continue };
        callbacks += 1;
        if callbacks > 128 {
            return Err("HBX UI callback budget exceeded".into());
        }
        let color = |reg| (cpu.regs.read_x(reg) as u32).to_le_bytes();
        let scalar = |reg| cpu.regs.read_s(reg);
        match svc.checked_sub(SVC_BASE) {
            Some(0) => {
                let rgba = color(1);
                for pixel in frame.rgba.chunks_exact_mut(4) {
                    pixel.copy_from_slice(&rgba);
                }
                drawing = true;
            }
            Some(slot @ (1 | 2)) if drawing => {
                let (x, y, w, h, radius) = (scalar(0), scalar(1), scalar(2), scalar(3), scalar(4));
                if ![x, y, w, h, radius, scalar(5)]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    return Err("HBX rectangle has non-finite coordinates".into());
                }
                let thickness = if slot == 2 { scalar(5).max(0.0) } else { 0.0 };
                let radius = radius.max(0.0).min(w.min(h).max(0.0) / 2.0);
                let rgba = color(1);
                for row in (y.floor().max(0.0) as usize)..((y + h).ceil() as usize).min(HEIGHT) {
                    for col in (x.floor().max(0.0) as usize)..((x + w).ceil() as usize).min(WIDTH) {
                        let dx = (col as f32 + 0.5 - (x + w / 2.0)).abs() - (w / 2.0 - radius);
                        let dy = (row as f32 + 0.5 - (y + h / 2.0)).abs() - (h / 2.0 - radius);
                        let distance =
                            dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
                        if distance <= 0.0 && (slot == 1 || distance >= -thickness) {
                            blend(&mut frame, col, row, rgba, 1.0);
                        }
                    }
                }
            }
            Some(3) if drawing => {
                let addr = cpu.regs.read_x(2);
                let mut text = Vec::new();
                for i in 0..=512 {
                    let byte =
                        mem.read_u8(addr.checked_add(i).ok_or("HBX text address overflow")?)?;
                    if byte == 0 {
                        break;
                    }
                    if i == 512 {
                        return Err("HBX text exceeds 512 bytes".into());
                    }
                    text.push(byte);
                }
                let size = *[88.0, 34.0, 26.0, 20.0, 17.0, 14.0]
                    .get(cpu.regs.read_x(1) as usize)
                    .ok_or("invalid HBX font")?;
                let scaled = font.as_scaled(size);
                let (start, y) = (scalar(0), scalar(1));
                if !start.is_finite() || !y.is_finite() {
                    return Err("HBX text has non-finite coordinates".into());
                }
                let mut x = start;
                let rgba = color(3);
                for ch in std::str::from_utf8(&text)?.chars() {
                    let id = scaled.glyph_id(ch);
                    let glyph = id.with_scale_and_position(size, point(x, y + scaled.ascent()));
                    if let Some(outline) = font.outline_glyph(glyph) {
                        let bounds = outline.px_bounds();
                        outline.draw(|col, row, coverage| {
                            let x = (bounds.min.x as i64).saturating_add(i64::from(col));
                            let y = (bounds.min.y as i64).saturating_add(i64::from(row));
                            if (0..WIDTH as i64).contains(&x) && (0..HEIGHT as i64).contains(&y) {
                                blend(&mut frame, x as usize, y as usize, rgba, coverage);
                            }
                        });
                    }
                    x += scaled.h_advance(id);
                }
                cpu.regs.write_s(0, x - start);
            }
            Some(10) if drawing => return Ok(frame),
            _ => {
                return Err(format!("unsupported HBX callback or call order: SVC {svc:#x}").into())
            }
        }
        cpu.regs.pc = cpu.regs.read_x(30);
    }
    Err("HBX first frame exceeded the instruction budget".into())
}

fn blend(frame: &mut Frame, x: usize, y: usize, rgba: [u8; 4], coverage: f32) {
    let start = (y * WIDTH + x) * 4;
    let dst = &mut frame.rgba[start..start + 4];
    let alpha = f32::from(rgba[3]) / 255.0 * coverage;
    let dst_alpha = f32::from(dst[3]) / 255.0;
    let out_alpha = alpha + dst_alpha * (1.0 - alpha);
    if out_alpha > 0.0 {
        for i in 0..3 {
            dst[i] = ((f32::from(rgba[i]) * alpha + f32::from(dst[i]) * dst_alpha * (1.0 - alpha))
                / out_alpha)
                .round() as u8;
        }
        dst[3] = (out_alpha * 255.0).round() as u8;
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires the pinned outside Hello CPU package"]
fn outside_hello_cpu_first_frame() -> Result<(), Box<dyn std::error::Error>> {
    use std::{fs, path::PathBuf};

    let root = PathBuf::from(std::env::var_os("OUNCE_HBX_PACKAGE").ok_or("set OUNCE_HBX_PACKAGE")?);
    let report = fs::read(root.join("report.json"))?;
    let code = fs::read(root.join("code.bin"))?;
    let data = fs::read(root.join("data.bin"))?;
    let image = HbxImage::parse(&report, &code, &data)?;
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/corpus/switch2/hello-cpu-first-frame.json"
    ))?;
    let interpreter = first_frame(&image, false)?;
    let tier3 = first_frame(&image, true)?;
    assert!(interpreter == tier3, "HBX engines differ on frame 1");
    assert_eq!(Some(tier3.width as u64), oracle["width"].as_u64());
    assert_eq!(Some(tier3.height as u64), oracle["height"].as_u64());
    for sample in oracle["pixels"].as_array().expect("registered pixels") {
        let x = sample["x"].as_u64().expect("x") as usize;
        let y = sample["y"].as_u64().expect("y") as usize;
        let pixel = tier3.pixel(x, y).expect("pixel in frame");
        assert_eq!(serde_json::json!(pixel), sample["rgba"], "pixel ({x}, {y})");
        println!("pixel ({x}, {y}): {pixel:?}");
    }
    Ok(())
}
