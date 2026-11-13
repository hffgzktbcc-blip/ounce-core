"use strict";
// Tests for assets/lab/report.js: the one report behind Report This Problem, Copy details and Help > Report a Problem.
// Run: node tests/lab/report.test.js (tests/lab_report_js.rs runs it with `cargo test` when node is installed).
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const R = require("../../assets/lab/report.js");

const SHA = "a".repeat(64);
const ctx = { config: { default_budget: 2000000000, version: "0.9.1", commit: "abc1234567", host: { os: "macOS", os_version: "15.2", arch: "aarch64", chip: "Apple M2" } }, app: true };
const run = (engine, kind, line, extra = {}) => ({ engine, stop: { kind, line }, files: {}, log_tail: [], ...extra });
const base = { name: "hello.elf", format: "elf", bytes: 123456, sha256: SHA, budget: 2000000000, verdict: { kind: "fails" }, host_reference: null };
const couldNot = { label: "Could not run this program", headline: "The program uses a processor instruction that Ounce Core does not support yet." };

function params(url) { const u = new URL(url); return Object.fromEntries(u.searchParams); }

// 1. A Could-not-run program: every field a maintainer needs arrives filled in.
{
  const d = { ...base, engines: [
    run("tier3", "unimplemented", "run-elf: DIAGNOSIS: fault at pc=0x400120: Unimplemented instruction word 0xD503201F", { log_tail: ["[run-elf] start", "[run-elf] stopped"] }),
    run("tier4", "unimplemented", "run-elf: DIAGNOSIS: fault at pc=0x400120: Unimplemented instruction word 0xD503201F") ] };
  const url = R.url(d, couldNot, ctx), q = params(url);
  assert(url.startsWith(R.ISSUE_URL + "?template=emulation_bug.yml"), "opens the emulation bug form");
  assert.strictEqual(q.engine, "Tier-3 and Tier-4 (both)", "engine is an exact dropdown label");
  assert(Object.values(R.ENGINES).includes(q.engine));
  assert(q.program.includes("hello.elf") && q.program.includes(SHA) && q.program.includes("123,456 bytes") && q.program.includes("step limit 2,000,000,000"));
  assert(q.expected.includes("engine vs engine (Tier-3 against Tier-4)") && q.expected.includes("Expected output SHA-256: none"));
  assert(q.observed.includes("0xD503201F") && q.observed.includes("0x400120"), "the unimplemented word and address");
  assert(q.observed.includes("Last log lines:") && q.observed.includes("[run-elf] stopped"));
  assert.strictEqual(q.repro, "ounce-core run-elf hello.elf --cpu=tier3 --max-insns=2000000000\nounce-core run-elf hello.elf --cpu=tier4 --max-insns=2000000000");
  assert.strictEqual(q.environment, "Ounce Core 0.9.1 (commit abc1234567), macOS 15.2, Apple M2, via the Mac app");
  assert(q.title.includes("Could not run this program") && q.title.includes("hello.elf"));
  // Copy details carries the same fields under the form's headings.
  const text = R.text(d, couldNot, ctx);
  for (const k of ["program", "expected", "observed", "repro", "environment"]) assert(text.includes(q[k]), "copied text includes " + k);
  assert(text.includes("Expected result and where it comes from") && text.includes(q.engine));
}

