#!/usr/bin/env python3
"""Replay a docs/verify/insn-fuzzer trace (hardware-recorded) through Unicorn and summarise.

Same comparison and counts as `ounce-core fuzz-replay`, so the two emulators can be read side by side:

  /tmp/fuzz_host --seed 1 --count 100000 [--fpcr-mix] > fz.trace   # Apple Silicon only
  target/release/ounce-core fuzz-replay fz.trace                    # ounce-core
  python3 tools/conformance-adapters/unicorn_fuzz.py fz.trace       # Unicorn

`T`/`TF` lines are words the host executed (`TF` under the FPCR it names); `U` lines are words it rejected.
Unicorn aborts the whole process on some encodings its translator cannot handle, so the cases run in a
worker that the parent restarts after the failing word, which is counted as "emulator aborted".
"""
import collections
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import unicorn_adapter as ua  # noqa: E402
from unicorn import UcError  # noqa: E402

CLASS_MASK = 0xFFE0FC00
FLAGS = 0x9F  # IOC DZC OFC UFC IXC IDC (bits 0-4, 7)
CASE_PREFIXES = ("T ", "U ", "TF")


def undefined_word_runs(word):
    """Whether Unicorn executes a word the host calls UNDEFINED."""
    zeros = " ".join(["0"] * 32) + " " + " ".join(["0" * 32] * 32)
    try:
        ua.run(f"T {word} 0 {zeros} => 0 0")
    except UcError:
        return False
    return True


def case(raw):
    """One trace line to a result record: `kind word fpcr [detail]`."""
    if raw[0] == "U":
        word = raw.split()[1]
        return f"{'undef_runs' if undefined_word_runs(word) else 'undef_ok'} {word} 0"
    fpcr = 0
    if raw.startswith("TF "):  # `TF <fpcr> <word> ...`
        _, fpcr_text, rest = raw.split(" ", 2)
        fpcr, raw = int(fpcr_text, 16), "T " + rest
    word = raw.split()[1]
    try:
        state = ua.run(raw, fpcr)
    except UcError as error:
        # UC_ERR_EXCEPTION: a word Unicorn's QEMU decoder rejects.
        rejected = "EXCEPTION" in str(error) or "INSN_INVALID" in str(error)
        return f"{'unimpl' if rejected else 'error'} {word} {fpcr}"
    _, expected = ua.parse(raw)
    found = ua.differences(state, expected)
    if found:
        return f"diff {word} {fpcr} {found[0].replace(' ', '_')}"
    flags = "flags" if (state["fpsr"] ^ int(expected[1], 16)) & FLAGS else "match"
    return f"{flags} {word} {fpcr}"


def worker(path, start):
    """Print one record per case from `start`, the line number first so a crash is attributable."""
    for index, raw in enumerate(open(path)):
        if index >= start and raw.startswith(CASE_PREFIXES):
            print(index, case(raw), flush=True)


def records(path):
    """Yield every case's record, restarting the worker past any word that aborts it."""
    lines = list(open(path))
    start = 0
    while start < len(lines):
        child = subprocess.Popen([sys.executable, __file__, "--worker", path, str(start)],
                                 stdout=subprocess.PIPE, text=True, stderr=subprocess.DEVNULL)
        last = start - 1
        for out in child.stdout:
            index, _, record = out.partition(" ")
            if not index.isdigit():  # Unicorn prints its own abort message on stdout
                continue
            last = int(index)
            yield record.strip()
        child.wait()
        if child.returncode == 0:
            return
        crashed = next((i for i in range(last + 1, len(lines)) if lines[i].startswith(CASE_PREFIXES)), None)
        if crashed is None:
            return
        word = lines[crashed].split()[2 if lines[crashed].startswith("TF ") else 1]
        # An abort on a word the host rejects is Unicorn failing to run it, not running it.
        yield f"{'undef_ok' if lines[crashed][0] == 'U' else 'abort'} {word} 0"
        start = crashed + 1


def main():
    stat = collections.Counter()
    bad, unimpl, aborted = collections.Counter(), collections.Counter(), collections.Counter()
    by_fpcr = collections.defaultdict(lambda: [0, 0])
    example = {}
    for record in records(sys.argv[1]):
        kind, word, fpcr, *detail = record.split()
        value, cls = int(fpcr), int(word, 16) & CLASS_MASK
        if kind.startswith("undef"):
            stat["host UNDEFINED (SIGILL)"] += 1
            stat["emulator runs a word the host calls UNDEFINED"] += kind == "undef_runs"
            continue
        stat["host executed (not SIGILL)"] += 1
        if kind == "unimpl":
            stat["emulator unimplemented"] += 1
            unimpl[cls] += 1
        elif kind == "abort":
            stat["emulator aborted"] += 1
            aborted[cls] += 1
        elif kind == "error":
            stat["emulator other errors"] += 1
        else:
            stat["emulator executed"] += 1
            by_fpcr[value][0] += 1
            if kind == "diff":
                stat["MISMATCH"] += 1
                by_fpcr[value][1] += 1
                bad[cls] += 1
                example.setdefault(cls, (word, detail[0].replace("_", " ")))
            else:
                stat["exact match (X,V,NZCV,QC)"] += 1
                stat["FPSR exception flags differ on matching results"] += kind == "flags"
    print(f"--- unicorn fuzz summary (Unicorn {ua.unicorn.__version__}, MAX cpu model) ---")
    for label in ("host executed (not SIGILL)", "host UNDEFINED (SIGILL)", "emulator unimplemented",
                  "emulator aborted", "emulator other errors", "emulator executed",
                  "exact match (X,V,NZCV,QC)", "MISMATCH", "FPSR exception flags differ on matching results",
                  "emulator runs a word the host calls UNDEFINED"):
        print(f"{label + ':':<52}{stat[label]}")
    if len(by_fpcr) > 1:
        print("by FPCR (value: executed, mismatches):")
        for value, (done, wrong) in sorted(by_fpcr.items()):
            print(f"  {value:#010x}: {done}, {wrong}")
    for title, table in (("unimplemented", unimpl), ("aborting", aborted)):
        if table:
            print(f"{title} classes: {len(table)} (class = word & 0xFFE0FC00), top by count:")
            for cls, n in table.most_common(12):
                print(f"  {cls:#010x} {n:6d}")
    if bad:
        print("mismatching classes (word & 0xFFE0FC00: count, example):")
        for cls, n in bad.most_common(25):
            word, why = example[cls]
            print(f"  {cls:#010x} {n:6d} {word}  {why}")


if __name__ == "__main__":
    if sys.argv[1] == "--worker":
        worker(sys.argv[2], int(sys.argv[3]))
    else:
        main()
