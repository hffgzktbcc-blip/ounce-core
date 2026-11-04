#[test]
fn scratch_ci_only_red() {
    assert!(std::env::var("CI").is_err(), "deliberate CI-only failure for gate.sh --ship verification");
}
