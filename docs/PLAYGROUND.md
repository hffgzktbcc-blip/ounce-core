# Real ARM64 browser playground

The playground compiles the same safe Rust CPU and memory source used by the
native runtime to WebAssembly. A fresh browser loads the interpreter and steps
actual instructions; JavaScript displays returned register snapshots. It does
not substitute scripted register values. The older `site/lab.html` workbench is
labeled as a design preview and links to this execution path.

## Run locally

From the repository root, install the Rust target and exact binding-tool version:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
scripts/build_playground.sh
python3 -m http.server 8090 --bind 127.0.0.1 --directory target/playground
```

Open `http://127.0.0.1:8090/`. The builder needs Rust and wasm-bindgen; a visitor
needs only a WebAssembly-capable browser. `WASM_BINDGEN=/path/to/wasm-bindgen` can
select an already-installed tool. Generated outputs stay under `target/`.
The Pages workflow builds this same artifact into the existing website's
`playground/` route; publication is separate from building it locally.

The browser crate has its own workspace and lockfile. It reuses `src/cpu/` and
`src/mmu/` directly and includes no desktop, GPU, audio or console service adapter.
The native workspace manifest/lock and native app release approval stay unchanged.

## Try it

- **Move and add:** step MOV then ADD; X0 becomes 1 then 3. SVC stops execution.
- **Vector multiply:** V1 starts at zero and V2 at positive infinity. Step the
  actual vector FMUL and inspect V0's raw bits.
- **Branch loop:** run in batches until the fixed total budget stops execution.
- **Custom words:** load whitespace-separated 32-bit hex instructions. Invalid
  input keeps the prior machine unchanged. Reset restores the original program
  and its starting vector inputs.

Limits: 256 instruction words, 8 KiB input text, a 4 KiB code page and 4 KiB stack,
10,000 total steps, at most 1000 per Run action. This is an interpreter subset;
no OS services, arbitrary binary loader, native JIT, complete FPCR semantics or
complete floating-point exception flags are provided. Chromium is the tested
browser; Firefox/Safari acceptance is not claimed. It is a learning tool, not a
correctness certificate or a security sandbox claim.

## Checks and retained evidence

```sh
CARGO_TARGET_DIR="$PWD/target/learn" cargo test --release --locked \
  --manifest-path crates/ounce-learn/Cargo.toml
python3 tools/playground/check_artifact.py target/playground
# Requires Playwright and its Chromium, or --chromium /path/to/system/chromium.
python3 tools/playground/check_browser.py --output target/NEW-browser-check
```

Local acceptance: 117 browser-crate/CPU/memory tests pass. A clean Chromium context
fetched the actual generated `.wasm` and checked MOV/ADD register and PC changes,
SVC stop, transactional input errors, the hardware vector case, input-preserving
reset, bounded loops and a mobile viewport. Execution after loading also passed
with network access disabled. [Browser observations](verify/playground/acceptance.json)
identify the actual Wasm hash; screenshots are retained alongside them. Hosted
browser qualification and live deployment are recorded separately when available.

The build retains actual license texts from its locked Cargo graph and installed
Rust toolchain. A notice inventory is tied to the Wasm bytes. The artifact checker
checks declared asset/notice hashes and refuses paths escaping the artifact root.
These are correspondence checks, not a legal opinion, a source-provenance guarantee
or clearance of the native Mac app's separate distribution questions. The CI
preview includes notices and checksums; it does not include the binding-tool binary.
