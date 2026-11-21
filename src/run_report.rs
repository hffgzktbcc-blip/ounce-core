//! Machine-readable run report (`ounce.run.v1`) emitted when `--report-json=<path>` is passed.

use std::path::PathBuf;
use std::sync::OnceLock;

static REPORT_JSON_PATH: OnceLock<PathBuf> = OnceLock::new();
static AUDIO_NULL: OnceLock<bool> = OnceLock::new();
static DETERMINISTIC: OnceLock<bool> = OnceLock::new();

pub fn set_report_path(path: PathBuf) {
    let _ = REPORT_JSON_PATH.set(path);
}

pub fn set_audio_null(enabled: bool) {
    let _ = AUDIO_NULL.set(enabled);
}

pub fn is_audio_null() -> bool {
    AUDIO_NULL.get().copied().unwrap_or(false)
}

pub fn set_deterministic(enabled: bool) {
    let _ = DETERMINISTIC.set(enabled);
}

pub fn is_deterministic() -> bool {
    DETERMINISTIC.get().copied().unwrap_or(false)
}

/// Structured stop report matching `ounce.run.v1`.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub tier: &'static str,
    pub silent_fallback: bool,
    pub stop_kind: &'static str,
    pub exit_code: i32,
    pub pc: Option<u64>,
    /// Link register (`x30`) and stack pointer when the program stopped, set exactly when `pc` is (#475).
    pub lr: Option<u64>,
    pub sp: Option<u64>,
    pub opcode: Option<u32>,
    pub fault_addr: Option<u64>,
    /// Resolved ELF symbol (`name` or `name+0xoff`) at `pc` on fault stops (#475).
    pub fault_symbol: Option<String>,
    /// Bounded AArch64 `X29`/`LR` frame-pointer chain (up to 16 frames) on fault/break stops (#475).
    pub backtrace: Option<String>,
    pub svc_id: Option<u32>,
    /// Last requested `sm` service name (or unimplemented HIPC interface) on `unhandled_svc` (exit 14) (#475).
    pub sm_service: Option<String>,
    /// Last dispatched CMIF command ID (or unimplemented HIPC command ID) on `unhandled_svc` (exit 14) (#475).
    pub cmif_cmd_id: Option<u32>,
    pub break_result: Option<u32>,
    pub insns_executed: u64,
    pub frames_presented: u64,
    pub framebuffer_hash: Option<String>,
    /// Stereo frames of audio the guest mixed (audout and audren), counted as they are mixed, so a headless run and a
    /// run with a sound device agree (#478).
    pub audio_frames_mixed: u64,
    /// `fnv1a64:<16 hex digits>` over that audio, or `None` when none was mixed.
    pub audio_hash: Option<String>,
    /// Status the guest passed to `exit`/`exit_group` (low 8 bits). `run-elf` only: homebrew has no exit status.
    pub guest_exit_status: Option<i32>,
    pub message: Option<String>,
}

/// The stable `exit_code` for a `stop_kind`, as listed in `docs/OUNCE_RUN_V1_SCHEMA.md` (checked by a test).
pub fn exit_code_for(stop_kind: &str) -> i32 {
    match stop_kind {
        "clean_exit" | "window_closed" | "frame_limit" => 0,
        "unimplemented_insn" => 10,
        "memory_fault" => 11,
        "svc_break" => 13,
        "unhandled_svc" => 14,
        "cpu_halted" => 15,
        "budget_exhausted" => 16,
        "loader_error" => 17,
        // `cpu_error` and anything unclassified.
        _ => 12,
    }
}

