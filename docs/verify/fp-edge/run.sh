#!/usr/bin/env bash
# FP edge-case experiment (issue #359): one fixed corpus, run on an AArch64 reference and replayed through the
# interpreter, Tier-3 and Tier-4. Run from the repo root.
#   docs/verify/fp-edge/run.sh OUTDIR            native AArch64 host (Linux or Apple Silicon): the hardware reference
#   HOST_CC='zig cc -target aarch64-linux-musl' HOST_RUN=qemu-aarch64 docs/verify/fp-edge/run.sh OUTDIR
#                                                cross-built and run under qemu-user: a WEAKER reference, labelled so
# OUTDIR must not exist. Exit status 1 = gate failed (see summarize.py), 2 = infrastructure problem.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="${1:?usage: run.sh OUTDIR}"
[ ! -e "$out" ] || { echo "$out exists; evidence is never overwritten" >&2; exit 2; }
mkdir -p "$out"
bin="${OUNCE_BIN:-target/release/ounce-core}"
host_cc="${HOST_CC:-cc}"; host_run="${HOST_RUN:-}"
if [ -n "$host_run" ]; then kind="qemu-user (weaker reference, not hardware)"; else
  [ "$(uname -m)" = arm64 ] || [ "$(uname -m)" = aarch64 ] || { echo "not an AArch64 host and HOST_RUN is unset" >&2; exit 2; }
  kind="native AArch64 hardware"; fi
$host_cc -O1 -Wall -o "$out/fuzz_host" "$here/../insn-fuzzer/fuzz_host.c"
$host_run "$out/fuzz_host" --list-words > "$out/words.txt"
# gzip -n: no timestamp in the header, so the same trace gives the same bytes
{ $host_run "$out/fuzz_host" --corpus edge 2> "$out/host.stderr" | gzip -n > "$out/trace.gz"; } || true
cases=$(zcat "$out/trace.gz" | wc -l | tr -d ' ')
[ "$cases" -gt 0 ] || { echo "host produced no cases" >&2; cat "$out/host.stderr" >&2; exit 2; }
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
cpu=$( (grep -m1 -E 'model name|Model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- ) || true)
[ -n "$cpu" ] || cpu=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)
python3 - "$out" <<PY
import json, sys, subprocess
sh = lambda *a: subprocess.run(a, capture_output=True, text=True).stdout.strip()
json.dump({"reference_kind": "$kind", "cpu": "$cpu".strip(), "os": sh("uname", "-sr"), "uname_m": sh("uname", "-m"),
           "ounce_core_sha": sh("git", "rev-parse", "HEAD"), "dirty": bool(sh("git", "status", "--porcelain")),
           "rustc": sh("rustc", "--version"), "host_cc": "$host_cc", "host_run": "$host_run", "cases": $cases,
           "words": len(open(sys.argv[1] + "/words.txt").read().splitlines()),
           "trace_sha256": "$(sha "$out/trace.gz")", "host_source_sha256": "$(sha "$here/../insn-fuzzer/fuzz_host.c")",
           "ounce_binary_sha256": "$(sha "$bin")", "host_stderr": open(sys.argv[1] + "/host.stderr").read().strip(),
           "commands": ["$host_cc -O1 -o fuzz_host insn-fuzzer/fuzz_host.c", "fuzz_host --corpus edge | gzip -n > trace.gz",
                        "zcat trace.gz | ounce-core fuzz-replay --engine ENGINE --show 20 --results results-ENGINE.tsv -"]},
          open(sys.argv[1] + "/meta.json", "w"), indent=2)
PY
for e in interp tier3 tier4; do
  zcat "$out/trace.gz" | "$bin" fuzz-replay --engine "$e" --show 20 --results "$out/results-$e.tsv" - > "$out/$e.stdout" 2> "$out/$e.stderr" || true
done
rm -f "$out/fuzz_host"
python3 "$here/summarize.py" "$out"
