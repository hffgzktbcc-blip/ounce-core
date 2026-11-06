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
        openMenuItem = applicationMenu.addItem(withTitle: "Open App…", action: #selector(chooseHomebrew), keyEquivalent: "o")
        openMenuItem.target = self
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

        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 425),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Ounce Core"
        window.contentMinSize = NSSize(width: 600, height: 425)
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
        let title = NSTextField(labelWithString: "Play homebrew")
        title.font = .systemFont(ofSize: 32, weight: .bold)
        heading.addArrangedSubview(title)
        stack.addArrangedSubview(heading)

        let intro = NSTextField(wrappingLabelWithString: "Open a homebrew app file (.nro) from your Mac. If you don’t have one yet, try the built-in demo.")
        intro.font = .systemFont(ofSize: 15)
        intro.textColor = .secondaryLabelColor
        stack.addArrangedSubview(intro)

        openButton = button("Open App…", #selector(chooseHomebrew))
        openButton.keyEquivalent = "\r"
        openButton.setAccessibilityHelp("Choose a homebrew .nro file on your Mac.")
        demoButton = button("Try the Demo", #selector(tryDemo))
        stopButton = button("Stop Session", #selector(stop))
        stopButton.isHidden = true
        let actions = NSStackView(views: [openButton, demoButton, stopButton])
        actions.spacing = 12
        stack.addArrangedSubview(actions)
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
        for view in [intro, card, details] {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        window.initialFirstResponder = openButton
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        // Poll one bounded snapshot instead of queueing an unbounded number of UI updates.
        Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in self?.refreshConsole() }
        if let file = pendingFile { pendingFile = nil; openHomebrew(file) }
    }

    private func button(_ title: String, _ action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.bezelStyle = .rounded
        button.controlSize = .large
        return button
    }

    @objc private func chooseHomebrew() {
        guard process == nil, window.attachedSheet == nil else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "nro") ?? .data]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a homebrew app file ending in .nro."
        panel.prompt = "Open App"
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url { self.openHomebrew(url) }
        }
    }

    @objc private func chooseReport() {
        guard window.attachedSheet == nil else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a saved Ounce-Verify corpus report. Opening it does not run an app or apply a repair."
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
        label("Saved check results", bold: true)
        label("These results describe an earlier run. Opening this file does not verify it again or approve a repair.")
        label("Pass: recorded checks passed. Failed: an observed mismatch. Blocked: checks could not finish. Not assessed: more evidence is needed.")
        label("\(report.records.count) recorded results • \(url.lastPathComponent)")
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

    private func openHomebrew(_ url: URL) {
        guard url.pathExtension.lowercased() == "nro" else {
            showError("Choose a homebrew .nro file.")
            return
        }
        guard process == nil else {
            showError("Stop the current session before opening another app.")
            return
        }
        do {
            let file = try FileHandle(forReadingFrom: url)
            defer { try? file.close() }
            let header = try file.read(upToCount: 20) ?? Data()
            guard header.count == 20, header.suffix(4) == Data("NRO0".utf8) else {
                status.stringValue = "This file isn’t a homebrew app. Choose another .nro file."
                return
            }
        } catch {
            status.stringValue = "Couldn’t read this file. Check that it’s on your Mac and try again."
            return
        }
        start(["run-nro", url.path, "--gui", "--present=gpu", "--sdmc=\(sdCard.path)"], name: url.deletingPathExtension().lastPathComponent, waitsForGuestFrame: true)
    }

    @objc private func tryDemo() { start(["gfx", "--present=gpu"], name: "graphics demo") }

    private func start(_ arguments: [String], name: String, waitsForGuestFrame: Bool = false) {
        guard process == nil else { showError("Stop the current session before opening another app."); return }
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
            let child = Process()
            child.executableURL = runtime
            child.arguments = arguments
            child.currentDirectoryURL = support
            var environment = ProcessInfo.processInfo.environment
            environment.removeValue(forKey: "OUNCE_CPU") // The GUI path must not select headless native execution.
            environment["RUST_BACKTRACE"] = "1"
            child.environment = environment
            child.standardInput = FileHandle.nullDevice
            let pipe = Pipe()
            child.standardOutput = pipe
            child.standardError = pipe
            let id = UUID()
            sessionID = id
            consoleBytes.removeAll()
            console.string = ""
            pipe.fileHandleForReading.readabilityHandler = { handle in
                let bytes = handle.availableData
                if bytes.isEmpty { handle.readabilityHandler = nil; log.close(); return }
                log.append(bytes)
            }
            child.terminationHandler = { [weak self] finished in
                DispatchQueue.main.async {
                    guard let self, self.sessionID == id else { return }
                    self.refreshConsole()
                    self.awaitingGuestFrame = nil
                    self.process = nil
                    self.setBusy(false)
                    if self.stopRequested {
                        self.status.stringValue = "Session stopped."
                    } else if finished.terminationStatus != 0 {
                        self.status.stringValue = waitsForGuestFrame
                            ? "Couldn’t finish running \(name). Check the session details or try another app."
                            : "The demo stopped unexpectedly. Check the session details."
                        self.setDetailsVisible(true)
                    } else {
                        self.status.stringValue = "Session finished."
                    }
                }
            }
            try child.run()
            process = child
            setBusy(true)
            awaitingGuestFrame = waitsForGuestFrame ? name : nil
            status.stringValue = waitsForGuestFrame
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
        folderButton.isEnabled = !busy
        openMenuItem.isEnabled = !busy
        stopButton.isHidden = !busy
        stopButton.isEnabled = busy
    }

    @objc private func toggleDetails() { setDetailsVisible(details.isHidden) }

    private func setDetailsVisible(_ visible: Bool) {
        details.isHidden = !visible
        detailsButton.title = visible ? "Hide Session Details" : "Show Session Details"
        let minimumHeight: CGFloat = visible ? 605 : 425
        window.contentMinSize = NSSize(width: 600, height: minimumHeight)
        if let size = window.contentView?.bounds.size, size.height < minimumHeight {
            window.setContentSize(NSSize(width: size.width, height: minimumHeight))
        }
    }

    @objc private func stop() {
        guard let process, process.isRunning else { return }
        stopRequested = true
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
        let alert = NSAlert()
        alert.messageText = "Ounce Core"
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.beginSheetModal(for: window)
    }

    func application(_ sender: NSApplication, openFile filename: String) -> Bool {
        let url = URL(fileURLWithPath: filename)
        guard url.pathExtension.lowercased() == "nro" else { return false }
        if window == nil { pendingFile = url } else { openHomebrew(url) }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { process?.terminate() }
}

let application = NSApplication.shared
let launcher = Launcher()
application.delegate = launcher
application.run()