/// The `ounce.run.v1` JSON object for `report`. Every field is flat: consumers such as `ounce-hb` do not
/// parse nested values yet (roadmap Q2 prerequisite).
pub fn to_json(report: &RunReport) -> serde_json::Value {
    serde_json::json!({
        "schema": "ounce.run.v1",
        "engine": "ounce-core",
        "engine_version": env!("CARGO_PKG_VERSION"),
        "tier": report.tier,
        "silent_fallback": report.silent_fallback,
        "stop_kind": report.stop_kind,
        "exit_code": report.exit_code,
        "pc": report.pc,
        "lr": report.lr,
        "sp": report.sp,
        "opcode": report.opcode,
        "fault_addr": report.fault_addr,
        "fault_symbol": report.fault_symbol,
        "backtrace": report.backtrace,
        "svc_id": report.svc_id,
        "sm_service": report.sm_service,
        "cmif_cmd_id": report.cmif_cmd_id,
        "break_result": report.break_result,
        "insns_executed": report.insns_executed,
        "frames_presented": report.frames_presented,
        "framebuffer_hash": report.framebuffer_hash,
        "audio_frames_mixed": report.audio_frames_mixed,
        "audio_hash": report.audio_hash,
        "guest_exit_status": report.guest_exit_status,
        "message": report.message,
    })
}

/// Maximum number of frames walked along the AArch64 `X29`/`LR` frame-pointer chain (#475).
pub const MAX_BACKTRACE_FRAMES: usize = 16;

/// Walks up to `MAX_BACKTRACE_FRAMES` along the AArch64 AAPCS64 frame-pointer chain (`[x29] = prev_fp`,
/// `[x29 + 8] = saved_lr`) starting at `pc`, formatting each frame as `0x<addr>` or `0x<addr> (<symbol>)`.
pub fn format_backtrace<F>(pc: u64, lr: u64, mut fp: u64, mmu: &crate::mmu::Memory, mut resolve: F) -> Option<String>
where
    F: FnMut(u64) -> Option<String>,
{
    let mut frames = Vec::with_capacity(MAX_BACKTRACE_FRAMES);
    let fmt_frame = |addr: u64, sym: Option<String>| match sym {
        Some(s) => format!("{addr:#x} ({s})"),
        None => format!("{addr:#x}"),
    };
    frames.push(fmt_frame(pc, resolve(pc)));

    let mut walked_fp = false;
    while frames.len() < MAX_BACKTRACE_FRAMES && fp != 0 && fp & 7 == 0 {
        let Ok(next_fp) = mmu.read_u64(fp) else { break };
        let Ok(saved_lr) = mmu.read_u64(fp.wrapping_add(8)) else {
            break;
        };
        if saved_lr == 0 {
            break;
        }
        walked_fp = true;
        // Return address points to the instruction after BL/BLR; resolve saved_lr.saturating_sub(4) when non-zero.
        let call_site = saved_lr.saturating_sub(4);
        frames.push(fmt_frame(saved_lr, resolve(call_site).or_else(|| resolve(saved_lr))));
        if next_fp == 0 || next_fp <= fp {
            break;
        }
        fp = next_fp;
    }

    // If the leaf function did not push an AAPCS64 frame record, still include non-zero LR as caller.
    if !walked_fp && lr != 0 && lr != pc && frames.len() < MAX_BACKTRACE_FRAMES {
        let call_site = lr.saturating_sub(4);
        frames.push(fmt_frame(lr, resolve(call_site).or_else(|| resolve(lr))));
    }

    Some(frames.join(" <- "))
}

