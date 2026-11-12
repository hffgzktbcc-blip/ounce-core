//! Runs the Node tests for the workbench's problem report (assets/lab/report.js). Skipped, with a note, where Node
//! is not installed; CI has it.

use std::process::Command;

#[test]
fn problem_report_fields_url_and_text() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/lab/report.test.js");
    match Command::new("node").arg(script).output() {
        Ok(out) => assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
        Err(_) => eprintln!("node is not installed: skipping the problem report page tests"),
    }
}
