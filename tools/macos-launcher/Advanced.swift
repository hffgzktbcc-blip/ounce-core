import AppKit
import UniformTypeIdentifiers

final class SessionLog {
    let url: URL
    private let lock = NSLock()
    private var file: FileHandle?
    private var written = 0
    private var tail = Data()
    private let limit = 8 * 1024 * 1024

    init(directory: URL) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let stamp = ISO8601DateFormatter().string(from: Date()).replacingOccurrences(of: ":", with: "-")
        url = directory.appendingPathComponent("\(stamp)-\(UUID().uuidString.prefix(8)).log")
        guard FileManager.default.createFile(atPath: url.path, contents: nil) else {
            throw CocoaError(.fileWriteUnknown)
        }
        file = try FileHandle(forWritingTo: url)
    }

    func append(_ data: Data) {
        lock.lock()
        defer { lock.unlock() }
        tail.append(data)
        if tail.count > 256 * 1024 { tail.removeFirst(tail.count - 256 * 1024) }
        guard let file, written < limit else { return }
        let part = data.prefix(limit - written)
        file.write(part)
        written += part.count
        if written == limit { file.write(Data("\n[Ounce Core] Session log capped at 8 MiB.\n".utf8)) }
    }

    func snapshot() -> Data {
        lock.lock()
        defer { lock.unlock() }
        return tail
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        try? file?.close()
        file = nil
    }

    deinit { close() }
}

/// Shows a saved results file (an Ounce-Verify report or a workbench result) in a read-only window.
final class ResultsViewer {
    private var window: NSWindow?

    func show(_ report: EvidenceReport, from url: URL) {
        let viewer = window ?? NSWindow(contentRect: NSRect(x: 0, y: 0, width: 760, height: 560),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window = viewer
        viewer.isReleasedWhenClosed = false
        viewer.title = "Results: \(url.lastPathComponent)"
        viewer.contentMinSize = NSSize(width: 600, height: 400)
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        let text = NSTextView(frame: NSRect(origin: .zero, size: viewer.contentLayoutRect.size))
        text.isEditable = false
        text.isSelectable = true
        text.isVerticallyResizable = true
        text.isHorizontallyResizable = false
        text.autoresizingMask = [.width]
        text.textContainer?.widthTracksTextView = true
        text.textContainerInset = NSSize(width: 24, height: 24)
        text.setAccessibilityLabel("Recorded verification results")
        scroll.documentView = text
        viewer.contentView = scroll
        let body = NSMutableAttributedString(string: "")
        func label(_ value: String, bold: Bool = false) {
            body.append(NSAttributedString(string: value + "\n\n", attributes: [
                .font: NSFont.systemFont(ofSize: bold ? 16 : 13, weight: bold ? .semibold : .regular),
                .foregroundColor: NSColor.labelColor
            ]))
        }
        label(report.heading, bold: true)
        label("These results describe an earlier run. Opening this file does not verify it again or approve a repair.")
        label(report.overview)
        label(url.lastPathComponent)
        if !report.records.isEmpty { label("\(report.records.count) recorded results") }
        for section in report.sections {
            label(section.title.replacingOccurrences(of: "-", with: " ").capitalized, bold: true)
            for line in section.lines { label(line) }
        }
        for record in report.records {
            let parts = record.caseID.split(separator: "/")
            let name = parts.count > 1 ? String(parts[1]) : record.caseID
            label(name.replacingOccurrences(of: "-", with: " ").capitalized, bold: true)
            label(EvidenceReport.explanation(record.verdict))
            label("Case: \(record.caseID)")
            label("Target: \(record.target) • Engine: \(record.execution.engine) • Revision: \(record.execution.commit.prefix(12))")
            for oracle in record.oracles.prefix(32) {
                label("\(oracle.name): \(oracle.status)")
            }
        }
        text.textStorage?.setAttributedString(body)
        viewer.center()
        viewer.makeKeyAndOrderFront(nil)
    }
}

/// The functions of the older launcher that the workbench does not replace. They live in the Advanced menu and have no
/// window of their own: a run shows its own graphics window, and the outcome is reported in a dialog.
final class AdvancedRunner {
    private let runtime: URL
    private let support: URL
    private let window: () -> NSWindow?
    private var process: Process?
    private var sessionID = UUID()
    private var currentLog: SessionLog?
    private var lastLog: URL?
    private var stopRequested = false
    private var lastProgram: URL?
    private var lastEngine: Engine = .tier3
    private var verifying: (harness: Harness, sd: URL, engine: Engine)?
    private var settings: RunSettings
    private let catalog: [Harness]

