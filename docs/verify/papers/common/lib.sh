# Shared by paper checks: `source` this, then call `paper_check`.
# paper_check <check dir> <out dir> <ounce-core binary> <source.c> [native=yes|no] [extra clang flags...]
#   Builds <source.c> as a static aarch64 ELF, runs it with `run-elf --cpu=tier3`, requires exit 0 and stdout equal to
#   expected.txt (byte for byte), then (native=yes, the default) builds it for the host with -DHOST and requires the same bytes.
# Prints one `REPRODUCED:` line and returns 0, or prints `NOT REPRODUCED:` and returns 1.
paper_check() {
  local here="$1" out="$2" bin="$3" src="$4" native="${5:-yes}"; shift 5 2>/dev/null || shift $#
  local common; common="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  [ ! -e "$out" ] || { echo "$out already exists" >&2; return 1; }
  mkdir -p "$out"
  local LD
  if command -v ld.lld >/dev/null 2>&1; then LD=(ld.lld); else
    local sysroot host; sysroot="$(rustc --print sysroot)"; host="$(rustc -vV | sed -n 's/^host: //p')"
    LD=("$sysroot/lib/rustlib/$host/bin/rust-lld" -flavor gnu)
    export DYLD_FALLBACK_LIBRARY_PATH="$sysroot/lib" LD_LIBRARY_PATH="$sysroot/lib"
  fi
  clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-outline-atomics \
    -ffp-contract=off -I"$common" "$@" -c "$here/$src" -o "$out/prog.o" || { echo "NOT REPRODUCED: build failed" >&2; return 1; }
  "${LD[@]}" -static -nostdlib -e _start "$out/prog.o" -o "$out/prog.elf" || { echo "NOT REPRODUCED: link failed" >&2; return 1; }
  local status=0
  "$bin" run-elf "$out/prog.elf" --cpu=tier3 --report-json="$out/report.json" > "$out/stdout.txt" 2> "$out/stderr.txt" || status=$?
  if [ "$status" -ne 0 ]; then echo "NOT REPRODUCED: program exited $status" >&2; cat "$out/stdout.txt" >&2; return 1; fi
  if ! diff -u "$here/expected.txt" "$out/stdout.txt" > "$out/diff.txt"; then
    echo "NOT REPRODUCED: output differs from expected.txt" >&2; cat "$out/diff.txt" >&2; return 1
  fi
  local nat="not compared (native=no)"
  if [ "$native" = yes ]; then
    nat="not compared (no host compiler)"
    if clang -O1 -ffp-contract=off -DHOST -I"$common" "$@" "$here/$src" -o "$out/prog_host" -lm 2>/dev/null; then
      "$out/prog_host" > "$out/host_stdout.txt" || true
      cmp -s "$out/host_stdout.txt" "$out/stdout.txt" || { echo "NOT REPRODUCED: native output differs from ounce-core's" >&2; return 1; }
      nat="identical to a native $(uname -m) build"
    fi
  fi
  echo "REPRODUCED: $(basename "$here"), Tier-3, output $nat (reports in $out)"
}
