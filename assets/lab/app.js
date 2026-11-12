"use strict";
// The Ounce Core workbench page. One flow: choose a program, pick how to check it, read the result.
// The Mac app embeds this page and drives it through `window.OunceWorkbench` (see docs/WORKBENCH_UI.md).
const TOKEN = document.body.dataset.token;
const NATIVE = document.body.dataset.native === "true";
const COMPUTER = /Mac/i.test(navigator.platform || "") ? "this Mac" : "this computer";
const ISSUE_URL = "https://github.com/hffgzktbcc-blip/ounce-core/issues/new";
const SETTINGS_KEY = "ounce.workbench.settings";

const $ = id => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") e.className = v;
    else if (k === "onclick") e.onclick = v;
    else if (v !== false && v != null) e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of kids.flat()) if (c != null && c !== false) e.append(c.nodeType ? c : document.createTextNode(c));
  return e;
};
const api = (path, opts = {}) => fetch(path, { ...opts, headers: { "X-Ounce-Token": TOKEN, ...(opts.headers || {}) } }).then(async r => {
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error || "The workbench returned an error (" + r.status + ").");
  return j;
});

let config = { native: NATIVE, default_budget: 2000000000, reference_data: false };
let staged = null;          // the file chosen in step 1, not yet checked
let checks = [], cur = null, detail = null, timer = null, starting = false;
let busySince = 0;         // when the check on screen started showing its progress
const MIN_BUSY_MS = 800;    // a very short check still shows its progress state, so the person sees it ran
let announced = null;       // id of the finished check whose result already took focus

// The three outcomes, in the words the Mac app and saved results use too.
const LABELS = { match: "Results match", differ: "Results differ", fail: "Could not run this program" };

const engineName = e => e === "tier3" ? "Interpreter (Tier-3)" : e === "tier4" ? "Recompiler (Tier-4)" : "Directly on " + COMPUTER;
const fmt = n => Number(n).toLocaleString();

// ---------- settings (step 2) ----------

function readBudget() {
  const text = $("budget").value.replace(/[,\s_]/g, "");
  if (text === "") return { value: null };
  if (!/^[0-9]+$/.test(text) || Number(text) < 1) return { error: "Enter a whole number greater than zero, or leave it empty for the default." };
  return { value: Number(text) };
}

function options() {
  const b = readBudget();
  return { engines: $("engines").value, budget: b.value ?? null, native: $("native").checked && config.native, reference: $("reference").checked && config.reference_data };
}

function queryString(o = options()) {
  const q = new URLSearchParams();
  q.set("engines", o.engines);
  if (o.budget) q.set("budget", String(o.budget));
  q.set("native", o.native ? "1" : "0");
  if (o.reference) q.set("reference", "1");
  return q.toString();
}

function setOptions(o = {}) {
  if (o.engines && [...$("engines").options].some(x => x.value === o.engines)) $("engines").value = o.engines;
  if ("budget" in o) $("budget").value = o.budget ? String(o.budget) : "";
  if ("native" in o) $("native").checked = !!o.native && config.native;
  if ("reference" in o) $("reference").checked = !!o.reference && config.reference_data;
  saveSettings();
  describeSettings();
}

function saveSettings() {
  try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(options())); } catch (_) { /* private mode: settings just reset next time */ }
}

function loadSettings() {
  try { const s = JSON.parse(localStorage.getItem(SETTINGS_KEY) || "null"); if (s) setOptions(s); } catch (_) { /* ignore */ }
}

function describeSettings() {
  const o = options(), parts = [];
  parts.push(o.engines === "tier3,tier4" ? "runs the program on both of Ounce Core's engines and compares what they do" :
    "runs the program on the " + (o.engines === "tier3" ? "interpreter" : "recompiler") + " only");
  if (o.native) parts.push("also runs it on " + COMPUTER + " as the reference");
  if (o.reference) parts.push("compares its output with the recorded reference");
  const custom = o.engines !== "tier3,tier4" || o.budget || o.native || o.reference;
  $("howSummary").textContent = (custom ? "Custom check: " : "Standard check: ") + parts.join(", ") + ".";
  const b = readBudget();
  $("budgetError").hidden = !b.error;
  $("budgetError").textContent = b.error || "";
}