/// Write `ounce.run.v1` JSON to the `--report-json` path if configured.
pub fn write_if_configured(report: &RunReport) {
    let Some(path) = REPORT_JSON_PATH.get() else {
        return;
    };
    let payload = to_json(report);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&payload)
        && let Err(err) = std::fs::write(path, bytes)
    {
        eprintln!("[Report] Failed to write {}: {err}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA_DOC: &str = include_str!("../docs/OUNCE_RUN_V1_SCHEMA.md");

    fn sample() -> RunReport {
        RunReport {
            tier: "interpreter",
            silent_fallback: false,
            stop_kind: "unimplemented_insn",
            exit_code: 10,
            pc: Some(0x8000_0100),
            lr: Some(0x8000_00F0),
            sp: Some(0x7FFF_F000),
            opcode: Some(0xDEAD_BEEF),
            fault_addr: None,
            fault_symbol: None,
            backtrace: None,
            svc_id: None,
            sm_service: None,
            cmif_cmd_id: None,
            break_result: None,
            insns_executed: 42,
            frames_presented: 3,
            framebuffer_hash: Some("fnv1a64:0123456789abcdef".to_string()),
            audio_frames_mixed: 480,
            audio_hash: Some("fnv1a64:fedcba9876543210".to_string()),
            guest_exit_status: None,
            message: Some("test stop".to_string()),
        }
    }

    /// (first, second) cells of the body rows of the markdown table under `heading`, backticks removed.
    fn table_column(heading: &str) -> Vec<(String, String)> {
        let section = SCHEMA_DOC.split(heading).nth(1).expect("schema heading");
        section
            .lines()
            .skip_while(|l| !l.starts_with('|'))
            .take_while(|l| l.starts_with('|'))
            .skip(2) // header and separator rows
            .filter_map(|l| {
                let cells: Vec<&str> = l.split('|').map(str::trim).collect();
                let name = cells.get(1)?.strip_prefix('`')?.strip_suffix('`')?;
                Some((
                    name.to_string(),
                    cells.get(2).copied().unwrap_or("").trim_matches('`').to_string(),
                ))
            })
            .collect()
    }

    #[test]
    fn serializes_the_report_it_is_given() {
        let json = to_json(&sample());
        assert_eq!(json["schema"], "ounce.run.v1");
        assert_eq!(json["engine"], "ounce-core");
        assert_eq!(json["stop_kind"], "unimplemented_insn");
        assert_eq!(json["exit_code"], 10);
        assert_eq!(json["pc"], 0x8000_0100u64);
        assert_eq!(json["opcode"], 0xDEAD_BEEFu32);
        assert_eq!(json["insns_executed"], 42);
        assert_eq!(json["frames_presented"], 3);
        assert!(json["fault_addr"].is_null() && json["guest_exit_status"].is_null());
    }

    #[test]
    fn json_fields_match_the_schema_document_exactly() {
        let json = to_json(&sample());
        let mut emitted: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        let documented = table_column("## Schema Fields");
        let mut documented: Vec<&str> = documented.iter().map(|(name, _)| name.as_str()).collect();
        emitted.sort_unstable();
        documented.sort_unstable();
        assert_eq!(
            emitted, documented,
            "update docs/OUNCE_RUN_V1_SCHEMA.md and to_json together"
        );
        assert!(
            json.as_object()
                .unwrap()
                .values()
                .all(|v| !v.is_object() && !v.is_array()),
            "fields must stay flat"
        );
    }

    #[test]
    fn exit_codes_match_the_documented_stop_kinds() {
        let rows = table_column("## Canonical `stop_kind` Mapping");
        assert!(rows.len() >= 11, "parsed {rows:?}");
        for (stop_kind, code) in rows {
            assert_eq!(exit_code_for(&stop_kind).to_string(), code, "{stop_kind}");
        }
    }

    #[test]
    fn format_backtrace_walks_fp_chain_and_stops_on_cycles_and_limit() {
        let mut mmu = crate::mmu::Memory::new();
        mmu.map_range(0x10000, 0x2000, crate::mmu::Permission::READ_WRITE);
        // Build a 20-frame chain and verify it caps at MAX_BACKTRACE_FRAMES (16).
        for i in 0..20u64 {
            let fp = 0x10000 + i * 16;
            let next_fp = if i < 19 { fp + 16 } else { 0 };
            mmu.write_u64(fp, next_fp).unwrap();
            mmu.write_u64(fp + 8, 0x40_0100 + i * 4).unwrap();
        }
        let bt = format_backtrace(0x40_0080, 0, 0x10000, &mmu, |a| Some(format!("f_{a:x}"))).unwrap();
        assert_eq!(bt.split(" <- ").count(), MAX_BACKTRACE_FRAMES);
        assert!(bt.starts_with("0x400080 (f_400080) <- 0x400100 (f_4000fc)"));

        // Non-monotonic cycle (next_fp <= fp) terminates cleanly after 1 step.
        mmu.write_u64(0x11000, 0x11000).unwrap();
        mmu.write_u64(0x11008, 0x40_0200).unwrap();
        let cyclic = format_backtrace(0x40_0080, 0, 0x11000, &mmu, |_| None).unwrap();
        assert_eq!(cyclic, "0x400080 <- 0x400200");
    }
}
