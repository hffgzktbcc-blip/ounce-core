#!/usr/bin/env bash
# Has this already been done? Searches merged history and shipped-work records (local data only).
set -eu
exec python3 -B "$(dirname "$0")/agent_workflow.py" done "$@"