function setupAvailability() {
  const native = $("native").closest(".field"), ref = $("reference").closest(".field");
  $("nativeLabel").textContent = "Also run on " + COMPUTER + " and compare";
  if (!config.native) {
    $("native").disabled = true; native.classList.add("off");
    $("nativeHint").textContent = "Not available in this build. It needs an Apple Silicon Mac (or another ARM64 computer) and a build with native execution turned on.";
  }
  if (!config.reference_data) {
    $("reference").disabled = true; ref.classList.add("off");
    $("referenceHint").textContent = "Not available: the recorded reference data is not installed with this copy of Ounce Core.";
  }
  $("budget").placeholder = "Default (" + fmt(config.default_budget) + ")";
}

// ---------- step 1: choose ----------

// Stage a program for the next check: a File, or the id of a check the server already holds (the Mac app uploads
// programs itself, then calls stage(id) so "Check again" has the program to work with).
async function stage(source) {
  $("chooseError").hidden = true;
  if (!source) return;
  let file = source;
  if (typeof source === "string") {
    try {
      const r = await fetch("/api/checks/" + encodeURIComponent(source) + "/program", { headers: { "X-Ounce-Token": TOKEN } });
      if (!r.ok) throw new Error();
      const d = checks.find(c => c.id === source) || (cur === source && detail) || (await api("/api/checks/" + encodeURIComponent(source)));
      file = new File([await r.blob()], d.name, { type: "application/octet-stream" });
    } catch (_) { return false; }
  }
  if (!/\.nro$/i.test(file.name)) { chooseError("That is not a homebrew program. Choose a file that ends in .nro."); return false; }
  staged = file;
  renderStaged();
  return true;
}

function chooseError(message) {
  $("chooseError").textContent = message;
  $("chooseError").hidden = false;
}

function renderStaged() {
  // Once a program is chosen the drop zone gives way to its name and "Choose another".
  $("drop").hidden = !!staged;
  $("chosen").hidden = !staged;
  $("chosenName").textContent = staged ? staged.name : "";
  $("chosenSize").textContent = staged ? fmt(staged.size) + " bytes" : "";
  $("first").hidden = !!staged || checks.length > 0;
  $("go").disabled = !staged || starting;
  $("goHelp").textContent = staged ? "" : "Choose a program first.";
}

async function startCheck() {
  if (!staged || starting) return;
  const b = readBudget();
  if (b.error) { $("advanced").open = true; $("budget").focus(); return describeSettings(); }
  const o = options();
  if (o.native && !window._nativeAccepted) {
    if (!confirm("This runs the program directly on " + COMPUTER + ", outside the emulator's protection. Only do this for programs you trust.\n\nRun it on " + COMPUTER + "?")) return;
    window._nativeAccepted = true;
  }
  starting = true; renderStaged(); renderResult(); $("result").scrollIntoView({ block: "nearest" });
  try {
    const r = await api("/api/checks?name=" + encodeURIComponent(staged.name) + "&" + queryString(o), { method: "POST", body: staged });
    saveSettings();
    showCheck(r.id, true);
  } catch (e) {
    chooseError(e.message);
  } finally {
    starting = false; renderStaged(); renderResult();
  }
}

// ---------- step 3: result ----------

// fresh: a check that was just started, so its progress shows even when it ends at once.
function showCheck(id, fresh = false) {
  cur = id; detail = null; announced = null; busySince = fresh ? Date.now() : 0;
  refresh();
}

async function refresh() {
  try {
    checks = (await api("/api/checks")).checks;
    if (cur) detail = await api("/api/checks/" + cur);
  } catch (e) { chooseError(e.message); }
  render();
  clearTimeout(timer);
  if (checks.some(c => c.status === "running")) timer = setTimeout(refresh, 1500);
}

const FAULTS = {
  "unhandled-svc": "The program asked the console's system for something Ounce Core does not support yet.",
  "unimplemented": "The program uses a processor instruction that Ounce Core does not support yet.",
  "fault": "The program crashed while it was running.",
  "break": "The program stopped itself because it hit an error.",
  "halt": "The program's processor stopped unexpectedly.",
  "loader": "The program could not be loaded. The file may be damaged or built for something else.",
};

function describeRun(d) {
  const first = d.engines.find(e => FAULTS[e.stop.kind]);
  return first ? FAULTS[first.stop.kind] : null;
}

