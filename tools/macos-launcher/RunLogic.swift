import Foundation

// Pure, UI-free logic for the Mac app: run settings and plain-language diagnosis of a runtime log.
// Nothing here executes a program; it only builds arguments and reads text the runtime already printed.

enum Engine: String, CaseIterable {
    case tier3, tier4, native

    var title: String {
        switch self {
        case .tier3: return "Interpreter (tier3)"
        case .tier4: return "JIT (tier4)"
        case .native: return "Native host (native)"
        }
    }
    var summary: String {
        switch self {
        case .tier3: return "Reference interpreter. Slowest, and the one to trust when engines disagree."
        case .tier4: return "Recompiles ARM64 to host code. Needs an Apple Silicon build with the JIT feature."
        case .native: return "Runs on the host CPU itself, headless, no window. It checks the toolchain, not the emulator."
        }
    }
    /// Native execution has no window and uses a time limit instead of an instruction budget.
    var isHeadlessOnly: Bool { self == .native }
    var usesInstructionBudget: Bool { self != .native }
}

struct RunSettings: Equatable {
    static let defaultBudget = 50_000_000
    static let verifyBudget = 4_000_000_000
    var engine: Engine = .tier3
    /// nil means the runtime default (50 million instructions).
    var budget: Int? = nil

    /// Parses the budget field. Blank means the default; anything else must be a positive whole number.
    static func parseBudget(_ text: String) -> Result<Int?, BudgetError> {
        let trimmed = text.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "_", with: "").replacingOccurrences(of: ",", with: "")
        if trimmed.isEmpty { return .success(nil) }
        guard let value = Int(trimmed), value > 0 else { return .failure(.notPositiveInteger) }
        return .success(value)
    }
    enum BudgetError: Error, LocalizedError {
        case notPositiveInteger
        var errorDescription: String? { "The instruction budget must be a whole number above zero, or blank for the default (50,000,000)." }
    }

    /// Command line for a visible run. The engine flag is always passed so the choice is never ambiguous.
    func arguments(nro: String, sdCard: String) -> [String] {
        ["run-nro", nro, "--gui", "--present=gpu", "--sdmc=\(sdCard)", "--cpu=\(engine.rawValue)"]
    }

    /// Environment overlay: OUNCE_CPU is removed so only the explicit flag decides, and the budget uses the
    /// runtime's existing OUNCE_NRO_BUDGET mechanism.
    func environment(base: [String: String]) -> [String: String] {
        var env = base
        env.removeValue(forKey: "OUNCE_CPU")
        env.removeValue(forKey: "OUNCE_NRO_BUDGET")
        if let budget { env["OUNCE_NRO_BUDGET"] = String(budget) }
        return env
    }
}

private func grouped(_ value: Int) -> String {
    let formatter = NumberFormatter()
    formatter.numberStyle = .decimal
    formatter.locale = Locale(identifier: "en_US")
    return formatter.string(from: NSNumber(value: value)) ?? String(value)
}

struct Diagnosis: Equatable {
    enum Kind: String { case finished, budgetExhausted, unimplementedInstruction, unhandledSVC, guestBreak, cpuHalt, fault, nativeSignal, loaderError, engineUnavailable, timeLimit, unknownFailure }
    let kind: Kind
    let headline: String
    let explanation: String
    let nextStep: String
    /// Verbatim runtime lines the conclusion rests on.
    let evidence: [String]
    /// What the program itself printed (not runtime status lines), so a finished run shows its result.
    var output: [String] = []
    var succeeded: Bool { kind == .finished }

    var text: String {
        var out = "\(headline)\n\n\(explanation)\n\nWhat to try: \(nextStep)"
        if !output.isEmpty { out += "\n\nProgram output:\n" + output.map { "  " + $0 }.joined(separator: "\n") }
        if !evidence.isEmpty { out += "\n\nFrom the log:\n" + evidence.map { "  " + $0 }.joined(separator: "\n") }
        return out
    }

    /// Lines the guest printed: everything except the runtime's own bracketed or indented status lines and banner.
    /// At most the last `limit` lines, each cut to 200 characters.
    static func programOutput(log: String, limit: Int = 8) -> [String] {
        let own = ["Format ", "Title ID", "Code size", "MOD0 ", "sdmc:/", "OUNCE-CORE", "Target:", "Loader]"]
        let lines = log.split(whereSeparator: \.isNewline).map(String.init).filter { line in
            guard let first = line.first, !line.trimmingCharacters(in: .whitespaces).isEmpty else { return false }
            if first == "[" || first == " " || first == "=" || first == "\t" { return false }
            return !own.contains { line.hasPrefix($0) }
        }
        return lines.suffix(limit).map { String($0.prefix(200)) }
    }

