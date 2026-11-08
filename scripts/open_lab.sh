#!/usr/bin/env bash
# Open or serve the Ounce-Core Interactive Verification Lab
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
LAB_FILE="${ROOT_DIR}/site/lab.html"

if [[ ! -f "$LAB_FILE" ]]; then
    echo "Error: lab.html not found at $LAB_FILE" >&2
    exit 1
fi

PORT="${1:-8080}"

echo "============================================================"
echo "  🔬 OUNCE-CORE INTERACTIVE VERIFICATION LAB & WORKBENCH"
echo "  Understand why programs fail. Prove when they work."
echo "============================================================"
echo "  Local file: $LAB_FILE"
echo ""

# Attempt to open directly in desktop browser if GUI is available
if command -v xdg-open >/dev/null 2>&1; then
    echo "  Opening in default browser (xdg-open)..."
    xdg-open "$LAB_FILE" >/dev/null 2>&1 || true
elif command -v open >/dev/null 2>&1; then
    echo "  Opening in default browser (macOS open)..."
    open "$LAB_FILE" >/dev/null 2>&1 || true
fi

echo "  Serving lab on http://127.0.0.1:${PORT}/lab.html"
echo "  (Press Ctrl+C to stop)"
echo "------------------------------------------------------------"

cd "${ROOT_DIR}/site"
exec python3 -m http.server "$PORT"