// What to tell the person: one of three outcomes, in plain words, plus what to do next.
function outcome(d) {
  const v = d.verdict, hr = d.host_reference, ref = d.engines.some(e => e.engine === "native");
  const files = (d.engines.find(e => e.engine === "native") || d.engines[0] || { files: {} }).files;
  const n = Object.keys(files).length;
  if (hr && hr.status === "differs") {
    const f = hr.first_difference;
    return { tone: "bad", label: LABELS.differ, headline: "The output does not match the recorded reference.",
      why: "The program finished, but its output differs from the output recorded from real hardware" + (f ? ", starting at line " + f.line + "." : "."),
      next: ["Send a report so this can be investigated. It includes the details below."], report: true };
  }
  if (!d.engines.length || d.engines.every(e => e.stop.kind === "unknown")) {
    const note = (d.engines.find(e => e.note) || {}).note;
    return { tone: "bad", label: LABELS.fail, headline: "The check could not start the program.",
      why: note || "Ounce Core did not report how the program ended.", next: ["Check that the program is a valid .nro file and try again. If it keeps happening, send a report."], report: true };
  }
  switch (v.kind) {
    case "matches": return { tone: "ok", label: LABELS.match,
      headline: "The program behaved the same in Ounce Core and on " + COMPUTER + ".",
      why: "Ounce Core's engines ended the same way as the program running directly on " + COMPUTER + ", and everything it wrote was identical (" + n + " compared: its files and console text). This covers this one run only.",
      next: ["Nothing more to do. Check another program, or try different settings."] };
    case "agrees": {
      if (hr && hr.status === "matches") return { tone: "ok", label: LABELS.match,
        headline: "The output matches the recorded reference.",
        why: "The program finished and wrote exactly the output recorded from real hardware (" + hr.expected_lines + " lines for " + hr.harness + "). The engines also agreed with each other.", next: ["Nothing more to do."] };
      const one = d.engines.filter(e => e.engine !== "native").length < 2;
      return { tone: "ok", label: LABELS.match,
        headline: one ? "The program ran to the end." : "Both of Ounce Core's engines agreed.",
        why: (one ? "It finished on the one engine you chose." : "They ended the same way and produced identical output.") + " It was not compared with " + COMPUTER + ", so this does not show it matches real hardware.",
        next: config.native ? ["To compare with " + COMPUTER + ", check again with that option turned on."] : ["Comparing with " + COMPUTER + " is not available in this build."],
        again: config.native ? { label: "Check again and compare with " + COMPUTER, opts: { native: true } } : null };
    }
    case "diverges": {
      const refd = ref || (hr && hr.status === "differs");
      return { tone: "bad", label: LABELS.differ,
        headline: refd ? "Ounce Core and " + COMPUTER + " gave different results." : "Ounce Core's two engines gave different results.",
        why: "Both finished, but they did not end the same way or write the same output. " + (v.diff ? "The first difference is below." : "The details show where."),
        next: ["Send a report so this can be investigated. It includes the details below."], report: true };
    }
    case "unfinished": return { tone: "warn", label: LABELS.fail,
      headline: "The program did not finish within the limit.",
      why: "It was still running when the step limit or time limit was reached. Programs that wait for a button press or a timer never finish on their own.",
      next: ["Raise the step limit under Advanced and check again."],
      again: { label: "Check again with a larger limit", opts: { budget: Math.min((d.budget || config.default_budget) * 10, 100000000000) } } };
    case "fails": default: {
      const reason = describeRun(d) || "The program stopped with an error.";
      const next = [ref ? "It fails the same way on " + COMPUTER + ", so the program itself is the likely cause." :
        "This does not say whether the program or Ounce Core is at fault." + (config.native ? " Check again with the comparison against " + COMPUTER + " turned on to find out." : "")];
      if (d.engines.some(e => e.stop.kind === "loader")) next.unshift("Check that the file is a complete, unmodified program, then try again.");
      next.push("If you think Ounce Core is at fault, send a report.");
      return { tone: "bad", label: LABELS.fail, headline: reason, why: "The check stopped before the program finished. The details show exactly where.",
        next, report: true, again: !ref && config.native ? { label: "Check again and compare with " + COMPUTER, opts: { native: true } } : null };
    }
  }
}

