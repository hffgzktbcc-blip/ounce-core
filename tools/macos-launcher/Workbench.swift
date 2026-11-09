import AppKit
import WebKit

/// The Ounce-Core workbench in a window: starts `ounce-core ui` from the bundled runtime and shows its page.
/// The window holds no logic of its own. The server is a child process (so `forbid(unsafe_code)` is untouched), it
/// listens on 127.0.0.1 only, and it exits when this app does (the stdin pipe below closes).
final class Workbench: NSObject, WKNavigationDelegate, WKUIDelegate {
    private let runtime: URL
    private var process: Process?
    private var keepAlive: Pipe?
    private var address: WorkbenchAddress?
    private var output = Data()
    private var buffer = ""
    private var pendingFiles: [URL] = []
    private var stopping = false
    private(set) var window: NSWindow!
    private var webView: WKWebView!

    init(runtime: URL) { self.runtime = runtime }

    func show() {
        let configuration = WKWebViewConfiguration()
        webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.setAccessibilityLabel("Ounce-Core workbench")
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 760),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Ounce Core"
        window.minSize = NSSize(width: 720, height: 480)
        window.contentView = webView
        window.setFrameAutosaveName("OunceCoreWorkbench")
        if !window.setFrameUsingName("OunceCoreWorkbench") { window.center() }
        window.makeKeyAndOrderFront(nil)
        message("Starting the workbench…")
        start()
    }

    func stop() {
        stopping = true
        process?.terminate()
    }

    /// Hands a program to the workbench (waits until the server is up).
    func check(file: URL) {
        guard let address else { pendingFiles.append(file); return }
        guard let data = try? Data(contentsOf: file) else { return fail("Couldn't read \(file.lastPathComponent).") }
        let request = WorkbenchLogic.uploadRequest(address, name: file.lastPathComponent, body: data)
        URLSession.shared.dataTask(with: request) { [weak self] data, _, error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error { return self.fail(error.localizedDescription) }
                switch WorkbenchLogic.parseUploadResponse(data ?? Data()) {
                case .success(let id): self.webView.evaluateJavaScript("pick('\(id)')")
                case .failure(let problem): self.fail(problem.message)
                }
            }
        }.resume()
        window.makeKeyAndOrderFront(nil)
    }

    func reveal() { window?.makeKeyAndOrderFront(nil) }

    private func start() {
        let process = Process()
        process.executableURL = runtime
        process.arguments = ["ui", "--no-open", "--port", "0", "--parent-pipe"]
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
                self.fail("The workbench stopped unexpectedly.\n\n" + self.tail())
            }
        }
        do { try process.run() } catch { return fail("Couldn't start the runtime: \(error.localizedDescription)") }
        self.process = process
        DispatchQueue.main.asyncAfter(deadline: .now() + 15) { [weak self] in
            if let self, self.address == nil, !self.stopping { self.fail("The workbench didn't start.\n\n" + self.tail()) }
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

    private func fail(_ text: String) {
        let alert = NSAlert()
        alert.messageText = "Ounce Core"
        alert.informativeText = text
        alert.addButton(withTitle: "OK")
        if let window { alert.beginSheetModal(for: window) } else { alert.runModal() }
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
        alert.messageText = "Ounce Core"
        alert.informativeText = message
        alert.addButton(withTitle: "Continue")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { completionHandler($0 == .alertFirstButtonReturn) }
    }

    func webView(_ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo,
                 completionHandler: @escaping () -> Void) {
        let alert = NSAlert()
        alert.messageText = "Ounce Core"
        alert.informativeText = message
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
