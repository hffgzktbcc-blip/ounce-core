import AppKit
import UniformTypeIdentifiers
import WebKit

/// The Ounce-Core workbench in a window: starts `ounce-core ui` from the bundled runtime and shows its page.
/// The window holds no logic of its own. The server is a child process (so `forbid(unsafe_code)` is untouched), it
/// listens on 127.0.0.1 only, and it exits when this app does (the stdin pipe below closes).
/// The page itself, plus a drop target for a program anywhere in the window. It only reports the file.
final class WorkbenchWebView: WKWebView {
    var onDrop: ((URL) -> Void)?

    private func file(_ info: NSDraggingInfo) -> URL? {
        (info.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL])?.first
    }
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation { file(sender) == nil ? super.draggingEntered(sender) : .copy }
    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation { file(sender) == nil ? super.draggingUpdated(sender) : .copy }
    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool { file(sender) != nil ? true : super.prepareForDragOperation(sender) }
    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        guard let url = file(sender) else { return super.performDragOperation(sender) }
        onDrop?(url)
        return true
    }
}

/// Dialogs and file panels are sheets on the window, never `runModal()`: a modal loop does not take Apple events, so
/// Quit from the Dock, a script or the system would be ignored while one was open.
enum Sheet {
    static func show(_ alert: NSAlert, on window: NSWindow?, _ done: @escaping (NSApplication.ModalResponse) -> Void) {
        if let window, window.isVisible { alert.beginSheetModal(for: window, completionHandler: done) } else { done(alert.runModal()) }
    }
    static func show(_ panel: NSSavePanel, on window: NSWindow?, _ done: @escaping (NSApplication.ModalResponse) -> Void) {
        if let window, window.isVisible { panel.beginSheetModal(for: window, completionHandler: done) } else { done(panel.runModal()) }
    }
}

/// One plain way to tell the person something went wrong: what happened, then what to do.
enum Notice {
    static func show(_ what: String, _ next: String, on window: NSWindow?) {
        let alert = NSAlert()
        alert.messageText = what
        alert.informativeText = next
        alert.alertStyle = .warning
        alert.addButton(withTitle: "OK")
        Sheet.show(alert, on: window) { _ in }
    }
}

/// Lets the page talk to the app without the app holding a strong reference to it.
private final class ScriptBridge: NSObject, WKScriptMessageHandler {
    weak var target: WKScriptMessageHandler?
    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.userContentController(controller, didReceive: message)
    }
}

final class Workbench: NSObject, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler {
    private let runtime: URL
    private var process: Process?
    private var keepAlive: Pipe?
    private var address: WorkbenchAddress?
    private var output = Data()
    private var buffer = ""
    private var pendingFiles: [URL] = []
    private var stopping = false
    private var acceptedThisMac = false
    private(set) var lastProgram: URL?
    private(set) var window: NSWindow!
    private var webView: WKWebView!

    init(runtime: URL) { self.runtime = runtime }

