#!/bin/bash
# Retain binary-bound release notices inside the bundle, then preserve its modes/signatures with ditto.
set -euo pipefail
app="$1"
output="$2"
shift 2
root="$(cd "$(dirname "$0")/.." && pwd)"
runtime="$app/Contents/Helpers/Ounce Runtime.app/Contents/MacOS/ounce-core"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
python3 "$root/scripts/collect_toolchain_notices.py" --binary "$runtime" --target aarch64-apple-darwin --output "$stage/toolchain"
python3 "$root/scripts/package_release.py" --binary "$runtime" --output "$stage/notices.zip" --toolchain-notices "$stage/toolchain" "$@"
# package_release verifies the notice review identity, supplements and toolchain hashes.
unzip -q "$stage/notices.zip" 'licenses/*' -d "$stage"
cp -R "$stage/licenses/." "$app/Contents/Resources/Licenses/"
python3 "$root/scripts/collect_macos_app_notices.py" "$app/Contents/Resources/Licenses/app-dependencies"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
ditto -c -k --sequesterRsrc --keepParent "$app" "$output"
python3 - "$app" "$output" <<'PY'
import sys, zipfile
from pathlib import Path
app, output = map(Path, sys.argv[1:])
with zipfile.ZipFile(output) as archive:
    for path in app.rglob('*'):
        if path.is_file():
            assert archive.read(app.name + '/' + path.relative_to(app).as_posix()) == path.read_bytes(), path
print('App archive contents verified, including retained release notices.')
PY
shasum -a 256 "$output" > "$output.sha256"
