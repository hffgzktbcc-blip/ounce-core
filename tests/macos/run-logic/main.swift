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
check(unimpl.nextStep.contains("emulation_bug.yml"), "unimplemented nextStep links bug template")
let tier3Unimpl = diag("[VM Error] Tier-3 fault at 0x80000080 (slice began at 0x80000000): Unimplemented instruction 0x0000DEAD at PC: 0x0000000080000080\n  Final PC: 0x0000000080000080\n")
check(tier3Unimpl.kind == .unimplementedInstruction && tier3Unimpl.explanation.contains("0x0000DEAD"), "tier3 unimplemented kind")
check(diag("\n[VM] Execution budget reached (50000000 instructions).\n").kind == .budgetExhausted, "budget kind")
check(diag("\n[VM] Execution budget reached (1000 instructions).", 1, .tier3, 1000).explanation.contains("1,000") ||
      diag("\n[VM] Execution budget reached (1000 instructions).", 1, .tier3, 1000).explanation.contains("1000"), "budget number shown")
let svc = diag("\n[VM] Stopping on unhandled SVC #0x7F.\n")
check(svc.kind == .unhandledSVC && svc.explanation.contains("0x7F") && svc.nextStep.contains("emulation_bug.yml"), "svc id and bug template")
check(diag("[VM] Stopping on unhandled SVC 0x2A").explanation.contains("0x2A"), "native svc format")
let brk = diag("\n[VM] Hit svcBreak: reason=0x1, info1=0x0, info2=0x4\n[VM] Break PC: 0x80000090\n[VM] Break result: 0x0000F601 (module=1, description=123)")
check(brk.kind == .guestBreak && brk.evidence.count == 3, "break kind with pc and result")
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
let report = IssueReport(program: "app.nro", engine: .tier4, diagnosis: failed, appVersion: "0.7.0", hostOS: "macOS 26", home: "/Users/sam", hostChip: "Apple M2")
check(report.command == "ounce-core run-nro app.nro --cpu=tier4", "report command")
check(report.engineLabel == "Tier-4 recompiler (--cpu=tier4)", "engine label matches the form dropdown")
check(report.body.contains("0x9AC04021") && report.body.contains("macOS 26") && !report.body.contains("/Users/sam"), "report keeps evidence and hides the home folder")
let reportURL = report.url?.absoluteString ?? ""
check(reportURL.hasPrefix("https://github.com/hffgzktbcc-blip/ounce-core/issues/new?template=emulation_bug.yml"), "report opens the emulation bug form")
check(reportURL.contains("engine=Tier-4%20recompiler") && !reportURL.contains("Users"), "form is prefilled without the home folder")
check(reportURL.contains("expected=") && reportURL.contains("environment=Ounce%20Core%20app%200.7.0,%20macOS%2026,%20Apple%20M2"), "form carries expected result and environment: \(reportURL)")
check(report.body.contains("Expected result and where it comes from\nCompared:") && report.body.contains("Apple M2"), "clipboard text follows the form's headings")

print("Run settings and diagnosis checks passed.")
