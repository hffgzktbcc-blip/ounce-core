#!/usr/bin/env bash
set -euo pipefail

# Orca passes lifecycle JSON on stdin
payload="$(cat || true)"
# Persistent SSH hosts remain running; per-workspace temporary resources
# can be cleaned up here if needed.
exit 0
