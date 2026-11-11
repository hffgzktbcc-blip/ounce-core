import Foundation

func check(_ condition: @autoclosure () -> Bool, _ message: String) {
    if !condition() { FileHandle.standardError.write(Data("FAILED: \(message)\n".utf8)); exit(1) }
}

// Budget parsing
check(RunSettings.parseBudget("") == .success(nil), "blank budget is default")
check(RunSettings.parseBudget(" 200,000_000 ") == .success(200_000_000), "separators are accepted")
for bad in ["0", "-5", "abc", "1.5"] {
    if case .success = RunSettings.parseBudget(bad) { check(false, "budget \(bad) must be rejected") }
}

// Arguments and environment
var settings = RunSettings(engine: .tier4, budget: 123)
check(settings.arguments(nro: "/a.nro", sdCard: "/sd").contains("--cpu=tier4"), "engine flag passed")
let env = settings.environment(base: ["OUNCE_CPU": "native", "OUNCE_NRO_BUDGET": "1", "PATH": "/bin"])
check(env["OUNCE_CPU"] == nil && env["OUNCE_NRO_BUDGET"] == "123" && env["PATH"] == "/bin", "environment overlay")
settings.budget = nil
check(settings.environment(base: ["OUNCE_NRO_BUDGET": "1"])["OUNCE_NRO_BUDGET"] == nil, "default budget clears inherited override")

// Diagnosis, using the exact message formats printed by src/main.rs and src/cpu/interpreter.rs
func diag(_ log: String, _ status: Int32 = 1, _ engine: Engine = .tier3, _ budget: Int? = nil) -> Diagnosis {
    Diagnosis.analyze(log: log, exitStatus: status, engine: engine, budget: budget)
}
let unimpl = diag("[VM] Starting\n\n[VM Error] Unimplemented instruction 0x9AC04021 at PC: 0x0000000080001234\n  Final PC: 0x0000000080001234\n")
check(unimpl.kind == .unimplementedInstruction, "unimplemented kind")
check(unimpl.explanation.contains("0x9AC04021") && unimpl.explanation.contains("0x0000000080001234"), "unimplemented word and pc")
check(diag("\n[VM] Execution budget reached (50000000 instructions).\n").kind == .budgetExhausted, "budget kind")
check(diag("\n[VM] Execution budget reached (1000 instructions).", 1, .tier3, 1000).explanation.contains("1,000") ||
      diag("\n[VM] Execution budget reached (1000 instructions).", 1, .tier3, 1000).explanation.contains("1000"), "budget number shown")
let svc = diag("\n[VM] Stopping on unhandled SVC #0x7F.\n")
check(svc.kind == .unhandledSVC && svc.explanation.contains("0x7F"), "svc id")
check(diag("[VM] Stopping on unhandled SVC 0x2A").explanation.contains("0x2A"), "native svc format")
check(diag("\n[VM] Hit svcBreak: reason=0x1, info1=0x0, info2=0x0\n[VM] Break PC: 0x1").kind == .guestBreak, "break kind")
check(diag("[VM] CPU fault (native signal 11) at 0x1000, address 0x0").kind == .nativeSignal, "native signal")
check(diag("\n[VM Error] Memory read fault at 0x10\n  Final PC: 0x80").evidence.count == 2, "fault evidence has pc")
check(diag("[Loader Error] bad header").kind == .loaderError, "loader error")
check(diag("[CPU] --cpu=native needs a build with `--features nce`; running the interpreter.", 0, .native).kind == .engineUnavailable, "fallback is not a plain pass")
check(!diag("[Tier-4] Not available: rebuild with `--features native-jit`; running Tier-3.", 0, .tier4).succeeded, "tier4 fallback is not a pass")
check(Diagnosis.fellBack(log: "x\n[Tier-4] Not available: rebuild; running Tier-3.\n"), "fellBack detects tier4")
check(diag("[CPU] needs a build with x\n[VM Error] Memory fault", 1, .native).kind == .fault, "real failure beats fallback note")
check(diag("Failed to read file: No such file or directory (os error 2)").kind == .loaderError, "unreadable file")
check(diag("\n[VM] Process exited gracefully (svcExitProcess).", 0).succeeded, "clean exit")
check(diag("something odd", 7).kind == .unknownFailure, "unknown failure")
check(diag("", 0).kind == .finished, "exit 0 without markers is finished")
// The first failure wins over later consequences.
check(diag("[VM Error] Unimplemented instruction 0x1 at PC: 0x2\n[VM] Execution budget reached (5 instructions).").kind == .unimplementedInstruction, "first failure wins")