function reportText(d, o) {
  const engines = d.engines.map(e => engineName(e.engine) + ": " + (e.stop.line || e.stop.kind)).join("\n");
  const diff = d.verdict.diff ? "\nFirst difference: " + d.verdict.diff.path + (d.verdict.diff.line ? " line " + d.verdict.diff.line : "") : "";
  return ["Result: " + o.label + ". " + o.headline, engines + diff, "Program: " + d.name + " (" + fmt(d.bytes) + " bytes)", "SHA-256: " + d.sha256,
    "Step limit: " + fmt(d.budget || config.default_budget), "Checked with the Ounce Core workbench."].join("\n");
}

function reportUrl(d, o) {
  const first = d.engines.find(e => FAULTS[e.stop.kind]) || d.engines[0] || { engine: "tier3", stop: {} };
  // The form's Engine dropdown only preselects one of its own labels.
  const formEngine = { tier3: "Tier-3 (default)", tier4: "Tier-4 JIT (--cpu=tier4)", native: "Native (--cpu=native)" }[first.engine] || "Not sure";
  const q = new URLSearchParams({ template: "emulation_bug.yml", title: "Workbench: " + o.label + " - " + d.name, program: d.name,
    engine: formEngine, observed: reportText(d, o).slice(0, 1500), repro: "ounce-core run-nro " + d.name + " --cpu=" + first.engine });
  return ISSUE_URL + "?" + q.toString();
}

// Opens the report form. The Mac app handles the "ounceWorkbench" message; a browser opens a new tab.
function report(d, o) {
  const url = reportUrl(d, o), text = reportText(d, o);
  const bridge = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ounceWorkbench;
  if (bridge) return bridge.postMessage({ action: "report", url, text });
  if (!window.open(url, "_blank", "noopener")) location.assign(url); // embedded views route this to the default browser
}

// The saved report: the answer in words first, then the same details Report This Problem carries.
function reportFile(d, o) {
  return [o.label, "", o.headline, o.why, "", "What to do next:", ...o.next.map(t => "- " + t), "", reportText(d, o), ""].join("\n");
}

// Saves the report as a text file: the Mac app shows a save panel, a browser downloads it.
function saveReport(d, o) {
  const text = reportFile(d, o), name = d.name.replace(/\.nro$/i, "") + " - report.txt";
  const bridge = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ounceWorkbench;
  if (bridge) return bridge.postMessage({ action: "save", name, text });
  const link = el("a", { href: URL.createObjectURL(new Blob([text], { type: "text/plain" })), download: name });
  document.body.append(link); link.click(); link.remove();
}

// What Help > Report a Problem in the Mac app opens: the same prefilled report as the button, for the check on
// screen (or the newest finished one). Null when there is no finished check yet.
function reportRequest() {
  const d = detail && detail.status !== "running" ? detail : null;
  if (!d) return null;
  const o = outcome(d);
  return { url: reportUrl(d, o), text: reportText(d, o) };
}

async function copy(text, button) {
  try { await navigator.clipboard.writeText(text); button.textContent = "Copied"; }
  catch (_) { button.textContent = "Could not copy"; }
  setTimeout(() => { button.textContent = "Copy details"; }, 2000);
}

