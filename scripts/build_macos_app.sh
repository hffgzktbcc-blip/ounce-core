#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
  echo "This launcher builds on macOS (Xcode command-line tools and Rust required)." >&2
  exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
app="${1:-$root/dist/Ounce Core.app}"
case "$app" in *.app) ;; *) echo "Output must be an .app bundle path." >&2; exit 1 ;; esac
if [[ -e "$app" ]]; then
  identity=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$app/Contents/Info.plist" 2>/dev/null || true)
  [[ "$identity" == org.ounce-core.launcher ]] || { echo "Refusing to replace an unrelated app: $app" >&2; exit 1; }
fi
stage="$(mktemp -d "${TMPDIR:-/tmp}/ounce-core-app.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
bundle="$stage/Ounce Core.app"
runtime="$bundle/Contents/Helpers/Ounce Runtime.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/Licenses" "$runtime/Contents/MacOS"
cargo build --release --locked --features gpu-present --manifest-path "$root/Cargo.toml"
target=$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cp "$target/release/ounce-core" "$runtime/Contents/MacOS/ounce-core"
cp "$root/tools/macos-launcher/Runtime-Info.plist" "$runtime/Contents/Info.plist"
xcrun swiftc -O -swift-version 5 -target "$(uname -m)-apple-macos13.0" \
  -framework AppKit -framework UniformTypeIdentifiers \
  "$root/tools/macos-launcher/main.swift" -o "$bundle/Contents/MacOS/OunceCore"
cp "$root/tools/macos-launcher/Info.plist" "$bundle/Contents/Info.plist"
cp "$root/LICENSE" "$root/THIRD_PARTY.md" "$root/assets/fonts/DejaVu-LICENSE.txt" "$bundle/Contents/Resources/Licenses/"
codesign --force --sign - "$runtime/Contents/MacOS/ounce-core"
codesign --force --sign - "$runtime"
codesign --force --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
mkdir -p "$(dirname "$app")"
backup="$stage/previous.app"
if [[ -e "$app" ]]; then mv "$app" "$backup"; fi
if ! ditto "$bundle" "$app" || ! codesign --verify --deep --strict "$app"; then
  rm -rf "$app"
  if [[ -e "$backup" ]]; then mv "$backup" "$app"; fi
  echo "Installation failed; previous app restored." >&2
  exit 1
fi
echo "Built $app"
