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

#[allow(dead_code)]
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
    pub opcode: Option<u32>,
    pub fault_addr: Option<u64>,
    pub svc_id: Option<u32>,
    pub break_result: Option<u32>,
    pub insns_executed: u64,
    pub message: Option<String>,
}

/// Write `ounce.run.v1` JSON to the `--report-json` path if configured.
pub fn write_if_configured(report: &RunReport) {
    let Some(path) = REPORT_JSON_PATH.get() else {
        return;
    };
    let payload = serde_json::json!({
        "schema": "ounce.run.v1",
        "engine": "ounce-core",
        "engine_version": env!("CARGO_PKG_VERSION"),
        "tier": report.tier,
        "silent_fallback": report.silent_fallback,
        "stop_kind": report.stop_kind,
        "exit_code": report.exit_code,
        "pc": report.pc,
        "opcode": report.opcode,
        "fault_addr": report.fault_addr,
        "svc_id": report.svc_id,
        "break_result": report.break_result,
        "insns_executed": report.insns_executed,
        "message": report.message,
    });
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
        if let Err(err) = std::fs::write(path, bytes) {
            eprintln!("[Report] Failed to write {}: {err}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_valid_ounce_run_v1_payload() {
        let report = RunReport {
            tier: "interpreter",
            silent_fallback: false,
            stop_kind: "unimplemented_insn",
            exit_code: 10,
            pc: Some(0x8000_0100),
            opcode: Some(0xDEAD_BEEF),
            fault_addr: None,
            svc_id: None,
            break_result: None,
            insns_executed: 42,
            message: Some("test stop".to_string()),
        };
        let payload = serde_json::json!({
            "schema": "ounce.run.v1",
            "engine": "ounce-core",
            "engine_version": env!("CARGO_PKG_VERSION"),
            "tier": report.tier,
            "silent_fallback": report.silent_fallback,
            "stop_kind": report.stop_kind,
            "exit_code": report.exit_code,
            "pc": report.pc,
            "opcode": report.opcode,
            "fault_addr": report.fault_addr,
            "svc_id": report.svc_id,
            "break_result": report.break_result,
            "insns_executed": report.insns_executed,
            "message": report.message,
        });
        assert_eq!(payload["schema"], "ounce.run.v1");
        assert_eq!(payload["stop_kind"], "unimplemented_insn");
        assert_eq!(payload["exit_code"], 10);
        assert_eq!(payload["insns_executed"], 42);
    }
}
