"use strict";
// The problem report for a finished check: one set of fields that fills the GitHub issue form
// (.github/ISSUE_TEMPLATE/emulation_bug.yml), the "Copy details" text and Help > Report a Problem in the Mac app.
// Pure functions with no DOM access, so tests/lab_report.test.js runs them under Node. Nothing here sends anything:
// opening the form only shows it; the person reads it and presses Submit.
(function (root) {
  const ISSUE_URL = "https://github.com/hffgzktbcc-blip/ounce-core/issues/new";
  // GitHub refuses very long addresses (about 8 KB), so the whole URL stays under this and long text is cut.
  const MAX_URL = 6500;
  const LIMITS = { program: 700, expected: 900, observed: 1800, repro: 400, environment: 300 };
  // Exactly the option labels of the Engine dropdown in the issue form: a dropdown only preselects an exact label.
  const ENGINES = {
    tier3: "Tier-3 interpreter (default)",
    tier4: "Tier-4 recompiler (--cpu=tier4)",
    both: "Tier-3 and Tier-4 (both)",
    native: "Native (--cpu=native)",
    unsure: "Not sure",
  };
  const FAULTS = ["unhandled-svc", "unimplemented", "fault", "break", "halt", "loader"];
  const SHORT = { tier3: "Tier-3", tier4: "Tier-4", native: "this computer" };

  // Home folders never leave the machine in a report: /Users/sam/x and /home/sam/x become ~/x.
  const scrub = text => String(text == null ? "" : text)
    .replace(/(?:\/Users|\/home)\/[^/\s"'\\]+/g, "~").replace(/[A-Za-z]:\\Users\\[^\\\s"']+/g, "~");
  const clip = (text, max) => text.length <= max ? text : text.slice(0, Math.max(0, max - 1)).replace(/[\ud800-\udbff]$/, "") + "…";
  const num = n => Number(n).toLocaleString("en-US");
  const faulted = e => FAULTS.includes(e.stop.kind);

  // The Engine dropdown choice: the engine(s) that stopped the run, or all of them when none failed (a difference).
  function engineChoice(d) {
    const ran = d.engines || [];
    const set = new Set((ran.some(faulted) ? ran.filter(faulted) : ran).map(e => e.engine));
    if (set.has("native") && set.size === 1) return ENGINES.native;
    const tiers = ["tier3", "tier4"].filter(t => set.has(t));
    if (set.has("native") && tiers.length) return ENGINES.unsure;
    if (tiers.length === 2) return ENGINES.both;
    return tiers.length ? ENGINES[tiers[0]] : ENGINES.unsure;
  }

  function firstFile(e) {
    const names = Object.keys((e && e.files) || {});
    return names.length ? { name: names[0], sha256: e.files[names[0]].sha256 } : null;
  }

  // What the run was compared with, in a sentence a maintainer can act on.
  function comparison(d) {
    const hr = d.host_reference, ran = d.engines || [];
    if (hr && hr.harness && (hr.status === "matches" || hr.status === "differs"))
      return "the recorded reference for " + hr.harness + " (" + hr.expected_lines + " lines, " + (d.format === "elf" ? "from a native build of the same program" : "from real hardware") + ")";
    if (ran.some(e => e.engine === "native")) return "engine vs this computer (the program run directly, as the reference)";
    const tiers = ran.filter(e => e.engine !== "native");
    if (tiers.length > 1) return "engine vs engine (" + tiers.map(e => SHORT[e.engine]).join(" against ") + ")";
    return "nothing (one engine only), so the only expectation is that the program runs to the end";
  }

  function fingerprint(d) {
    const hr = d.host_reference;
    if (hr && hr.expected_sha256) return "Expected output SHA-256: " + hr.expected_sha256 + " (recorded reference, " + hr.expected_lines + " lines)";
    const ref = (d.engines || []).find(e => e.engine === "native" && e.stop.kind === "exited") || (d.engines || []).find(e => e.stop.kind === "exited");
    const file = firstFile(ref);
    if (file) return "Expected output SHA-256: " + file.sha256 + " (" + file.name + " from the " + SHORT[ref.engine] + " run that finished)";
    return "Expected output SHA-256: none, because no engine finished and wrote output to compare";
  }

  function environment(d, ctx) {
    const c = ctx.config || {}, h = c.host || {};
    const os = [h.os_version ? (h.os || "") + " " + h.os_version : h.os, h.chip || h.arch].filter(Boolean).join(", ");
    const via = ctx.app ? "the Mac app" : "the browser workbench";
    return scrub("Ounce Core " + (c.version || "version unknown") + (c.commit ? " (commit " + c.commit + ")" : "") + ", " + (os || ctx.platform || "system unknown") + ", via " + via);
  }

  function reproLines(d) {
    const ran = d.engines || [], bad = ran.filter(faulted);
    const engines = [...new Set((bad.length ? bad : ran).map(e => e.engine))];
    const cmd = d.format === "elf" ? "run-elf" : "run-nro";
    const limit = d.format === "elf" && d.budget ? " --max-insns=" + d.budget : "";
    return (engines.length ? engines : ["tier3"]).map(e => "ounce-core " + cmd + " " + d.name + " --cpu=" + e + limit).join("\n");
  }

  function observedText(d, o) {
    const ran = d.engines || [], v = d.verdict || {}, lines = ["Result: " + o.label + ". " + o.headline];
    for (const e of ran) lines.push((SHORT[e.engine] || e.engine) + ": " + (e.stop.line || e.stop.kind));
    const x = v.diff;
    if (x) lines.push("First difference: " + x.path + (x.line ? " line " + x.line : "") + (x.a_label ? " (" + x.a_label + ": " + x.a + " | " + x.b_label + ": " + x.b + ")" : x.note ? " (" + x.note + ")" : ""));
    const hr = d.host_reference, f = hr && hr.first_difference;
    if (f) lines.push("Recorded reference differs at line " + f.line + ". Expected: " + (f.expected ?? "(no line)") + " | Got: " + (f.actual ?? "(no line)"));
    const ic = d.instructions;
    if (ic && ic.mismatched > 0 && ic.first) lines.push("Instruction check: " + ic.mismatched + " of " + num(ic.checked) + " sampled instructions differ from this computer, first at address " + ic.first.pc + " (instruction " + ic.first.opcode + ")");
    const tail = ((ran.find(faulted) || {}).log_tail || []).filter(Boolean).slice(-5);
    if (tail.length) lines.push("Last log lines:", ...tail);
    return lines.join("\n");
  }

  // The report as plain fields. `o` is the on-screen outcome ({label, headline}); `ctx` is {config, app, platform}.
  function fields(d, o, ctx = {}) {
    const program = [
      d.name + " (" + (d.format === "elf" ? "static AArch64 Linux program" : "Switch homebrew .nro") + ", " + num(d.bytes) + " bytes)",
      "SHA-256: " + d.sha256,
      "Run in the Ounce Core workbench: " + (d.engines || []).map(e => SHORT[e.engine] || e.engine).join(" and ") + ", step limit " + num(d.budget || (ctx.config || {}).default_budget || 0),
      "Source, commit or release and how it was built: (please add if you know)",
    ].join("\n");
    const expected = "Compared: " + comparison(d) + ".\n" +
      "Expected: the program runs to the end and every engine produces the same output.\n" + fingerprint(d);
    return {
      title: "Workbench: " + o.label + " - " + d.name,
      program: scrub(program), engine: engineChoice(d), expected: scrub(expected),
      observed: scrub(observedText(d, o)), repro: scrub(reproLines(d)), environment: environment(d, ctx),
    };
  }

  const SECTIONS = [["program", "Program and version"], ["engine", "Engine"], ["expected", "Expected result and where it comes from"],
    ["observed", "What Ounce-Core did"], ["repro", "Exact command"], ["environment", "Operating system, chip and Ounce Core version"]];

  // "Copy details": the same fields, in the order and under the headings of the issue form.
  function text(d, o, ctx) {
    const f = fields(d, o, ctx);
    return [f.title, ...SECTIONS.map(([k, label]) => "\n" + label + "\n" + f[k])].join("\n") + "\n";
  }

  function build(f, scale) {
    const q = { template: "emulation_bug.yml", title: clip(f.title, 140), engine: f.engine };
    for (const k of Object.keys(LIMITS)) q[k] = clip(f[k], Math.max(120, Math.floor(LIMITS[k] * scale)));
    return ISSUE_URL + "?" + Object.entries(q).map(([k, v]) => k + "=" + encodeURIComponent(v)).join("&");
  }

  // The prefilled issue form address. Long text is cut until the address fits; program bytes are never included.
  function url(d, o, ctx) {
    const f = fields(d, o, ctx);
    for (const scale of [1, 0.75, 0.5, 0.35, 0.25, 0.1]) {
      const u = build(f, scale);
      if (u.length <= MAX_URL) return u;
    }
    return build({ ...f, observed: "", expected: "", program: "" }, 0.1);
  }

  const api = { fields, text, url, engineChoice, scrub, ENGINES, MAX_URL, ISSUE_URL };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.OunceReport = api;
})(typeof window !== "undefined" ? window : globalThis);
