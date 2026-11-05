use std::process::Command;

#[test]
fn missing_input_is_a_nonzero_cli_failure() {
    let path = std::env::temp_dir().join(format!(
        "ounce_missing_{}_not_created.nro",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to read file"));
}

#[test]
fn malformed_image_is_a_nonzero_cli_failure() {
    let path = std::env::temp_dir().join(format!("ounce_bad_{}.nro", std::process::id()));
    let mut bytes = [0u8; 128];
    bytes[0x10..0x14].copy_from_slice(b"NRO0");
    bytes[0x18..0x1c].copy_from_slice(&128u32.to_le_bytes());
    bytes[0x24..0x28].copy_from_slice(&4096u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[Loader Error]"));
}