    /// The runtime prints one of these when it silently runs a different engine than requested.
    static func fallbackLine(in lines: [String]) -> String? {
        lines.first { $0.contains("needs a build with") || $0.contains("Native execution unavailable")
            || ($0.contains("[Tier-4]") && $0.contains("Not available")) }
    }
    static func fellBack(log: String) -> Bool { fallbackLine(in: log.split(whereSeparator: \.isNewline).map(String.init)) != nil }

    /// Reads the runtime's log and exit status. The first failure line in the log wins because later lines are usually consequences.
    static func analyze(log: String, exitStatus: Int32, engine: Engine, budget: Int?) -> Diagnosis {
        let lines = log.split(whereSeparator: \.isNewline).map { String($0) }
        func first(_ marker: String) -> String? { lines.first { $0.contains(marker) } }
        func hex(_ line: String, after marker: String) -> String? {
            guard let range = line.range(of: marker) else { return nil }
            let rest = line[range.upperBound...].drop { $0 == " " }
            let digits = rest.prefix { $0.isHexDigit || $0 == "x" || $0 == "X" }
            return digits.isEmpty ? nil : String(digits)
        }

        if let line = first("Unimplemented instruction") {
            let word = hex(line, after: "Unimplemented instruction") ?? "unknown"
            let pc = hex(line, after: "PC:") ?? "unknown"
            return Diagnosis(kind: .unimplementedInstruction,
                headline: "The program used an ARM64 instruction Ounce Core does not implement yet.",
                explanation: "At address \(pc) the CPU fetched the instruction word \(word) and the decoder has no rule for it. This is a gap in Ounce Core, not necessarily a bug in your program, and nothing after this point can be trusted.",
                nextStep: "Look up the word \(word) in the Arm Architecture Reference Manual, and check docs/verify/ for the decoder census, which ranks missing instructions. Running with the native engine on an Apple Silicon Mac shows what real hardware does with the same program.",
                evidence: [line])
        }
        if let line = first("Execution budget reached") {
            let shown = budget ?? RunSettings.defaultBudget
            return Diagnosis(kind: .budgetExhausted,
                headline: "The program was still running when the instruction budget ran out.",
                explanation: "Ounce Core stops a run after \(grouped(shown)) instructions so a program that waits for input or loops cannot run forever. The program did not crash; it was cut off, so this is not a pass or a fail.",
                nextStep: "Raise the Instruction budget in Run Settings (for example ten times higher) and run again. If it still never finishes, the program may be waiting for something Ounce Core does not provide.",
                evidence: [line])
        }
        if let line = first("unhandled SVC") {
            let id = hex(line, after: "SVC #") ?? hex(line, after: "SVC ") ?? "unknown"
            return Diagnosis(kind: .unhandledSVC,
                headline: "The program asked the system for something Ounce Core does not provide.",
                explanation: "It made supervisor call \(id) (an SVC, the way a program requests a kernel service) and the kernel model has no handler for it, so the run stopped.",
                nextStep: "Check docs/svc-coverage.md for call \(id). A missing service is a gap in Ounce Core.",
                evidence: [line])
        }
        if let line = first("Hit svcBreak") {
            var evidence = [line]
            if let result = first("Break result:") { evidence.append(result) }
            return Diagnosis(kind: .guestBreak,
                headline: "The program stopped itself on purpose.",
                explanation: "It called svcBreak, which programs use to report a fatal error (an assertion or an unrecoverable service error). Ounce Core ran the program correctly up to its own error exit; the cause is usually a service result the program did not like.",
                nextStep: "Read the lines just above the break in the session log for the last service request the program made.",
                evidence: evidence)
        }
        if let line = first("CPU Halted") {
            return Diagnosis(kind: .cpuHalt,
                headline: "The CPU model halted.",
                explanation: "The interpreter stopped with a message it could not recover from.",
                nextStep: "Try another engine to see whether the stop depends on the interpreter.",
                evidence: [line])
        }
        if let line = first("CPU fault (native signal") {
            return Diagnosis(kind: .nativeSignal,
                headline: "The program crashed while running on the host CPU.",
                explanation: "Native execution reported a hardware fault (for example a bad memory access). On real hardware the same program would fault the same way, so this is evidence about the program or its build.",
                nextStep: "Run the same program on the interpreter (tier3). If tier3 reports a memory fault at the same address, the program has a bug.",
                evidence: [line])
        }
        if let line = first("[VM Error]") {
            var evidence = [line]
            if let pc = first("Final PC:") { evidence.append(pc.trimmingCharacters(in: .whitespaces)) }
            return Diagnosis(kind: .fault,
                headline: "The program caused a fault.",
                explanation: "The CPU or memory model rejected an operation (such as reading unmapped memory or breaking an alignment rule). The final program counter shows where it stopped.",
                nextStep: "Run it on another engine to see whether they agree. A fault that every engine reports points at the program, one only the interpreter reports points at Ounce Core.",
                evidence: evidence)
        }
        if let line = first("Time limit reached") {
            return Diagnosis(kind: .timeLimit,
                headline: "The run hit its time limit.",
                explanation: "Native execution is limited by time rather than by instruction count.",
                nextStep: "Raise OUNCE_NRO_TIMEOUT_SECS in the environment that launches the app, or use tier3 or tier4, which honour the instruction budget.",
                evidence: [line])
        }
        if let line = first("[Loader Error]") ?? first("Failed to read file") {
            return Diagnosis(kind: .loaderError,
                headline: "The file could not be loaded.",
                explanation: "The loader rejected the program before any instruction ran. The file may be damaged, built for a different target, or use a layout Ounce Core cannot read.",
                nextStep: "Check that it is an unmodified .nro built for AArch64 with devkitA64.",
                evidence: [line])
        }
        if let line = fallbackLine(in: lines) {
            return Diagnosis(kind: .engineUnavailable,
                headline: "The program finished, but not on the engine you chose.",
                explanation: "The runtime fell back to another engine because \(engine.rawValue) is not available in this build or on this Mac (tier4 and native need an Apple Silicon Mac and a build with the JIT and native features). The result below is from the fallback, so it is not evidence about \(engine.rawValue).",
                nextStep: "Choose the interpreter (tier3) explicitly, or rebuild the app on an Apple Silicon Mac with scripts/build_macos_app.sh.",
                evidence: [line])
        }
        if exitStatus == 0, let line = first("Window closed by user") {
            return Diagnosis(kind: .finished,
                headline: "The session ended when the window was closed.",
                explanation: "You (or the program's window) closed it before the program exited by itself, so how it would have ended is unknown.",
                nextStep: "Run it again and leave the window open to see the program's own ending.",
                evidence: [line])
        }
        if exitStatus == 0 {
            let exit = first("Process exited gracefully") ?? first("Guest exited") ?? first("Rendered ")
            return Diagnosis(kind: .finished,
                headline: "The program ran to completion.",
                explanation: "It exited normally on \(engine.title). That says it ran; it does not say the output is correct. Use Verify against host for a harness program to compare results with a real reference.",
                nextStep: "For known verification harnesses, choose Verify against Host….",
                evidence: exit.map { [$0] } ?? [],
                output: programOutput(log: log))
        }
        return Diagnosis(kind: .unknownFailure,
            headline: "The run ended with an error Ounce Core could not classify.",
            explanation: "The runtime exited with status \(exitStatus) and the log has none of the stop messages this panel recognises.",
            nextStep: "Open the session log and look at the last lines.",
            evidence: Array(lines.suffix(3)))
    }
}