// 2. Engine mapping.
{
  const e = (...r) => R.engineChoice({ engines: r });
  assert.strictEqual(e(run("tier3", "fault", "x"), run("tier4", "exited", "")), R.ENGINES.tier3, "only the engine that failed");
  assert.strictEqual(e(run("tier3", "exited", ""), run("tier4", "fault", "x")), R.ENGINES.tier4);
  assert.strictEqual(e(run("tier3", "exited", ""), run("tier4", "exited", "")), R.ENGINES.both, "a difference names both");
  assert.strictEqual(e(run("native", "fault", "x"), run("tier3", "exited", "")), R.ENGINES.native);
  assert.strictEqual(e(run("native", "fault", "x"), run("tier3", "fault", "x")), R.ENGINES.unsure);
  assert.strictEqual(e(), R.ENGINES.unsure);
  const template = fs.readFileSync(path.join(__dirname, "../../.github/ISSUE_TEMPLATE/emulation_bug.yml"), "utf8").replace(/\r\n/g, "\n"); // Windows checkouts may use CRLF
  for (const label of Object.values(R.ENGINES)) assert(template.includes("- " + label + "\n"), "template lists the option: " + label);
  for (const id of ["program", "engine", "expected", "observed", "repro", "environment"]) assert(template.includes("id: " + id), "template has field " + id);
}

// 3. Recorded reference and engine-vs-engine fingerprints.
{
  const hr = { status: "differs", harness: "lz4", expected_lines: 12, expected_sha256: "b".repeat(64), first_difference: { line: 3, expected: "ok", actual: "bad" } };
  const d = { ...base, name: "lz4.nro", format: "nro", host_reference: hr, engines: [run("tier3", "exited", "", { files: { "results.txt": { sha256: "c".repeat(64) } } })] };
  const f = R.fields(d, { label: "Results differ", headline: "The output does not match the recorded reference." }, ctx);
  assert(f.expected.includes("recorded reference for lz4 (12 lines, from real hardware)") && f.expected.includes("b".repeat(64)));
  assert(f.observed.includes("Recorded reference differs at line 3. Expected: ok | Got: bad"));
  assert(f.repro.startsWith("ounce-core run-nro lz4.nro --cpu=tier3") && !f.repro.includes("--max-insns"));
  const e = R.fields({ ...base, engines: [run("tier3", "exited", "", { files: { console: { sha256: "d".repeat(64) } } }), run("tier4", "exited", "")], verdict: { diff: { path: "console", line: 2, a_label: "Tier-3", a: "1", b_label: "Tier-4", b: "2" } } }, { label: "Results differ", headline: "x" }, ctx);
  assert(e.expected.includes("d".repeat(64)) && e.expected.includes("console from the Tier-3 run"));
  assert(e.observed.includes("First difference: console line 2 (Tier-3: 1 | Tier-4: 2)"));
}

// 4. Home folders are hidden, in the URL and the copied text.
{
  const d = { ...base, name: "x.elf", engines: [run("tier3", "fault", "cannot load /Users/sam/code/x.elf and /home/lee/y", { log_tail: ["opened C:\\Users\\kim\\z"] })] };
  const url = R.url(d, couldNot, ctx), text = R.text(d, couldNot, ctx);
  for (const t of [decodeURIComponent(url), text]) assert(!/\b(sam|lee|kim)\b/.test(t) && t.includes("~"), "no account names");
}

// 5. Long text is cut so the address fits, and program bytes are never part of it.
{
  const huge = "x".repeat(5000), many = Array.from({ length: 50 }, (_, i) => "line " + i + " " + huge);
  const d = { ...base, name: "n".repeat(120) + ".elf", engines: [run("tier3", "fault", huge, { log_tail: many }), run("tier4", "fault", huge, { log_tail: many })] };
  const url = R.url(d, { label: couldNot.label, headline: huge }, ctx);
  assert(url.length <= R.MAX_URL, "url length " + url.length);
  const q = params(url);
  assert(q.observed.endsWith("…") && q.engine === R.ENGINES.both && q.program.includes(SHA), "cut text, keeps the fixed fields");
  assert(!("bytes" in q) && !url.includes("hexdump"));
}

// 6. Missing server info degrades to honest placeholders, never a crash.
{
  const f = R.fields({ ...base, engines: [run("tier3", "fault", "x")] }, couldNot, { config: {}, platform: "MacIntel" });
  assert.strictEqual(f.environment, "Ounce Core version unknown, MacIntel, via the browser workbench");
}
console.log("report.js checks passed.");
