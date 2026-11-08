#!/usr/bin/env bash
# Resumable agent handoff; see scripts/handoff.py --help.
set -eu
exec python3 -B "$(dirname "$0")/handoff.py" "$@"