// A finished run shows what the program printed, not the runtime's status lines.
let hello = diag("====\n  OUNCE-CORE: x\n====\n\n[Loader] Reading package/executable: /a.nro\n[BootLoader] Successfully loaded package:\n  Format      : NRO0\n  Code size   : 0x4000\n[VM] Starting execution\nounce-fixture result=0x0BAD0008\n[VM] Process exited gracefully (svcExitProcess).\n[VM Summary] Executed 9067 instructions\n  -> Basic-Block JIT Cache: 1\n", 0)
check(hello.succeeded && hello.output == ["ounce-fixture result=0x0BAD0008"], "program output is extracted: \(hello.output)")
check(hello.text.contains("Program output:\n  ounce-fixture result=0x0BAD0008"), "program output is shown")
check(Diagnosis.programOutput(log: (1...20).map { "line \($0)" }.joined(separator: "\n")).count == 8, "output is limited to the last 8 lines")
check(Diagnosis.programOutput(log: String(repeating: "x", count: 500))[0].count == 200, "long output lines are cut")

// A failed run produces a report that names the file only and hides the home folder.
let failed = diag("[VM Error] Unimplemented instruction 0x9AC04021 at PC: 0x0000000080001234\n  Final PC: /Users/sam/secret", 1, .tier4)
let report = IssueReport(program: "app.nro", engine: .tier4, diagnosis: failed, appVersion: "0.7.0", hostOS: "macOS 26", home: "/Users/sam")
check(report.command == "ounce-core run-nro app.nro --cpu=tier4", "report command")
check(report.engineLabel == "Tier-4 JIT (--cpu=tier4)", "engine label matches the form dropdown")
check(report.body.contains("0x9AC04021") && report.body.contains("macOS 26") && !report.body.contains("/Users/sam"), "report keeps evidence and hides the home folder")
let reportURL = report.url?.absoluteString ?? ""
check(reportURL.hasPrefix("https://github.com/hffgzktbcc-blip/ounce-core/issues/new?template=emulation_bug.yml"), "report opens the emulation bug form")
check(reportURL.contains("engine=Tier-4%20JIT") && !reportURL.contains("Users"), "form is prefilled without the home folder")

// Harness catalog against the committed differential.json
let catalogData = try Data(contentsOf: URL(fileURLWithPath: "docs/verify/differential.json"))
let catalog = try HarnessCatalog.parse(catalogData)
check(catalog.count >= 10, "catalog loaded")
check(HarnessCatalog.match(nroFile: "lz4.nro", in: catalog)?.resultFile == "lz4-results.txt", "lz4 result file")
check(HarnessCatalog.match(nroFile: "lz4_harness.nro", in: catalog)?.name == "lz4", "harness suffix")
check(HarnessCatalog.match(nroFile: "Vector-FP.nro", in: catalog)?.arm64Reference == true, "case-insensitive, arm64 reference")
check(HarnessCatalog.match(nroFile: "homebrew-menu.nro", in: catalog) == nil, "unknown nro")
for harness in catalog {
    let path = "docs/verify/\(harness.dir)/expected-host.txt"
    check(FileManager.default.fileExists(atPath: path), "reference exists for \(harness.name)")
}

// Comparison
let same = ResultComparison.compare(expected: "a\nb\n", actual: "a\nb")
check(same.identical && same.actualLines == 2, "trailing newline tolerated")
let different = ResultComparison.compare(expected: "a\nb\nc\n", actual: "a\nX\n")
check(!different.identical && different.differingLines == 2 && different.firstDifference == 2, "diff counts")
check(different.samples[1].actual == "(missing)", "missing line shown")
let many = ResultComparison.compare(expected: String(repeating: "x\n", count: 20), actual: String(repeating: "y\n", count: 20))
check(many.samples.count == ResultComparison.sampleLimit && many.differingLines == 20, "sample limit")
let lz4 = catalog[catalog.firstIndex { $0.name == "lz4" }!]
check(same.summary(harness: lz4, engine: .tier3, hostIsARM64: true).hasPrefix("Match."), "match summary")
check(different.summary(harness: lz4, engine: .tier3, hostIsARM64: true).hasPrefix("Mismatch."), "mismatch summary")
let fp = catalog[catalog.firstIndex { $0.name == "fp-instructions" }!]
check(different.summary(harness: fp, engine: .tier3, hostIsARM64: false).contains("ARM64 host"), "arm64 caveat")
print("Run settings, diagnosis and host-comparison checks passed.")