function evidence(d) {
  const ref = d.engines.find(e => e.engine === "native");
  const rows = d.engines.map(e => {
    let tag;
    if (e.engine === "native") tag = ["mut", "reference"];
    else if (ref) { const same = e.stop.kind === ref.stop.kind && e.exit_code === ref.exit_code && JSON.stringify(e.files) === JSON.stringify(ref.files); tag = same ? ["ok", "same as " + COMPUTER] : ["bad", "differs from " + COMPUTER]; }
    else tag = e.stop.kind === "exited" ? ["ok", "finished"] : ["warn", "did not finish"];
    return el("tr", {}, el("td", {}, engineName(e.engine)), el("td", {}, e.stop.line || e.stop.kind), el("td", {}, e.seconds + " s"), el("td", {}, el("span", { class: "pill " + tag[0] }, tag[1])));
  });
  const parts = [el("table", {}, el("thead", {}, el("tr", {}, el("th", {}, "Engine"), el("th", {}, "How it ended"), el("th", {}, "Time"), el("th", {}, "Outcome"))), el("tbody", {}, rows))];
  const x = d.verdict.diff;
  if (x) parts.push(el("div", { class: "diff" }, el("div", {}, x.path + (x.line ? ", line " + x.line : "")),
    x.note ? el("div", {}, x.note) : [el("div", { class: "a" }, x.a_label + ": " + x.a), el("div", { class: "b" }, x.b_label + ": " + x.b)]));
  const hr = d.host_reference;
  if (hr) {
    const f = hr.first_difference;
    parts.push(el("p", { class: "help" }, hr.status === "matches" ? "Recorded reference: identical (" + hr.expected_lines + " lines)." :
      hr.status === "differs" ? "Recorded reference: differs" + (f ? " at line " + f.line + "." : ".") : "Recorded reference: " + (hr.note || hr.status) ));
    if (f) parts.push(el("div", { class: "diff" }, el("div", { class: "b" }, "Expected: " + (f.expected ?? "(no line)")), el("div", { class: "a" }, "Got: " + (f.actual ?? "(no line)"))));
  }
  const ic = d.instructions;
  if (ic) {
    const f = ic.first;
    parts.push(el("p", { class: "help" }, (ic.mismatched > 0 ?
      ic.mismatched + " of " + fmt(ic.checked) + " sampled instructions gave a different result on " + COMPUTER + (f ? " (first at address " + f.pc + ", instruction " + f.opcode + ")." : ".") :
      fmt(ic.checked) + " sampled instructions were replayed on " + COMPUTER + " and none disagreed.") + " Only simple instructions in this one run are replayed."));
  }
  return parts;
}

function details(d) {
  const facts = el("dl", { class: "facts" },
    el("dt", {}, "Program"), el("dd", {}, d.name), el("dt", {}, "Size"), el("dd", {}, fmt(d.bytes) + " bytes"),
    el("dt", {}, "SHA-256"), el("dd", { class: "mono" }, d.sha256),
    el("dt", {}, "Step limit"), el("dd", {}, fmt(d.budget || config.default_budget)),
    el("dt", {}, "Compared"), el("dd", {}, Object.keys((d.engines.find(e => e.engine === "native") || d.engines[0] || { files: {} }).files).join(", ") || "nothing written"));
  const logs = d.engines.map(e => el("div", { class: "engine" }, el("h4", {}, engineName(e.engine)), el("pre", {}, e.log_tail.join("\n") || "(no output)")));
  return [facts, el("h3", {}, "Last lines of each log"), logs, el("p", { class: "help" }, d.verdict.detail ? "Technical summary: " + d.verdict.detail : "")];
}

// The progress state: what is happening now, in the three steps the person can follow.
function busyView(name, loading, d) {
  const where = d && d.native ? "both engines and " + COMPUTER : "the engines";
  const step = (text, on) => el("span", { class: on ? "on" : "", "aria-current": on ? "step" : false }, text);
  return el("div", { class: "verdict busy", role: "status" }, el("div", { class: "label" }, "Checking " + name + "…"),
    el("p", { class: "why" }, "Running the program on " + where + ". Long programs can take a minute or two."),
    el("div", { class: "steps" }, step("Load program", loading), step("Run on engines", !loading), step("Compare results", false)),
    el("div", { class: "bar", "aria-hidden": "true" }));
}