    init(runtime: URL, window: @escaping () -> NSWindow?) {
        self.runtime = runtime
        self.window = window
        support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Ounce Core", isDirectory: true)
        let engine = UserDefaults.standard.string(forKey: "engine").flatMap(Engine.init(rawValue:)) ?? .tier3
        settings = RunSettings(engine: engine, budget: UserDefaults.standard.object(forKey: "instructionBudget") as? Int)
        if let url = Bundle.main.url(forResource: "differential", withExtension: "json", subdirectory: "Verify"),
           let data = try? Data(contentsOf: url) { catalog = (try? HarnessCatalog.parse(data)) ?? [] } else { catalog = [] }
    }

    var isRunning: Bool { process != nil }

    private var sdCard: URL {
        if let path = UserDefaults.standard.string(forKey: "sdCardFolder") { return URL(fileURLWithPath: path, isDirectory: true) }
        return support.appendingPathComponent("SD Card", isDirectory: true)
    }

    private func notice(_ what: String, _ next: String) { Notice.show(what, next, on: window()) }

    private func choose(_ message: String, prompt: String, types: [UTType], then handler: @escaping (URL) -> Void) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = types
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = message
        panel.prompt = prompt
        guard panel.runModal() == .OK, let url = panel.url else { return }
        handler(url)
    }

    private let nro = UTType(filenameExtension: "nro") ?? .data

    // MARK: Menu actions

    func chooseProgram() {
        guard !isRunning else { return busyNotice() }
        choose("Choose a Switch homebrew program (a file ending in .nro) to run in its own window.", prompt: "Run", types: [nro]) { self.run($0) }
    }

    func run(_ url: URL) {
        guard !isRunning else { return busyNotice() }
        if let problem = WorkbenchLogic.programProblem(url) { return notice(problem.what, problem.next) }
        guard let current = validatedSettings() else { return }
        lastProgram = url
        verifying = nil
        start(current.arguments(nro: url.path, sdCard: sdCard.path), name: url.deletingPathExtension().lastPathComponent, settings: current)
    }

    func graphicsDemo() {
        guard !isRunning else { return busyNotice() }
        var demo = settings
        demo.engine = .tier3 // The built-in demo is a graphics demo for the interpreter path.
        verifying = nil
        lastProgram = nil
        start(["gfx", "--present=gpu"], name: "the graphics demo", settings: demo)
    }

    func chooseVerification() {
        guard !isRunning else { return busyNotice() }
        choose("Choose a verification program, named like the one it was built from (for example lz4.nro).", prompt: "Verify", types: [nro]) { self.verify($0) }
    }

    func showSettings() {
        guard !isRunning else { return busyNotice() }
        let engines = NSPopUpButton(frame: .zero, pullsDown: false)
        for engine in Engine.allCases { engines.addItem(withTitle: engine.title) }
        engines.selectItem(at: Engine.allCases.firstIndex(of: settings.engine) ?? 0)
        let budget = NSTextField()
        budget.placeholderString = "50,000,000 (default)"
        budget.stringValue = settings.budget.map(String.init) ?? ""
        let note = NSTextField(wrappingLabelWithString: "")
        note.font = .systemFont(ofSize: 12)
        note.textColor = .secondaryLabelColor
        func describe() {
            let engine = Engine.allCases[max(engines.indexOfSelectedItem, 0)]
            note.stringValue = engine.summary + (engine.isHeadlessOnly ? " It ignores the instruction budget and uses a time limit." : "")
        }
        describe()
        let handler = SettingsTarget(describe)
        engines.target = handler
        engines.action = #selector(SettingsTarget.changed)
        func row(_ label: String, _ view: NSView) -> NSStackView {
            let text = NSTextField(labelWithString: label)
            text.widthAnchor.constraint(equalToConstant: 120).isActive = true
            return NSStackView(views: [text, view])
        }
        let form = NSStackView(views: [row("Engine", engines), row("Instruction budget", budget), note])
        form.orientation = .vertical
        form.alignment = .leading
        form.spacing = 8
        form.frame = NSRect(x: 0, y: 0, width: 420, height: 110)
        note.widthAnchor.constraint(equalToConstant: 410).isActive = true
        budget.widthAnchor.constraint(equalToConstant: 170).isActive = true
        let alert = NSAlert()
        alert.messageText = "Run settings"
        alert.informativeText = "These apply to Run in a Window and Verify Against Host Reference. The workbench chooses its own engines."
        alert.accessoryView = form
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        withExtendedLifetime(handler) {
            guard alert.runModal() == .alertFirstButtonReturn else { return }
            switch RunSettings.parseBudget(budget.stringValue) {
            case .success(let value):
                settings.engine = Engine.allCases[max(engines.indexOfSelectedItem, 0)]
                settings.budget = value
                UserDefaults.standard.set(settings.engine.rawValue, forKey: "engine")
                if let value { UserDefaults.standard.set(value, forKey: "instructionBudget") } else { UserDefaults.standard.removeObject(forKey: "instructionBudget") }
            case .failure(let error):
                notice("That instruction budget can’t be used.", error.localizedDescription)
            }
        }
    }

