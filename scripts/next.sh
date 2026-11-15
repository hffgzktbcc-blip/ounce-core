#!/usr/bin/env bash
# Optional local workflow helper; see --help.
set -eu
exec python3 -B "$(dirname "$0")/agent_workflow.py" next "$@"
