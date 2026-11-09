#!/usr/bin/env bash
# Build a local, offline-capable interpreter preview; this does not publish it.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
output="$root/target/playground"
bindgen="${WASM_BINDGEN:-wasm-bindgen}"
[[ "$("$bindgen" --version)" == "wasm-bindgen 0.2.129" ]] || { echo 'Use wasm-bindgen-cli 0.2.129 (matching the locked browser crate).' >&2; exit 1; }
CARGO_TARGET_DIR="$root/target/learn" cargo build --release --locked --target wasm32-unknown-unknown --manifest-path "$root/crates/ounce-learn/Cargo.toml"
mkdir -p "$output/pkg"
"$bindgen" --target web --out-dir "$output/pkg" "$root/target/learn/wasm32-unknown-unknown/release/ounce_learn.wasm"
cp "$root/tools/playground/index.html" "$output/index.html"
python3 "$root/scripts/package_playground_notices.py" --output "$output"
printf 'Local preview built: %s\nServe with: python3 -m http.server 8090 --bind 127.0.0.1 --directory %s\n' "$output" "$output"