    func chooseSDFolder() {
        guard !isRunning else { return busyNotice() }
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.directoryURL = sdCard
        panel.message = "Programs run from the Advanced menu see this folder as their SD card. Currently: \(sdCard.path)"
        panel.prompt = "Use as SD Card"
        if panel.runModal() == .OK, let url = panel.url { UserDefaults.standard.set(url.path, forKey: "sdCardFolder") }
    }

    func showLastLog() {
        if let lastLog { return NSWorkspace.shared.activateFileViewerSelecting([lastLog]) }
        let directory = support.appendingPathComponent("Logs")
        do { try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true); NSWorkspace.shared.open(directory) }
        catch { notice("Ounce Core couldn’t open the log folder.", "\(error.localizedDescription) The logs are in \(directory.path).") }
    }

    func stop() {
        guard let process, process.isRunning else { return }
        stopRequested = true
        currentLog?.append(Data("\n[Ounce Core] Stopped by the user.\n".utf8))
        process.terminate()
    }

    func terminate() { process?.terminate() }

    private func busyNotice() { notice("A program is already running.", "Close its window, or choose Advanced > Stop Running Program, then try again.") }

    private func validatedSettings() -> RunSettings? { settings }

    // MARK: Verify against the host reference

    private func expectedHostURL(_ harness: Harness) -> URL { Bundle.main.resourceURL!.appendingPathComponent("Verify/\(harness.dir)/expected-host.txt") }

    private func verify(_ url: URL) {
        if let problem = WorkbenchLogic.programProblem(url) { return notice(problem.what, problem.next) }
        guard !catalog.isEmpty else {
            return notice("The built-in verification data is missing.", "Reinstall Ounce Core, or rebuild it with scripts/build_macos_app.sh.")
        }
        guard let harness = HarnessCatalog.match(nroFile: url.lastPathComponent, in: catalog) else {
            return notice("\(url.lastPathComponent) isn’t a known verification program.",
                          "Verification only works for programs built from the harnesses recorded in docs/verify, matched by file name: \(catalog.map(\.name).joined(separator: ", ")). To check any other program, use File > Check a Program.")
        }
        guard FileManager.default.fileExists(atPath: expectedHostURL(harness).path) else {
            return notice("The reference result for \(harness.name) is missing from this app.", "Reinstall Ounce Core, or rebuild it with scripts/build_macos_app.sh.")
        }
        var current = settings
        // verify_differential.py uses a large budget for harnesses; an explicit user budget still wins.
        if current.budget == nil { current.budget = RunSettings.verifyBudget }
        do {
            let sd = FileManager.default.temporaryDirectory.appendingPathComponent("ounce-verify-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: sd, withIntermediateDirectories: true)
            verifying = (harness, sd, current.engine)
            lastProgram = url
            start(["run-nro", url.path, "--present=headless", "--sdmc=\(sd.path)", "--cpu=\(current.engine.rawValue)"],
                  name: "verification of \(harness.name)", settings: current, extraEnvironment: ["OUNCE_NRO_TIMEOUT_SECS": "300"])
        } catch { notice("Ounce Core couldn’t prepare the verification.", error.localizedDescription) }
    }

    /// Compares the harness output with the bundled host reference. Only a byte-for-byte comparison.
    private func finishVerification(_ run: (harness: Harness, sd: URL, engine: Engine), diagnosis: Diagnosis) {
        defer { try? FileManager.default.removeItem(at: run.sd) }
        let title = "\(run.harness.name) on \(run.engine.rawValue)"
        func failure(_ headline: String, _ summary: String) { present(headline, summary, report: report(diagnosis, summary: headline + "\n" + summary)) }
        if diagnosis.kind == .engineUnavailable {
            return failure("\(title): not run on that engine", diagnosis.text + "\n\nNo comparison was made, because it would not describe the engine you chose.")
        }
        guard diagnosis.succeeded else { return failure("\(title): the run did not finish", diagnosis.text) }
        guard let actual = try? String(contentsOf: run.sd.appendingPathComponent(run.harness.resultFile), encoding: .utf8) else {
            return failure("\(title): no output", "The program ran but did not write \(run.harness.resultFile), so there is nothing to compare. Check that it was built from the matching harness.")
        }
        guard let expected = try? String(contentsOf: expectedHostURL(run.harness), encoding: .utf8) else {
            return failure("\(title): reference unreadable", "The reference result could not be read, so no comparison was made. Reinstall Ounce Core.")
        }
        let comparison = ResultComparison.compare(expected: expected, actual: actual)
        #if arch(arm64)
        let hostIsARM64 = true
        #else
        let hostIsARM64 = false
        #endif
        let summary = comparison.summary(harness: run.harness, engine: run.engine, hostIsARM64: hostIsARM64)
        if comparison.identical { present("\(title): matches the host reference", summary, report: nil) }
        else { failure("\(title): differs from the host reference", summary) }
    }

    // MARK: Running

    private func report(_ diagnosis: Diagnosis, summary: String? = nil) -> IssueReport? {
        guard let program = lastProgram else { return nil }
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
        return IssueReport(program: program.lastPathComponent, engine: lastEngine, diagnosis: diagnosis, appVersion: version,
                           hostOS: ProcessInfo.processInfo.operatingSystemVersionString, home: NSHomeDirectory(), comparisonSummary: summary)
    }

    /// One dialog for an outcome. A failed run offers Report a Problem: it copies the report and opens the issue form.
    /// Nothing is sent until the person submits the form.
    private func present(_ headline: String, _ body: String, report: IssueReport?) {
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = headline
        alert.informativeText = body
        alert.alertStyle = report == nil ? .informational : .warning
        alert.addButton(withTitle: "OK")
        if report != nil { alert.addButton(withTitle: "Report a Problem…") }
        if lastLog != nil { alert.addButton(withTitle: "Show Log") }
        let response = alert.runModal()
        if response == .alertSecondButtonReturn, let report {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(report.body, forType: .string)
            if let url = report.url { NSWorkspace.shared.open(url) }
        } else if response != .alertFirstButtonReturn { showLastLog() }
    }

    private func start(_ arguments: [String], name: String, settings run: RunSettings, extraEnvironment: [String: String] = [:]) {
        guard FileManager.default.isExecutableFile(atPath: runtime.path) else {
            return notice("The built-in runtime is missing.", "Reinstall Ounce Core, or rebuild it with scripts/build_macos_app.sh.")
        }
        do {
            try FileManager.default.createDirectory(at: sdCard, withIntermediateDirectories: true)
            let log = try SessionLog(directory: support.appendingPathComponent("Logs"))
            lastLog = log.url
            currentLog = log
            stopRequested = false
            lastEngine = run.engine
            let child = Process()
            child.executableURL = runtime
            child.arguments = arguments
            child.currentDirectoryURL = support
            var environment = run.environment(base: ProcessInfo.processInfo.environment) // The GUI path must not inherit OUNCE_CPU.
            environment["RUST_BACKTRACE"] = "1"
            for (key, value) in extraEnvironment { environment[key] = value }
            child.environment = environment
            child.standardInput = FileHandle.nullDevice
            let pipe = Pipe()
            child.standardOutput = pipe
            child.standardError = pipe
            let id = UUID()
            sessionID = id
            pipe.fileHandleForReading.readabilityHandler = { handle in
                let bytes = handle.availableData
                if bytes.isEmpty { handle.readabilityHandler = nil; log.close(); return }
                log.append(bytes)
            }
            child.terminationHandler = { [weak self] finished in
                // Give the pipe a moment to drain so the last log lines are in the diagnosis.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                    guard let self, self.sessionID == id else { return }
                    self.process = nil
                    NSApp.dockTile.badgeLabel = nil
                    let diagnosis = Diagnosis.analyze(log: String(decoding: log.snapshot(), as: UTF8.self), exitStatus: finished.terminationStatus,
                                                      engine: run.engine, budget: run.budget)
                    let verification = self.verifying
                    self.verifying = nil
                    if self.stopRequested {
                        if let verification { try? FileManager.default.removeItem(at: verification.sd) }
                    } else if let verification {
                        self.finishVerification(verification, diagnosis: diagnosis)
                    } else if finished.terminationStatus != 0 {
                        self.present("Ounce Core couldn’t finish running \(name).", diagnosis.text, report: self.report(diagnosis))
                    }
                }
            }
            try child.run()
            process = child
            NSApp.dockTile.badgeLabel = verifying != nil ? "Verifying" : "Running"
        } catch { notice("Ounce Core couldn’t start \(name).", error.localizedDescription) }
    }
}

private final class SettingsTarget: NSObject {
    private let callback: () -> Void
    init(_ callback: @escaping () -> Void) { self.callback = callback }
    @objc func changed() { callback() }
}