/// Text for a bug report about a failed run, shaped like .github/ISSUE_TEMPLATE/emulation_bug.yml. It names the program by
/// file name only and replaces the home folder with "~", so nothing about the user's account leaves the Mac unless they add it.
struct IssueReport {
    static let formURL = "https://github.com/hffgzktbcc-blip/ounce-core/issues/new"

    let program: String
    let engine: Engine
    let diagnosis: Diagnosis
    let appVersion: String
    let hostOS: String
    let home: String

    /// Engine labels exactly as the issue form's dropdown spells them, so the form can preselect one.
    var engineLabel: String {
        switch engine {
        case .tier3: return "Tier-3 (default)"
        case .tier4: return "Tier-4 JIT (--cpu=tier4)"
        case .native: return "Native (--cpu=native)"
        }
    }
    var command: String { "ounce-core run-nro \(program) --cpu=\(engine.rawValue)" }

    private func clean(_ text: String) -> String {
        home.isEmpty ? text : text.replacingOccurrences(of: home, with: "~")
    }
    var title: String { "\(program): \(diagnosis.headline)" }
    /// The runtime lines the diagnosis rests on, or the last log lines when none were recognised.
    var observed: String { clean(([diagnosis.headline] + diagnosis.evidence).joined(separator: "\n")) }
    var environment: String { "Ounce Core app \(appVersion), \(hostOS)" }

    /// Complete text for the clipboard, so the report survives even if the form does not prefill.
    var body: String {
        """
        Program and version: \(program) (give the source, commit or release, and how it was built)
        Engine: \(engineLabel)
        Expected result and where it comes from: (describe the reference, for example a host build of the same source)
        What Ounce-Core did:
        \(observed)
        Exact command: \(command)
        Environment: \(environment)
        """
    }

    /// The issue form with the known fields filled in. Opening it does not submit anything.
    var url: URL? {
        var parts = URLComponents(string: IssueReport.formURL)
        parts?.queryItems = [
            URLQueryItem(name: "template", value: "emulation_bug.yml"),
            URLQueryItem(name: "title", value: title),
            URLQueryItem(name: "program", value: program),
            URLQueryItem(name: "engine", value: engineLabel),
            URLQueryItem(name: "observed", value: String(observed.prefix(1500))),
            URLQueryItem(name: "repro", value: "\(command)\n\(environment)")
        ]
        return parts?.url
    }
}
