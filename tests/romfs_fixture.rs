use std::process::Command;

#[test]
fn romfs_fixture_nro_calling_romfs_init_succeeds() {
    let nro_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/romfs/romfs.nro");
    assert!(
        std::path::Path::new(nro_path).exists(),
        "romfs fixture must exist at {nro_path}"
    );

    // 1. In-memory SD card run: NRO calling romfsInit() succeeds
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", nro_path, "--present=headless"])
        .output()
        .expect("failed to execute ounce-core");

    assert!(output.status.success(), "run must exit with code 0: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ounce-romfs result=0x00000EEE"),
        "expected successful RomFS result, stdout: {stdout}"
    );
    assert!(
        stdout.contains("RomFS Bytes Read:"),
        "expected RomFS bytes read in summary, stdout: {stdout}"
    );

    // 2. With --sdmc=<dir>: host folder behind sdmc:/ is never modified (copy-on-write overlay)
    let temp_sdmc = std::env::temp_dir().join(format!("ounce_romfs_sdmc_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_sdmc);
    std::fs::create_dir_all(&temp_sdmc).unwrap();
    std::fs::write(temp_sdmc.join("unmodified.txt"), b"seed text").unwrap();

    let output_sdmc = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "run",
            nro_path,
            "--present=headless",
            &format!("--sdmc={}", temp_sdmc.display()),
        ])
        .output()
        .expect("failed to execute ounce-core with --sdmc");

    assert!(output_sdmc.status.success(), "run with --sdmc must exit with code 0");
    let stdout_sdmc = String::from_utf8_lossy(&output_sdmc.stdout);
    assert!(
        stdout_sdmc.contains("ounce-romfs result=0x00000EEE"),
        "expected successful RomFS result with --sdmc, stdout: {stdout_sdmc}"
    );

    // Verify host directory was untouched
    assert_eq!(std::fs::read(temp_sdmc.join("unmodified.txt")).unwrap(), b"seed text");
    let entries: Vec<_> = std::fs::read_dir(&temp_sdmc)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        entries,
        ["unmodified.txt"],
        "host sdmc directory must never be modified"
    );

    let _ = std::fs::remove_dir_all(&temp_sdmc);
}
