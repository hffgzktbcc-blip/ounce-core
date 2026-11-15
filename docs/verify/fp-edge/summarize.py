#!/usr/bin/env python3
"""Summarise an fp-edge run: per-engine result table, first divergences, engine-vs-interpreter differences.

usage: summarize.py OUTDIR            (OUTDIR holds words.txt, trace.gz, results-<engine>.tsv, meta.json)
Writes OUTDIR/summary.json and OUTDIR/summary.md. Exit status: 0, or 1 when the gate fails (any engine that ran
shows a value mismatch or an FPSR exception-flag mismatch (IOC, DZC, OFC, UFC, IXC, IDC) at any corpus FPCR value, or
no case was compared). Pure standard library.
"""
import collections, gzip, json, pathlib, sys

FPCR_NAMES = {0x0: "default", 0x1000000: "FZ", 0x2000000: "DN", 0x3000000: "FZ+DN", 0x400000: "RMode=RP",
              0x800000: "RMode=RM", 0xc00000: "RMode=RZ", 0x80000: "FZ16", 0x3c80000: "FZ+DN+FZ16+RZ"}
ENGINES = ("interp", "tier3", "tier4")


def load(path):
    rows = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            n, fpcr, word, status, exc, detail = line.rstrip("\n").split("\t", 5)
            rows.append((int(n), int(fpcr, 16), word, status, exc == "1", detail))
    return rows


def trace_line(trace, n):
    with gzip.open(trace, "rt") as f:
        for i, line in enumerate(f, 1):
            if i == n:
                return line.split()
    return None


def operands(trace, row):
    """Hex V0..V3 and X0 of the failing case, enough to reproduce it by hand (layout: tag [fpcr] word nzcv X0..X30 V0..V31)."""
    t = trace_line(trace, row[0])
    if not t:
        return None
    base = 3 + (1 if t[0] == "TF" else 0) + 31
    return {"word": t[1 + (t[0] == "TF")], "fpcr": hex(row[1]), "x0": t[base - 31], **{f"v{i}": t[base + i] for i in range(4)}}


def main(out):
    out = pathlib.Path(out)
    names = {}
    for line in (out / "words.txt").read_text().splitlines():
        w, name = line.split(" ", 1)
        names[w] = name
    meta = json.loads((out / "meta.json").read_text())
    trace = out / "trace.gz"
    data = {e: load(out / f"results-{e}.tsv") for e in ENGINES if (out / f"results-{e}.tsv").exists()}
    summary = {"meta": meta, "engines": {}, "vs_interp": {}}
    gate_ok = bool(data)
    for e, rows in data.items():
        total = collections.Counter(r[3] for r in rows)
        by_mode = collections.defaultdict(collections.Counter)
        for r in rows:
            by_mode[FPCR_NAMES.get(r[1], hex(r[1]))][r[3]] += 1
        first_any = next((r for r in rows if r[3] == "mismatch"), None)
        first_default = next((r for r in rows if r[3] == "mismatch" and r[1] == 0), None)
        unsupported_all = collections.defaultdict(lambda: [0, 0])
        for r in rows:
            u = unsupported_all[names.get(r[2], r[2])]
            u[0] += 1
            u[1] += r[3] == "unsupported"
        flag_rows = [r for r in rows if r[3] == "match" and r[4]]
        fpsr = len(flag_rows)
        flag_words = collections.Counter(names.get(r[2], r[2]) for r in flag_rows)
        first_flag = flag_rows[0] if flag_rows else None
        bad_words = collections.Counter(names.get(r[2], r[2]) for r in rows if r[3] == "mismatch")
        summary["engines"][e] = {
            "cases": len(rows), "status": dict(total), "by_fpcr": {k: dict(v) for k, v in by_mode.items()},
            "fpsr_exception_flags_differ_on_matching_results": fpsr,
            "flag_mismatch_words_top": flag_words.most_common(15),
            "first_flag_divergence": first_flag and {"trace_line": first_flag[0], "name": names.get(first_flag[2]), "fpcr": hex(first_flag[1]),
                                                     "operands": operands(trace, first_flag)},
            "words_always_unsupported": sorted(n for n, (c, u) in unsupported_all.items() if c == u),
            "mismatching_words_top": bad_words.most_common(15),
            "first_divergence": first_any and {"trace_line": first_any[0], "name": names.get(first_any[2]), "fpcr": hex(first_any[1]),
                                               "detail": first_any[5], "operands": operands(trace, first_any)},
            "first_divergence_default_fpcr": first_default and {"trace_line": first_default[0], "name": names.get(first_default[2]),
                                                                  "detail": first_default[5], "operands": operands(trace, first_default)},
        }
        # every FPCR value and the FPSR exception flags are modelled: a mismatch in either fails the gate
        if first_any or fpsr or not rows:
            gate_ok = False
    base = data.get("interp")
    for e in ENGINES[1:]:
        if base and e in data and len(base) == len(data[e]):
            diff = [(a, b) for a, b in zip(base, data[e]) if (a[3], a[5]) != (b[3], b[5])]
            first = diff[0] if diff else None
            summary["vs_interp"][e] = {"differing_cases": len(diff),
                                       "first": first and {"trace_line": first[0][0], "name": names.get(first[0][2]), "fpcr": hex(first[0][1]),
                                                           "interp": [first[0][3], first[0][5]], e: [first[1][3], first[1][5]]}}
    summary["gate"] = {"rule": "no value or FPSR exception-flag mismatch at any FPCR value on any engine that ran, and at least one case compared", "passed": gate_ok}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")

    md = [f"# FP edge-case experiment: {meta.get('reference_kind')}", "",
          f"- reference: **{meta.get('reference_kind')}** on {meta.get('cpu')} ({meta.get('os')})",
          f"- ounce-core `{meta.get('ounce_core_sha')}`, corpus `{meta.get('words')} words / {meta.get('cases')} cases`, trace sha256 `{meta.get('trace_sha256')}`", ""]
    modes = list(FPCR_NAMES.values())
    for e, s in summary["engines"].items():
        md += [f"## {e}", "", "| FPCR | match | mismatch | unsupported |", "|---|---:|---:|---:|"]
        for m in modes:
            c = s["by_fpcr"].get(m, {})
            md.append(f"| {m} | {c.get('match', 0)} | {c.get('mismatch', 0)} | {c.get('unsupported', 0)} |")
        md += ["", f"total: {s['status']}", "", f"FPSR exception flags differ on {s['fpsr_exception_flags_differ_on_matching_results']} otherwise-matching cases"
               + (f" (top words: {s['flag_mismatch_words_top'][:5]})" if s["flag_mismatch_words_top"] else ""), ""]
        for key in ("first_divergence_default_fpcr", "first_divergence"):
            fd = s[key]
            md.append(f"- {key.replace('_', ' ')}: " + (f"`{fd['name']}` trace line {fd['trace_line']}: {fd['detail'][:160]}" if fd else "none"))
        md.append("")
    for e, v in summary["vs_interp"].items():
        md.append(f"- {e} vs interp: {v['differing_cases']} differing cases" + (f", first `{v['first']['name']}` fpcr {v['first']['fpcr']} line {v['first']['trace_line']}" if v["first"] else ""))
    md += ["", f"gate ({summary['gate']['rule']}): **{'PASS' if gate_ok else 'FAIL'}**", ""]
    (out / "summary.md").write_text("\n".join(md))
    print("\n".join(md))
    return 0 if gate_ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
