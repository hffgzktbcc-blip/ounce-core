# Multiple-controller and touch fixture

Project-authored integration fixture using public libnx APIs. It configures eight player slots,
then checks FullKey, JoyDual, JoyLeft and JoyRight states on players 1–4, including distinct
buttons and left-stick coordinates. It also checks a touch point. The host test supplies input
through `InputFeed`; the guest reports `ounce-input result=0x00000EEE` only if all checks pass.

Build: `make -C tests/fixtures/nro/input_multi all`.
Test: `cargo test --release libnx_multi_input_fixture_reads_styles_and_touch`.

Built 2026-10-28 with devkitA64 r30 (GCC 16.1.0), libnx 4.12.0, switch-tools 1.13.1.
NRO SHA-256: `7810f89006c9f54db3cdb4fbbde1f2476527f4c1a395ee33a82ab08edb6fdda8`.
This fixture does not establish independent homebrew compatibility or physical controller behavior.
Six-axis, vibration, GC input and style-change event signaling remain outside its coverage.
