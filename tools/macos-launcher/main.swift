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
    private var status = NSTextField(labelWithString: "Ready. Open a homebrew app, or try the demo.")
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

        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 720, height: 540),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Ounce Core"
        window.minSize = NSSize(width: 620, height: 500)
        window.delegate = self
        window.isReleasedWhenClosed = false
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 14
        stack.translatesAutoresizingMaskIntoConstraints = false
        window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: window.contentView!.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: window.contentView!.trailingAnchor, constant: -24),
            stack.topAnchor.constraint(equalTo: window.contentView!.topAnchor, constant: 24),
            stack.bottomAnchor.constraint(equalTo: window.contentView!.bottomAnchor, constant: -24)
        ])
        let title = NSTextField(labelWithString: "Open homebrew on your Mac")
        title.font = .systemFont(ofSize: 25, weight: .semibold)
        stack.addArrangedSubview(title)
        let subtitle = NSTextField(wrappingLabelWithString: "Choose an open-source .nro app, or start with the built-in interactive demo.")
        subtitle.textColor = .secondaryLabelColor
        stack.addArrangedSubview(subtitle)
        openButton = button("Open Homebrew…", #selector(chooseHomebrew))
        openButton.keyEquivalent = "\r"
        demoButton = button("Graphics Demo", #selector(tryDemo))
        stopButton = button("Stop", #selector(stop))
        stopButton.isEnabled = false
        let actions = NSStackView(views: [openButton, demoButton, stopButton])
        actions.spacing = 10
        stack.addArrangedSubview(actions)
        status.font = .systemFont(ofSize: 13, weight: .medium)
        status.lineBreakMode = .byTruncatingMiddle
        stack.addArrangedSubview(status)
        let controls = NSTextField(wrappingLabelWithString: "Open a homebrew .nro, or try the graphics demo. Press Esc to close the session.")
        controls.font = .systemFont(ofSize: 12)
        controls.textColor = .secondaryLabelColor
        stack.addArrangedSubview(controls)
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        console.isEditable = false
        console.isSelectable = true
        console.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        console.isVerticallyResizable = true
        console.autoresizingMask = [.width]
        console.textContainer?.widthTracksTextView = true
        scroll.documentView = console
        stack.addArrangedSubview(scroll)
        scroll.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 200).isActive = true
        folderButton = button("SD Card Folder…", #selector(chooseFolder))
        let logs = button("Show Logs", #selector(showLogs))
        let files = NSStackView(views: [folderButton, logs])
        files.spacing = 10
        stack.addArrangedSubview(files)
        folderLabel.font = .systemFont(ofSize: 11)
        folderLabel.textColor = .secondaryLabelColor
        folderLabel.lineBreakMode = .byTruncatingMiddle
        updateFolderLabel()
        stack.addArrangedSubview(folderLabel)
        let preview = NSTextField(labelWithString: "Development preview · Compatibility varies by app · Closing this launcher stops the session")
        preview.font = .systemFont(ofSize: 10)
        preview.textColor = .secondaryLabelColor
        stack.addArrangedSubview(preview)
        for view in [subtitle, status, controls, folderLabel, preview] {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
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
        return button
    }

    @objc private func chooseHomebrew() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "nro") ?? .data]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.prompt = "Open Homebrew"
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url { self.openHomebrew(url) }
        }
    }

    private func openHomebrew(_ url: URL) {
        guard url.pathExtension.lowercased() == "nro" else {
            showError("Choose a homebrew .nro file.")
            return
        }
        start(["run", url.path, "--gui", "--present=gpu", "--sdmc=\(sdCard.path)"], name: url.deletingPathExtension().lastPathComponent, waitsForGuestFrame: true)
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
                        self.status.stringValue = "Session failed (status \(finished.terminationStatus)). See the log below."
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
            if !stopRequested { status.stringValue = "\(name) · First guest frame received" }
        }
    }

    private func setBusy(_ busy: Bool) {
        openButton.isEnabled = !busy
        demoButton.isEnabled = !busy
        folderButton.isEnabled = !busy
        stopButton.isEnabled = busy
    }

    @objc private func stop() {
        stopRequested = true
        process?.terminate()
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
