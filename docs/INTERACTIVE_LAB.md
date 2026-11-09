# Interactive Verification Lab (`site/lab.html`)

An accessible, zero-install web workbench designed for library maintainers, researchers, and students to explore ARM64 execution, debug divergences, and inspect binary formats without assembly or compiler expertise.

The lab is served locally via `scripts/open_lab.sh` (or `cargo run --release -- ui`) and published online at [`https://hffgzktbcc-blip.github.io/ounce-core/lab.html`](https://hffgzktbcc-blip.github.io/ounce-core/lab.html).

![Interactive Lab Screenshot](lab-screenshot.png)

---

## Design Principles: "Craft Anything" for Systems Verification

The interface translates low-level CPU verification concepts into an intuitive, guided workflow:

1. **Dual Modes (Simple ⟷ Deep Lab)**:
   - **Simple View**: Clean, high-level verification dashboard with green pass/fail badges, plain-English explanations of discrepancies, and clear next steps.
   - **Deep Lab**: Full diagnostic ledger with raw hex opcodes, register state deltas (X0–X30, V0–V31, PC, PSTATE, FPSR), disassembly diffs, and execution traces.
2. **Arbitrary File Intake**:
   - Client-side drag-and-drop file inspection powered by browser-native Web Crypto (`crypto.subtle.digest('SHA-256')`).
   - Pure client-side binary header parsing:
     - **ELF64 (AArch64)**: e_ident, machine 0xB7, entry point, section counts.
     - **ELF (x86_64 / other)**: Machine detection and architecture validation.
     - **Homebrew NRO0**: Magic validation, header size, text/ro/data layout.
     - **HFS0 / XCI / RomFS**: Container magic verification and partition extraction hints.
3. **The Three Pillars in Action**:
   - **Ounce-Verify**: 3-engine verification matrix comparing Reference Interpreter, Tier-3 JIT, and Native Hardware execution against pinned upstream baselines (SQLite, Lua, LZ4, xxHash, wasm3).
   - **Ounce-Debug**: Interactive root-cause divergence locator recreating the historical bugs found by Ounce-Core (e.g. the `PRFM` load/prefetch register overwrite, and the `FMLA` fused multiply-add sign error).
   - **Ounce-Learn**: Step-by-step CPU execution demonstrating register changes, flags (NZCV + QC), and memory loads. Links directly into the [WebAssembly Playground](PLAYGROUND.md) (`crates/ounce-learn`) for live WebAssembly instruction execution.
4. **Human-in-the-Loop Guardrails**:
   - Explicit permission gates before simulating external runs.
   - Responsible research notices: explicitly enforces no retail keys, no proprietary firmware, and honest maturity labels (`research-only`, `bounded-verified`, `independently reproduced`, `publicly supported`).

---

## How to Run & Inspect

### Local Execution
```bash
# Launch via helper script (starts local server and opens default browser)
./scripts/open_lab.sh

# Or via cargo CLI
cargo run --release -- ui     # or: cargo run --release -- lab

# Or direct Python server
python3 -m http.server 8080 --directory site
# Then open: http://localhost:8080/lab.html
```

### Headless Verification & Testing
The lab HTML is validated by browser automation and unit tests:
```bash
# Validate offline artifact packaging
scripts/gate.sh --check quick
```

---

## Relationship with Other Frontends

| Frontend | Scope | Location | Tech Stack |
| :--- | :--- | :--- | :--- |
| **Interactive Lab** | Visual workbench, format analyzer, divergence explorer | [`site/lab.html`](../site/lab.html) | HTML5, Tailwind CSS, Web Crypto, JS |
| **WASM Playground** | Real in-browser AArch64 instruction stepping | [`crates/ounce-learn`](../crates/ounce-learn/) | Rust compiled to wasm32, wasm-bindgen |
| **Native Mac App** | Native Apple Silicon desktop runner | [`tools/macos-launcher`](../tools/macos-launcher/) | Swift, AppKit, macOS child-process runtime |
| **CLI / `--gui`** | Direct terminal verification and headless testing | [`src/main.rs`](../src/main.rs) | Safe Rust (`forbid(unsafe_code)`) |