function renderResult() {
  const sec = $("result"), body = $("resultBody");
  const showBusy = starting && !detail;
  sec.hidden = !detail && !showBusy;
  body.replaceChildren();
  if (showBusy) return body.append(busyView(staged ? staged.name : "the program", true));
  if (!detail) return;
  const d = detail;
  body.append(el("div", { class: "run-head" }, el("span", { class: "run-name" }, d.name), el("span", { class: "run-meta mono" }, d.sha256.slice(0, 12) + " · " + fmt(d.bytes) + " bytes")));
  const wait = MIN_BUSY_MS - (Date.now() - busySince);
  if (d.status === "running" || (wait > 0 && announced !== d.id && busySince)) {
    body.append(busyView(d.name, false, d));
    if (d.status !== "running") setTimeout(renderResult, wait + 20);
    return;
  }
  const o = outcome(d);
  const verdict = el("div", { class: "verdict " + o.tone, tabindex: "-1", id: "verdict" }, el("div", { class: "label" }, o.label), el("p", { class: "headline" }, o.headline), el("p", { class: "why" }, o.why));
  const acts = [];
  if (o.again) acts.push(el("button", { type: "button", class: "primary", onclick: () => rerun(o.again.opts) }, o.again.label));
  if (o.report) acts.push(el("button", { type: "button", class: o.again ? "" : "primary", onclick: () => report(d, o) }, "Report This Problem"), el("button", { type: "button", onclick: ev => copy(reportText(d, o), ev.target) }, "Copy details"));
  acts.push(el("button", { type: "button", onclick: () => saveReport(d, o) }, "Save Report…"), el("button", { type: "button", onclick: reset }, "Check another program"));
  verdict.append(el("div", { class: "next" }, el("h3", {}, "What to do next"), o.next.map(t => el("p", {}, t)), el("div", { class: "actions" }, acts)));
  body.append(verdict, el("div", { class: "more" },
    el("details", {}, el("summary", {}, "Evidence"), el("div", {}, evidence(d))),
    el("details", {}, el("summary", {}, "Details"), el("div", {}, details(d)))));
  if (announced !== d.id) { announced = d.id; verdict.focus({ preventScroll: false }); }
}

async function rerun(opts) {
  // A program the app uploaded is not staged yet: ask the server for it.
  if (!staged && cur) await stage(cur);
  if (!staged) { reset(); return chooseError("Choose the program again to check it with different settings."); }
  setOptions(opts);
  startCheck();
}

function reset() {
  cur = null; detail = null; announced = null;
  render();
  $("chooseError").hidden = true;
  $("drop").focus();
  window.scrollTo(0, 0);
}

// ---------- recent checks ----------

function outcomeOf(c) {
  if (c.status === "running") return "Checking";
  return { matches: LABELS.match, agrees: LABELS.match, diverges: LABELS.differ, fails: LABELS.fail, unfinished: LABELS.fail }[c.kind] || "Done";
}

function renderList() {
  const l = $("list");
  l.replaceChildren();
  $("recent").hidden = !checks.length;
  $("first").hidden = !!checks.length || !!staged;
  $("recentSummary").textContent = "Recent checks (" + checks.length + ")";
  for (const c of checks) l.append(el("li", {}, el("button", { type: "button", "aria-current": c.id === cur ? "true" : "false", onclick: () => { showCheck(c.id); $("result").scrollIntoView(); } },
    el("span", { class: "n" }, c.name), el("span", { class: "run-meta" }, outcomeOf(c)))));
}

function render() { renderList(); renderResult(); }

// ---------- wiring ----------

function wire() {
  const drop = $("drop");
  drop.onclick = () => $("file").click();
  $("file").onchange = e => { stage(e.target.files[0]); e.target.value = ""; };
  drop.ondragover = e => { e.preventDefault(); drop.classList.add("over"); };
  drop.ondragleave = () => drop.classList.remove("over");
  drop.ondrop = e => { e.preventDefault(); drop.classList.remove("over"); if (e.dataTransfer.files[0]) stage(e.dataTransfer.files[0]); };
  document.addEventListener("dragover", e => e.preventDefault());
  document.addEventListener("drop", e => { e.preventDefault(); if (e.dataTransfer.files[0]) stage(e.dataTransfer.files[0]); });
  $("another").onclick = () => $("file").click();
  for (const id of ["engines", "budget", "native", "reference"]) $(id).addEventListener("input", () => { describeSettings(); saveSettings(); });
  $("go").onclick = startCheck;
  $("budget").addEventListener("keydown", e => { if (e.key === "Enter") startCheck(); });
}

// The page-level actions the Mac app (or a test) may call.
window.OunceWorkbench = {
  choose: () => $("file").click(),
  options, setOptions, queryString,
  pick: id => showCheck(id, true),
  stage, reportRequest,
  reset,
};
window.pick = id => showCheck(id, true); // kept: the Mac app already calls pick('<id>') after uploading a program itself

(async function init() {
  wire();
  try { config = { ...config, ...(await api("/api/config")) }; } catch (_) { /* older server: keep defaults */ }
  setupAvailability();
  loadSettings();
  describeSettings();
  renderStaged();
  refresh();
})();