    func show() {
        let configuration = WKWebViewConfiguration()
        let bridge = ScriptBridge()
        bridge.target = self
        configuration.userContentController.add(bridge, name: "ounceWorkbench")
        let view = WorkbenchWebView(frame: .zero, configuration: configuration)
        view.onDrop = { [weak self] url in self?.check(file: url) }
        webView = view
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.setAccessibilityLabel("Ounce-Core workbench")
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 760),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Ounce Core"
        window.tabbingMode = .disallowed // no "Show Tab Bar" items in the View menu
        window.minSize = NSSize(width: 720, height: 480)
        window.contentView = webView
        window.setFrameAutosaveName("OunceCoreWorkbench")
        if !window.setFrameUsingName("OunceCoreWorkbench") { window.center() }
        window.makeKeyAndOrderFront(nil)
        message("Starting Ounce Core…")
        start()
    }

    func stop() {
        stopping = true
        process?.terminate()
    }

    /// Hands a program to the workbench (waits until the server is up).
    func check(file: URL) {
        if let problem = WorkbenchLogic.programProblem(file) { window?.makeKeyAndOrderFront(nil); return fail(problem.what, problem.next) }
        guard let address else { pendingFiles.append(file); return }
        guard let data = try? Data(contentsOf: file) else {
            return fail("Ounce Core couldn’t read \(file.lastPathComponent).", "Check that the file is still there and that you can open it, then try again.")
        }
        NSDocumentController.shared.noteNewRecentDocumentURL(file)
        lastProgram = file
        // Use the settings currently shown in the page; an older page or a failed call falls back to emulator-only.
        webView.evaluateJavaScript("OunceWorkbench.queryString()") { [weak self] value, _ in
            guard let self else { return }
            let options = WorkbenchLogic.options(WorkbenchLogic.checkOptions(fromPage: value), for: WorkbenchLogic.programKind(header: data.prefix(20)))
            if WorkbenchLogic.runsOnThisMac(options) && !self.acceptedThisMac {
                let alert = NSAlert()
                alert.messageText = "Run this program on this Mac?"
                alert.informativeText = "This runs \(file.lastPathComponent) directly on this Mac, outside the emulator’s protection. Only do this for programs you trust."
                alert.addButton(withTitle: "Run on This Mac")
                alert.addButton(withTitle: "Cancel")
                Sheet.show(alert, on: self.window) { response in
                    guard response == .alertFirstButtonReturn else { return }
                    self.acceptedThisMac = true
                    self.upload(file: file, data: data, options: options)
                }
                return
            }
            self.upload(file: file, data: data, options: options)
        }
        window.makeKeyAndOrderFront(nil)
    }

    private func upload(file: URL, data: Data, options: String) {
        guard let address else { return }
        let request = WorkbenchLogic.uploadRequest(address, name: file.lastPathComponent, body: data, options: options)
        URLSession.shared.dataTask(with: request) { [weak self] data, _, error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error { return self.fail("Ounce Core couldn’t reach its checker.", "\(error.localizedDescription) Quit and reopen Ounce Core, then try again.") }
                switch WorkbenchLogic.parseUploadResponse(data ?? Data()) {
                case .success(let id):
                    // stage(id) lets "Check again" reuse the program the server now holds; pick(id) shows the check.
                    self.webView.evaluateJavaScript("OunceWorkbench.stage('\(id)'); OunceWorkbench.pick('\(id)')")
                case .failure(let problem): self.fail("Ounce Core didn’t accept \(file.lastPathComponent).", problem.message)
                }
            }
        }.resume()
    }

    func zoom(by factor: CGFloat) { webView.pageZoom = min(3, max(0.5, webView.pageZoom * factor)) }
    func zoom(to value: CGFloat) { webView.pageZoom = value }

    func reveal() { window?.makeKeyAndOrderFront(nil) }

    /// Opens the same prefilled issue form as the page's Report This Problem button (engine, hash, headline), for the
    /// check on screen or the newest finished one. Without a finished check it opens the plain form. Nothing is sent
    /// until the person submits the form.
    func reportProblem() {
        let home = NSHomeDirectory()
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
        let fallback = {
            guard let url = WorkbenchLogic.problemReportURL(appVersion: version, hostOS: ProcessInfo.processInfo.operatingSystemVersionString) else { return }
            NSWorkspace.shared.open(url)
        }
        guard address != nil else { return fallback() }
        webView.evaluateJavaScript("JSON.stringify(OunceWorkbench.reportRequest())") { value, _ in
            if let url = WorkbenchLogic.pageReport(value, home: home) { NSWorkspace.shared.open(url) } else { fallback() }
        }
    }

    private func start() {
        let process = Process()
        process.executableURL = runtime
        process.arguments = WorkbenchLogic.serverArguments(verifyDir: Bundle.main.resourceURL?.appendingPathComponent("Verify", isDirectory: true).standardizedFileURL
            .nilIfMissing)
        process.environment = WorkbenchLogic.serverEnvironment(base: ProcessInfo.processInfo.environment)
        let pipe = Pipe(), stdin = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        process.standardInput = stdin
        keepAlive = stdin
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            DispatchQueue.main.async { self?.consume(data) }
        }
        process.terminationHandler = { [weak self] _ in
            DispatchQueue.main.async {
                guard let self, !self.stopping else { return }
                self.fail("The checker stopped unexpectedly.", "Quit and reopen Ounce Core. If it happens again, choose Help > Report a Problem and include this:\n\n" + self.tail())
            }
        }
        do { try process.run() } catch { return fail("Ounce Core couldn’t start its checker.", "The bundled runtime could not be launched (\(error.localizedDescription)). Reinstall Ounce Core, then open it again.") }
        self.process = process
        DispatchQueue.main.asyncAfter(deadline: .now() + 15) { [weak self] in
            if let self, self.address == nil, !self.stopping { self.fail("Ounce Core didn’t start its checker.", "Quit and reopen Ounce Core. If it happens again, choose Help > Report a Problem and include this:\n\n" + self.tail()) }
        }
    }

    private func consume(_ data: Data) {
        output.append(data)
        if output.count > 64 * 1024 { output = output.suffix(32 * 1024) }
        buffer += String(decoding: data, as: UTF8.self)
        guard address == nil else { buffer = ""; return }
        while let newline = buffer.firstIndex(of: "\n") {
            let line = String(buffer[..<newline])
            buffer.removeSubrange(...newline)
            if let found = WorkbenchLogic.parseAddress(line) {
                address = found
                buffer = ""
                webView.load(URLRequest(url: found.url))
                let files = pendingFiles
                pendingFiles = []
                files.forEach { check(file: $0) }
                return
            }
        }
    }

    private func tail() -> String {
        let text = String(decoding: output, as: UTF8.self).split(separator: "\n").suffix(8).joined(separator: "\n")
        return text.isEmpty ? "(no output)" : text
    }

    private func message(_ text: String) {
        let escaped = text.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;")
        webView.loadHTMLString("<body style=\"font:15px -apple-system;margin:40px;color:#888\"><pre style=\"white-space:pre-wrap\">\(escaped)</pre>", baseURL: nil)
    }

    private func fail(_ what: String, _ next: String) { Notice.show(what, next, on: window) }

    // MARK: WKScriptMessageHandler (Report This Problem in the page)

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let address, WorkbenchLogic.isInternal(message.frameInfo.request.url, port: address.port) else { return }
        if let url = WorkbenchLogic.reportDestination(message.body, home: NSHomeDirectory()) { NSWorkspace.shared.open(url); return }
        if let report = WorkbenchLogic.saveRequest(message.body, home: NSHomeDirectory()) { saveReport(report.name, report.text) }
    }

    /// Save Report… on a result: a save panel, then the text file. The report holds the verdict, the evidence summary and the
    /// program's file name and hash. It never holds the program itself.
    private func saveReport(_ name: String, _ text: String) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = name
        panel.allowedContentTypes = [.plainText]
        panel.message = "Save a plain-text report of this check."
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            do { try text.write(to: url, atomically: true, encoding: .utf8) }
            catch { self?.fail("Ounce Core couldn’t save the report.", "\(error.localizedDescription) Choose another folder and try again.") }
        }
    }

    // MARK: WKNavigationDelegate

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        let url = action.request.url
        let allowed = address.map { WorkbenchLogic.isInternal(url, port: $0.port) } ?? (url == nil || url?.scheme == "about")
        if !allowed {
            if let url, ["http", "https"].contains(url.scheme) { NSWorkspace.shared.open(url) }
            return decisionHandler(.cancel)
        }
        decisionHandler(.allow)
    }

    // MARK: WKUIDelegate (the page needs confirm() and file pickers)

    func webView(_ webView: WKWebView, runJavaScriptConfirmPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo,
                 completionHandler: @escaping (Bool) -> Void) {
        let alert = NSAlert()
        // The page only asks for confirmation before running a program directly on this computer.
        alert.messageText = message.contains("directly on") ? "Run this program on this Mac?" : "Continue?"
        alert.informativeText = message
        alert.addButton(withTitle: "Continue")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { completionHandler($0 == .alertFirstButtonReturn) }
    }

    func webView(_ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo,
                 completionHandler: @escaping () -> Void) {
        let alert = NSAlert()
        alert.messageText = message
        alert.beginSheetModal(for: window) { _ in completionHandler() }
    }

    func webView(_ webView: WKWebView, runOpenPanelWith parameters: WKOpenPanelParameters, initiatedByFrame frame: WKFrameInfo,
                 completionHandler: @escaping ([URL]?) -> Void) {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = parameters.allowsMultipleSelection
        panel.canChooseDirectories = false
        panel.beginSheetModal(for: window) { completionHandler($0 == .OK ? panel.urls : nil) }
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction,
                 windowFeatures: WKWindowFeatures) -> WKWebView? { nil }
}

private extension URL {
    /// This folder, if it exists.
    var nilIfMissing: URL? { FileManager.default.fileExists(atPath: path) ? self : nil }
}
