import AppKit
import UniformTypeIdentifiers

// A native front end for the existing CLI. The runtime remains a separate child process.
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
        if written == limit { file.write(Data("\n[Launcher] Session log capped at 8 MiB.\n".utf8)) }
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

/// A card that accepts a dropped file. It only reports the file; the Launcher decides what to do with it.
final class DropView: NSBox {
    var onDrop: ((URL) -> Void)?

    override init(frame: NSRect) { super.init(frame: frame); registerForDraggedTypes([.fileURL]) }
    required init?(coder: NSCoder) { fatalError("not used") }

    private func file(_ info: NSDraggingInfo) -> URL? {
        (info.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL])?.first
    }
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation { file(sender) == nil ? [] : .copy }
    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        guard let url = file(sender) else { return false }
        onDrop?(url)
        return true
    }
}

final class Launcher: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var window: NSWindow!
    private var openButton: NSButton!
    private var demoButton: NSButton!
    private var stopButton: NSButton!
    private var folderButton: NSButton!
    private var status = NSTextField(wrappingLabelWithString: "Ready to open an app")
    private var details = NSStackView()
    private var detailsButton: NSButton!
    private var openMenuItem: NSMenuItem!
    private var folderLabel = NSTextField(labelWithString: "")
    private var console = NSTextView()
    private var consoleBytes = Data()
    private var process: Process?
    private var sessionID = UUID()
    private var lastLog: URL?
    private var currentLog: SessionLog?
    private var stopRequested = false
    private var awaitingGuestFrame: String?
    private var pendingFile: URL?
    private var reportWindow: NSWindow?
    private var emptyCard: DropView!
    private var enginePopUp: NSPopUpButton!
    private var engineNote = NSTextField(wrappingLabelWithString: "")
    private var budgetField = NSTextField()
    private var verifyButton: NSButton!
    private var diagnosisCard = NSBox()
    private var diagnosisHeadline = NSTextField(wrappingLabelWithString: "")
    private var diagnosisBody = NSTextField(wrappingLabelWithString: "")
    private var settings = RunSettings()
    private var lastProgram: URL?
    private var reportButton: NSButton!
    private var lastEngine: Engine?
    private var pendingReport: IssueReport?
    private var verifying: (harness: Harness, sd: URL, engine: Engine)?
    private var catalog: [Harness] = []
    private var workbench: Workbench!

    private var support: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Ounce Core", isDirectory: true)
    }

    private var sdCard: URL {
        if let path = UserDefaults.standard.string(forKey: "sdCardFolder") {
            return URL(fileURLWithPath: path, isDirectory: true)
        }
        return support.appendingPathComponent("SD Card", isDirectory: true)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        let menu = NSMenu()
        let applicationMenu = NSMenu()
        applicationMenu.addItem(withTitle: "About Ounce Core", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        applicationMenu.addItem(.separator())
        let checkItem = applicationMenu.addItem(withTitle: "Check a Program…", action: #selector(checkProgram), keyEquivalent: "o")
        checkItem.target = self
        openMenuItem = applicationMenu.addItem(withTitle: "Run in a Window (Classic)…", action: #selector(chooseHomebrew), keyEquivalent: "r")
        openMenuItem.target = self
        let classicItem = applicationMenu.addItem(withTitle: "Show Classic Launcher", action: #selector(showClassic), keyEquivalent: "")
        classicItem.target = self
        applicationMenu.addItem(.separator())
        applicationMenu.addItem(withTitle: "Quit Ounce Core", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let item = NSMenuItem()
        item.submenu = applicationMenu
        menu.addItem(item)
        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        let editItem = NSMenuItem(title: "Edit", action: nil, keyEquivalent: "")
        editItem.submenu = edit
        menu.addItem(editItem)
        NSApp.mainMenu = menu

        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 680, height: 600),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Ounce Core"
        window.contentMinSize = NSSize(width: 640, height: 600)
        window.delegate = self
        window.isReleasedWhenClosed = false

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 16
        stack.translatesAutoresizingMaskIntoConstraints = false
        let content = window.contentView!
        content.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 32),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -32),
            stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 28),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: content.bottomAnchor, constant: -24)
        ])

        let heading = NSStackView()
        heading.orientation = .vertical
        heading.alignment = .leading
        heading.spacing = 5
        let symbol = NSImageView()
        symbol.image = NSImage(systemSymbolName: "gamecontroller.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 26, weight: .semibold))
        symbol.contentTintColor = .controlAccentColor
        symbol.setAccessibilityElement(false)
        heading.addArrangedSubview(symbol)
        let title = NSTextField(wrappingLabelWithString: "Understand why programs fail. Prove when they work.")
        title.font = .systemFont(ofSize: 24, weight: .bold)
        heading.addArrangedSubview(title)
        stack.addArrangedSubview(heading)

        let intro = NSTextField(wrappingLabelWithString: "Ounce Core runs ARM64 programs on several engines, explains where a run stopped, and compares results with a reference recorded on real hardware.")
        intro.font = .systemFont(ofSize: 14)
        intro.textColor = .secondaryLabelColor
        stack.addArrangedSubview(intro)

        // First-run state: what the app does and where to start. Hidden once a program has been run.
        emptyCard = DropView()
        emptyCard.boxType = .custom
        emptyCard.borderWidth = 1
        emptyCard.borderColor = .separatorColor
        emptyCard.cornerRadius = 12
        emptyCard.fillColor = .controlBackgroundColor
        emptyCard.onDrop = { [weak self] url in self?.openProgram(url) }
        let emptyText = NSTextField(wrappingLabelWithString: """
            Drop a program here, or choose Open Program….

            • Homebrew .nro files run today.
            • Static AArch64 Linux ELF programs run in the CLI with: ounce-core run-elf <file>.
            • Choose an engine and an instruction budget under Run Settings.
            • If a run stops early, the Diagnosis panel says why in plain language.
            • For a known verification harness (lz4, xxhash, sqlite, lua and others), Verify against Host compares the output with the committed host reference.
            """)
        emptyText.font = .systemFont(ofSize: 13)
        emptyText.translatesAutoresizingMaskIntoConstraints = false
        emptyCard.contentView!.addSubview(emptyText)
        NSLayoutConstraint.activate([
            emptyText.leadingAnchor.constraint(equalTo: emptyCard.contentView!.leadingAnchor, constant: 16),
            emptyText.trailingAnchor.constraint(equalTo: emptyCard.contentView!.trailingAnchor, constant: -16),
            emptyText.topAnchor.constraint(equalTo: emptyCard.contentView!.topAnchor, constant: 12),
            emptyText.bottomAnchor.constraint(equalTo: emptyCard.contentView!.bottomAnchor, constant: -12)
        ])
        stack.addArrangedSubview(emptyCard)
        emptyCard.setAccessibilityLabel("Drop a program here to run it")

        openButton = button("Open Program…", #selector(chooseHomebrew))
        openButton.keyEquivalent = "\r"
        openButton.setAccessibilityHelp("Choose a program (.nro) on your Mac.")
        demoButton = button("Try the Demo", #selector(tryDemo))
        stopButton = button("Stop Session", #selector(stop))
        stopButton.isHidden = true
        verifyButton = button("Verify against Host…", #selector(chooseVerification))
        verifyButton.setAccessibilityHelp("Run a known verification program headless and compare its output with the committed host reference.")
        let actions = NSStackView(views: [openButton, verifyButton, demoButton, stopButton])
        actions.spacing = 12
        stack.addArrangedSubview(actions)

        // Run settings: engine and instruction budget (the runtime's OUNCE_NRO_BUDGET).
        let storedEngine = UserDefaults.standard.string(forKey: "engine").flatMap(Engine.init(rawValue:)) ?? .tier3
        let storedBudget = UserDefaults.standard.object(forKey: "instructionBudget") as? Int
        settings = RunSettings(engine: storedEngine, budget: storedBudget)
        enginePopUp = NSPopUpButton(frame: .zero, pullsDown: false)
        for engine in Engine.allCases { enginePopUp.addItem(withTitle: engine.title) }
        enginePopUp.selectItem(at: Engine.allCases.firstIndex(of: storedEngine) ?? 0)
        enginePopUp.target = self
        enginePopUp.action = #selector(settingsChanged)
        enginePopUp.setAccessibilityLabel("Engine")
        budgetField.placeholderString = "50,000,000 (default)"
        budgetField.stringValue = storedBudget.map(String.init) ?? ""
        budgetField.target = self
        budgetField.action = #selector(settingsChanged)
        budgetField.setAccessibilityLabel("Instruction budget")
        budgetField.widthAnchor.constraint(equalToConstant: 170).isActive = true
        let engineLabel = NSTextField(labelWithString: "Engine")
        let budgetLabel = NSTextField(labelWithString: "Instruction budget")
        let settingsRow = NSStackView(views: [engineLabel, enginePopUp, budgetLabel, budgetField])
        settingsRow.spacing = 8
        stack.addArrangedSubview(settingsRow)
        engineNote.font = .systemFont(ofSize: 12)
        engineNote.textColor = .secondaryLabelColor
        stack.addArrangedSubview(engineNote)
        engineNote.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        updateEngineNote()
        catalog = loadCatalog()
        let results = button("View Check Results…", #selector(chooseReport))
        results.setAccessibilityHelp("Open a saved Ounce-Verify report to read its recorded results.")
        stack.addArrangedSubview(results)

        let card = NSBox()
        card.boxType = .custom
        card.borderWidth = 1
        card.borderColor = .separatorColor
        card.cornerRadius = 12
        card.fillColor = .controlBackgroundColor
        card.contentViewMargins = NSSize(width: 0, height: 0)
        stack.addArrangedSubview(card)
        let session = NSStackView()
        session.orientation = .vertical
        session.alignment = .leading
        session.spacing = 5
        session.translatesAutoresizingMaskIntoConstraints = false
        card.contentView!.addSubview(session)
        NSLayoutConstraint.activate([
            session.leadingAnchor.constraint(equalTo: card.contentView!.leadingAnchor, constant: 16),
            session.trailingAnchor.constraint(equalTo: card.contentView!.trailingAnchor, constant: -16),
            session.topAnchor.constraint(equalTo: card.contentView!.topAnchor, constant: 12),
            session.bottomAnchor.constraint(equalTo: card.contentView!.bottomAnchor, constant: -12)
        ])
        status.font = .systemFont(ofSize: 16, weight: .semibold)
        session.addArrangedSubview(status)
        let help = NSTextField(wrappingLabelWithString: "To close a running app, press Esc in its window or choose Stop Session here.")
        help.font = .systemFont(ofSize: 13)
        help.textColor = .secondaryLabelColor
        session.addArrangedSubview(help)
        status.widthAnchor.constraint(equalTo: session.widthAnchor).isActive = true
        help.widthAnchor.constraint(equalTo: session.widthAnchor).isActive = true

        diagnosisCard.boxType = .custom
        diagnosisCard.borderWidth = 1
        diagnosisCard.borderColor = .separatorColor
        diagnosisCard.cornerRadius = 12
        diagnosisCard.fillColor = .controlBackgroundColor
        diagnosisCard.isHidden = true
        diagnosisCard.setAccessibilityLabel("Diagnosis")
        stack.addArrangedSubview(diagnosisCard)
        reportButton = button("Report This Problem…", #selector(reportProblem))
        reportButton.setAccessibilityHelp("Copy a report about this failed run and open the GitHub issue form in your browser. Nothing is sent until you submit the form.")
        reportButton.isHidden = true
        let diagnosisStack = NSStackView(views: [diagnosisHeadline, diagnosisBody, reportButton])
        diagnosisStack.orientation = .vertical
        diagnosisStack.alignment = .leading
        diagnosisStack.spacing = 6
        diagnosisStack.translatesAutoresizingMaskIntoConstraints = false
        diagnosisCard.contentView!.addSubview(diagnosisStack)
        NSLayoutConstraint.activate([
            diagnosisStack.leadingAnchor.constraint(equalTo: diagnosisCard.contentView!.leadingAnchor, constant: 16),
            diagnosisStack.trailingAnchor.constraint(equalTo: diagnosisCard.contentView!.trailingAnchor, constant: -16),
            diagnosisStack.topAnchor.constraint(equalTo: diagnosisCard.contentView!.topAnchor, constant: 12),
            diagnosisStack.bottomAnchor.constraint(equalTo: diagnosisCard.contentView!.bottomAnchor, constant: -12)
        ])
        diagnosisHeadline.font = .systemFont(ofSize: 15, weight: .semibold)
        diagnosisBody.font = .systemFont(ofSize: 13)
        diagnosisBody.isSelectable = true
        diagnosisHeadline.widthAnchor.constraint(equalTo: diagnosisStack.widthAnchor).isActive = true
        diagnosisBody.widthAnchor.constraint(equalTo: diagnosisStack.widthAnchor).isActive = true

        detailsButton = button("Show Session Details", #selector(toggleDetails))
        stack.addArrangedSubview(detailsButton)
        details.orientation = .vertical
        details.alignment = .leading
        details.spacing = 8
        details.isHidden = true
        stack.addArrangedSubview(details)

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        console.isEditable = false
        console.isSelectable = true
        console.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        console.setAccessibilityLabel("Session log")
        console.isVerticallyResizable = true
        console.autoresizingMask = [.width]
        console.textContainer?.widthTracksTextView = true
        scroll.documentView = console
        details.addArrangedSubview(scroll)
        scroll.widthAnchor.constraint(equalTo: details.widthAnchor).isActive = true
        scroll.heightAnchor.constraint(equalToConstant: 120).isActive = true
        folderButton = button("Choose SD Card Folder…", #selector(chooseFolder))
        let logs = button("Open Log in Finder", #selector(showLogs))
        let files = NSStackView(views: [folderButton, logs])
        files.spacing = 10
        details.addArrangedSubview(files)
        folderLabel.font = .systemFont(ofSize: 12)
        folderLabel.textColor = .secondaryLabelColor
        folderLabel.lineBreakMode = .byTruncatingMiddle
        updateFolderLabel()
        details.addArrangedSubview(folderLabel)
        folderLabel.widthAnchor.constraint(equalTo: details.widthAnchor).isActive = true
        for view in [title, intro, emptyCard!, card, diagnosisCard, details] {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        window.initialFirstResponder = openButton
        window.center()
        // The workbench is the front door. The classic launcher window stays available from the menu.
        let runtime = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/Ounce Runtime.app/Contents/MacOS/ounce-core")
        workbench = Workbench(runtime: runtime)
        workbench.show()
        NSApp.activate(ignoringOtherApps: true)
        // Poll one bounded snapshot instead of queueing an unbounded number of UI updates.
        Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in self?.refreshConsole() }
        if let file = pendingFile { pendingFile = nil; workbench.check(file: file) }
    }

    private func button(_ title: String, _ action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.bezelStyle = .rounded
        button.controlSize = .large
        return button
    }

    @objc private func checkProgram() {
        let panel = NSOpenPanel()
        panel.message = "Choose a homebrew program (.nro) to check"
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url { workbench.check(file: url) }
    }

    @objc private func showClassic() {
        window.makeKeyAndOrderFront(nil)
    }

    /// The classic window is not shown at launch (the workbench is the front door), so every classic action brings it
    /// forward first. Otherwise its status, diagnosis and error sheets would update a window that is not on screen.
    private func revealClassic() {
        guard !window.isVisible else { return }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func chooseHomebrew() {
        guard process == nil, window.attachedSheet == nil else { return }
        revealClassic()
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "nro") ?? .data]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a program file ending in .nro."
        panel.prompt = "Open Program"
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url { self.openProgram(url) }
        }
    }

    @objc private func chooseReport() {
        guard window.attachedSheet == nil else { return }
        revealClassic()
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a saved Ounce-Verify results, diagnosis, repair or run report. Opening it does not run an app or apply a repair."
        panel.prompt = "View Results"
        panel.beginSheetModal(for: window) { response in
            guard response == .OK, let url = panel.url else { return }
            do { self.showReport(try EvidenceReport.read(url), from: url) }
            catch { self.showError(error.localizedDescription) }
        }
    }

    private func showReport(_ report: EvidenceReport, from url: URL) {
        let viewer = reportWindow ?? NSWindow(contentRect: NSRect(x: 0, y: 0, width: 760, height: 560),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        reportWindow = viewer
        viewer.isReleasedWhenClosed = false
        viewer.title = "Check results — \(url.lastPathComponent)"
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

    /// Reads the first 20 bytes and checks for the NRO magic. Returns a user-facing problem, or nil if it looks like an NRO.
    private func nroProblem(_ url: URL) -> String? {
        do {
            let file = try FileHandle(forReadingFrom: url)
            defer { try? file.close() }
            let header = try file.read(upToCount: 20) ?? Data()
            if header.starts(with: Data([0x7F, 0x45, 0x4C, 0x46])) {
                return "This file is an ELF binary, not a Switch homebrew .nro. Run static AArch64 Linux ELFs from Terminal with: ounce-core run-elf \(url.path)"
            }
            guard header.count == 20, header.suffix(4) == Data("NRO0".utf8) else {
                return "This file isn’t a valid Switch homebrew .nro program (missing 'NRO0' header at offset 0x10). Choose another .nro file."
            }
            return nil
        } catch {
            return "Couldn’t read this file. Check that it’s on your Mac and try again."
        }
    }

    private func openProgram(_ url: URL) {
        switch url.pathExtension.lowercased() {
        case "nro": break
        case "elf":
            showError("The Mac app runs Switch homebrew .nro files. Run static AArch64 Linux ELF programs from Terminal with: ounce-core run-elf \(url.path)")
            return
        default:
            showError("Choose a program ending in .nro.")
            return
        }
        guard process == nil else {
            showError("Stop the current session before opening another program.")
            return
        }
        if let problem = nroProblem(url) { status.stringValue = problem; showError(problem); return }
        guard let current = currentSettings() else { return }
        lastProgram = url
        verifying = nil
        start(current.arguments(nro: url.path, sdCard: sdCard.path), name: url.deletingPathExtension().lastPathComponent,
              waitsForGuestFrame: !current.engine.isHeadlessOnly, settings: current)
    }

    @objc private func tryDemo() {
        var demo = settings
        demo.engine = .tier3 // The built-in demo is a graphics demo for the interpreter path.
        verifying = nil
        lastProgram = nil // A failed demo is not a report about the last program that was opened.
        start(["gfx", "--present=gpu"], name: "graphics demo", settings: demo)
    }

    // MARK: Run settings

    /// Validates the budget field. Shows an alert and returns nil on bad input so nothing runs with a surprise value.
    private func currentSettings() -> RunSettings? {
        switch RunSettings.parseBudget(budgetField.stringValue) {
        case .success(let budget):
            settings.budget = budget
            let index = max(enginePopUp.indexOfSelectedItem, 0)
            settings.engine = Engine.allCases[index]
            UserDefaults.standard.set(settings.engine.rawValue, forKey: "engine")
            if let budget { UserDefaults.standard.set(budget, forKey: "instructionBudget") }
            else { UserDefaults.standard.removeObject(forKey: "instructionBudget") }
            return settings
        case .failure(let error):
            showError(error.localizedDescription)
            return nil
        }
    }

    @objc private func settingsChanged() {
        settings.engine = Engine.allCases[max(enginePopUp.indexOfSelectedItem, 0)]
        updateEngineNote()
    }

    private func updateEngineNote() {
        var note = settings.engine.summary
        if settings.engine.isHeadlessOnly { note += " It ignores the instruction budget and uses a time limit." }
        engineNote.stringValue = note
    }

    // MARK: Verify against host

    private func loadCatalog() -> [Harness] {
        guard let url = Bundle.main.url(forResource: "differential", withExtension: "json", subdirectory: "Verify"),
              let data = try? Data(contentsOf: url) else { return [] }
        return (try? HarnessCatalog.parse(data)) ?? []
    }

    @objc private func chooseVerification() {
        guard process == nil, window.attachedSheet == nil else { return }
        revealClassic()
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "nro") ?? .data]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a verification harness program, named like the harness it was built from (for example lz4.nro)."
        panel.prompt = "Verify"
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url { self.verify(url) }
        }
    }

    private func verify(_ url: URL) {
        guard process == nil else { showError("Stop the current session first."); return }
        if let problem = nroProblem(url) { status.stringValue = problem; showError(problem); return }
        guard !catalog.isEmpty else {
            showError("The bundled verification data is missing. Rebuild Ounce Core.app with scripts/build_macos_app.sh.")
            return
        }
        guard let harness = HarnessCatalog.match(nroFile: url.lastPathComponent, in: catalog) else {
            showDiagnosisText("Not a known verification harness",
                "Verify against Host only works for programs built from the harnesses recorded in docs/verify, matched by file name: \(catalog.map(\.name).joined(separator: ", ")). Use Open Program… to run any other program and read its diagnosis.")
            return
        }
        guard FileManager.default.fileExists(atPath: expectedHostURL(harness).path) else {
            showError("The committed host reference for \(harness.name) is missing from this app bundle.")
            return
        }
        guard var current = currentSettings() else { return }
        // verify_differential.py uses a large budget for harnesses; an explicit user budget still wins.
        if current.budget == nil { current.budget = RunSettings.verifyBudget }
        do {
            let sd = FileManager.default.temporaryDirectory.appendingPathComponent("ounce-verify-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: sd, withIntermediateDirectories: true)
            verifying = (harness, sd, current.engine)
            lastProgram = url
            start(["run-nro", url.path, "--present=headless", "--sdmc=\(sd.path)", "--cpu=\(current.engine.rawValue)"],
                  name: "verification of \(harness.name)", settings: current, extraEnvironment: ["OUNCE_NRO_TIMEOUT_SECS": "300"])
        } catch { showError(error.localizedDescription) }
    }

    private func expectedHostURL(_ harness: Harness) -> URL {
        Bundle.main.resourceURL!.appendingPathComponent("Verify/\(harness.dir)/expected-host.txt")
    }

    /// Compares the harness output with the bundled host reference and shows the verdict. Only a byte-for-byte comparison.
    private func finishVerification(_ run: (harness: Harness, sd: URL, engine: Engine), diagnosis: Diagnosis, log: String) {
        defer { try? FileManager.default.removeItem(at: run.sd) }
        let title = "Verify against host: \(run.harness.name) on \(run.engine.rawValue)"
        func showFailure(_ headline: String, _ summary: String) {
            showDiagnosisText(headline, summary)
            prepareReport(diagnosis, comparisonSummary: headline + "\n" + summary)
        }
        if diagnosis.kind == .engineUnavailable {
            showFailure(title + " — not run on that engine", diagnosis.text + "\n\nNo comparison was made, because it would not describe the engine you chose.")
            return
        }
        guard diagnosis.succeeded else {
            showFailure(title + " — the run did not finish", diagnosis.text)
            return
        }
        let produced = run.sd.appendingPathComponent(run.harness.resultFile)
        guard let actual = try? String(contentsOf: produced, encoding: .utf8) else {
            showFailure(title + " — no output", "The program ran but did not write \(run.harness.resultFile) to the SD card, so there is nothing to compare. Check that the NRO was built from the matching harness.")
            return
        }
        guard let expected = try? String(contentsOf: expectedHostURL(run.harness), encoding: .utf8) else {
            showFailure(title + " — reference error", "The committed host reference could not be read; no comparison was made.")
            return
        }
        let comparison = ResultComparison.compare(expected: expected, actual: actual)
        #if arch(arm64)
        let hostIsARM64 = true
        #else
        let hostIsARM64 = false
        #endif
        let summary = comparison.summary(harness: run.harness, engine: run.engine, hostIsARM64: hostIsARM64)
        if comparison.identical { showDiagnosisText(title + " — match", summary) }
        else { showFailure(title + " — mismatch", summary) }
    }

    // MARK: Diagnosis panel

    private func showDiagnosisText(_ headline: String, _ body: String) {
        revealClassic()
        emptyCard.isHidden = true
        reportButton.isHidden = true
        pendingReport = nil
        diagnosisHeadline.stringValue = headline
        diagnosisBody.stringValue = body
        diagnosisCard.isHidden = false
    }

    private func showDiagnosis(_ diagnosis: Diagnosis) {
        showDiagnosisText(diagnosis.headline, diagnosis.text.replacingOccurrences(of: diagnosis.headline + "\n\n", with: "", options: .anchored))
        // A failed run ends with an action: one click copies a report and opens the issue form.
        guard !diagnosis.succeeded else { return }
        prepareReport(diagnosis)
    }

    private func prepareReport(_ diagnosis: Diagnosis, comparisonSummary: String? = nil) {
        guard let program = lastProgram, let engine = lastEngine else { return }
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
        pendingReport = IssueReport(program: program.lastPathComponent, engine: engine, diagnosis: diagnosis, appVersion: version,
                                    hostOS: ProcessInfo.processInfo.operatingSystemVersionString, home: NSHomeDirectory(), comparisonSummary: comparisonSummary)
        reportButton.isHidden = false
    }

    @objc private func reportProblem() {
        guard let report = pendingReport else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(report.body, forType: .string)
        if let url = report.url { NSWorkspace.shared.open(url) }
        status.stringValue = "Report text copied. Review the issue form in your browser before submitting; add the expected result."
    }

    private func start(_ arguments: [String], name: String, waitsForGuestFrame: Bool = false,
                       settings run: RunSettings, extraEnvironment: [String: String] = [:]) {
        guard process == nil else { showError("Stop the current session before opening another program."); return }
        let runtime = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/Ounce Runtime.app/Contents/MacOS/ounce-core")
        guard FileManager.default.isExecutableFile(atPath: runtime.path) else {
            showError("The bundled runtime is missing. Rebuild Ounce Core.app.")
            return
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
            consoleBytes.removeAll()
            console.string = ""
            diagnosisCard.isHidden = true
            pipe.fileHandleForReading.readabilityHandler = { handle in
                let bytes = handle.availableData
                if bytes.isEmpty { handle.readabilityHandler = nil; log.close(); return }
                log.append(bytes)
            }
            child.terminationHandler = { [weak self] finished in
                // Give the pipe a moment to drain so the last log lines are in the diagnosis.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                    guard let self, self.sessionID == id else { return }
                    self.refreshConsole()
                    self.awaitingGuestFrame = nil
                    self.process = nil
                    self.setBusy(false)
                    let text = String(decoding: log.snapshot(), as: UTF8.self)
                    let diagnosis = Diagnosis.analyze(log: text, exitStatus: finished.terminationStatus, engine: run.engine, budget: run.budget)
                    if self.stopRequested {
                        self.status.stringValue = "Session stopped."
                        self.verifying.map { try? FileManager.default.removeItem(at: $0.sd) }
                        self.verifying = nil
                    } else if let verification = self.verifying {
                        self.verifying = nil
                        self.status.stringValue = diagnosis.succeeded ? "Verification run finished." : "Verification run did not finish."
                        self.finishVerification(verification, diagnosis: diagnosis, log: text)
                    } else if finished.terminationStatus != 0 {
                        self.status.stringValue = "Couldn’t finish running \(name). See the diagnosis below."
                        self.showDiagnosis(diagnosis)
                        self.setDetailsVisible(true)
                    } else {
                        self.status.stringValue = "Session finished."
                        self.showDiagnosis(diagnosis)
                    }
                }
            }
            try child.run()
            process = child
            setBusy(true)
            awaitingGuestFrame = waitsForGuestFrame ? name : nil
            status.stringValue = verifying != nil
                ? "Running \(name) headless. This can take a while for large harnesses."
                : waitsForGuestFrame
                ? "Starting \(name)… Waiting for the first guest frame."
                : "Session active: \(name)"
        } catch { showError(error.localizedDescription) }
    }

    private func refreshConsole() {
        guard let bytes = currentLog?.snapshot(), bytes != consoleBytes else { return }
        consoleBytes = bytes
        let output = String(decoding: consoleBytes, as: UTF8.self)
        console.string = output
        console.scrollToEndOfDocument(nil)
        if let name = awaitingGuestFrame, output.contains("\n[Display] Guest VI frame: ") {
            awaitingGuestFrame = nil
            if !stopRequested { status.stringValue = "Showing \(name) in a separate window" }
        }
    }

    private func setBusy(_ busy: Bool) {
        openButton.isEnabled = !busy
        demoButton.isEnabled = !busy
        verifyButton.isEnabled = !busy
        enginePopUp.isEnabled = !busy
        budgetField.isEnabled = !busy
        folderButton.isEnabled = !busy
        openMenuItem.isEnabled = !busy
        stopButton.isHidden = !busy
        stopButton.isEnabled = busy
    }

    @objc private func toggleDetails() { setDetailsVisible(details.isHidden) }

    private func setDetailsVisible(_ visible: Bool) {
        details.isHidden = !visible
        detailsButton.title = visible ? "Hide Session Details" : "Show Session Details"
        let minimumHeight: CGFloat = visible ? 780 : 600
        window.contentMinSize = NSSize(width: 640, height: minimumHeight)
        if let size = window.contentView?.bounds.size, size.height < minimumHeight {
            window.setContentSize(NSSize(width: size.width, height: minimumHeight))
        }
    }

    @objc private func stop() {
        guard let process, process.isRunning else { return }
        stopRequested = true
        currentLog?.append(Data("\n[Launcher] Session stopped by the user.\n".utf8)) // So a saved log shows why it ends here.
        process.terminate()
        stopButton.isEnabled = false
        status.stringValue = "Stopping…"
    }

    @objc private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.directoryURL = sdCard
        panel.prompt = "Use as SD Card"
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url {
                UserDefaults.standard.set(url.path, forKey: "sdCardFolder")
                self.updateFolderLabel()
            }
        }
    }

    private func updateFolderLabel() {
        folderLabel.stringValue = "SD card: \(sdCard.path)"
        folderLabel.toolTip = sdCard.path
    }

    @objc private func showLogs() {
        if let lastLog { NSWorkspace.shared.activateFileViewerSelecting([lastLog]) }
        else {
            let directory = support.appendingPathComponent("Logs")
            do { try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true); NSWorkspace.shared.open(directory) }
            catch { showError(error.localizedDescription) }
        }
    }

    private func showError(_ message: String) {
        revealClassic()
        let alert = NSAlert()
        alert.messageText = "Ounce Core"
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.beginSheetModal(for: window)
    }

    func application(_ sender: NSApplication, openFile filename: String) -> Bool {
        let url = URL(fileURLWithPath: filename)
        guard url.pathExtension.lowercased() == "nro" else { return false }
        if workbench == nil { pendingFile = url } else { workbench.check(file: url) }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { process?.terminate(); workbench?.stop() }
}

let application = NSApplication.shared
let launcher = Launcher()
application.delegate = launcher
application.run()
