import AppKit
import UniformTypeIdentifiers

// The app is one window: the workbench. The runtime stays a separate child process (`Ounce Runtime.app`), and the
// older launcher functions that the workbench does not cover live in the Advanced menu (Advanced.swift).
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation, NSMenuDelegate {
    private var workbench: Workbench!
    private var advanced: AdvancedRunner!
    private let results = ResultsViewer()
    private var pendingFiles: [URL] = []
    private var recentMenu = NSMenu(title: "Open Recent")
    private let documentation = URL(string: "https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/MAC_APP.md")!

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.mainMenu = makeMenu()
        let runtime = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/Ounce Runtime.app/Contents/MacOS/ounce-core")
        workbench = Workbench(runtime: runtime)
        advanced = AdvancedRunner(runtime: runtime, window: { [weak self] in self?.workbench.window })
        workbench.show()
        // Closing the one window quits, so the checker never keeps running without a window.
        NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: workbench.window, queue: .main) { _ in NSApp.terminate(nil) }
        NSApp.activate(ignoringOtherApps: true)
        let files = pendingFiles
        pendingFiles = []
        files.forEach { workbench.check(file: $0) }
    }

    // MARK: Menu

    private func makeMenu() -> NSMenu {
        let bar = NSMenu()
        func add(_ menu: NSMenu, _ title: String, _ action: Selector?, _ key: String = "", _ modifiers: NSEvent.ModifierFlags = .command) {
            let item = menu.addItem(withTitle: title, action: action, keyEquivalent: key)
            item.keyEquivalentModifierMask = modifiers
            if action != nil, action != #selector(NSApplication.terminate(_:)), action != #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
               action != #selector(NSApplication.hide(_:)) { item.target = self }
        }
        func attach(_ menu: NSMenu) { let item = NSMenuItem(); item.submenu = menu; bar.addItem(item) }

        let app = NSMenu()
        add(app, "About Ounce Core", #selector(NSApplication.orderFrontStandardAboutPanel(_:)))
        app.addItem(.separator())
        add(app, "Hide Ounce Core", #selector(NSApplication.hide(_:)), "h")
        app.addItem(.separator())
        add(app, "Quit Ounce Core", #selector(NSApplication.terminate(_:)), "q")
        attach(app)

        let file = NSMenu(title: "File")
        add(file, "Check a Program…", #selector(checkProgram), "o")
        let recent = NSMenuItem(title: "Open Recent", action: nil, keyEquivalent: "")
        recentMenu.delegate = self
        recent.submenu = recentMenu
        file.addItem(recent)
        add(file, "Open Saved Results…", #selector(openResults), "o", [.command, .shift])
        file.addItem(.separator())
        add(file, "Close Window", #selector(NSWindow.performClose(_:)), "w")
        attach(file)

        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        attach(edit)

        let view = NSMenu(title: "View")
        add(view, "Actual Size", #selector(zoomReset), "0")
        add(view, "Zoom In", #selector(zoomIn), "+")
        add(view, "Zoom Out", #selector(zoomOut), "-")
        view.addItem(.separator())
        add(view, "Enter Full Screen", #selector(NSWindow.toggleFullScreen(_:)), "f", [.command, .control])
        attach(view)

        let advancedMenu = NSMenu(title: "Advanced")
        add(advancedMenu, "Run in a Window…", #selector(runInWindow), "r")
        add(advancedMenu, "Graphics Demo", #selector(graphicsDemo))
        add(advancedMenu, "Verify Against Host Reference…", #selector(verifyAgainstHost))
        advancedMenu.addItem(.separator())
        add(advancedMenu, "Run Settings…", #selector(runSettings))
        add(advancedMenu, "SD Card Folder…", #selector(sdCardFolder))
        add(advancedMenu, "Show Last Session Log", #selector(showLog))
        advancedMenu.addItem(.separator())
        add(advancedMenu, "Stop Running Program", #selector(stopRunning), ".")
        attach(advancedMenu)

        let help = NSMenu(title: "Help")
        add(help, "Report a Problem…", #selector(reportProblem))
        add(help, "Documentation", #selector(openDocumentation))
        attach(help)
        NSApp.helpMenu = help
        return bar
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        guard menu === recentMenu else { return }
        menu.removeAllItems()
        let recents = NSDocumentController.shared.recentDocumentURLs
        if recents.isEmpty {
            let none = menu.addItem(withTitle: "No Recent Programs", action: nil, keyEquivalent: "")
            none.isEnabled = false
            return
        }
        for url in recents {
            let item = menu.addItem(withTitle: url.lastPathComponent, action: #selector(openRecent(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = url
            item.toolTip = url.path
        }
        menu.addItem(.separator())
        let clear = menu.addItem(withTitle: "Clear Menu", action: #selector(clearRecents), keyEquivalent: "")
        clear.target = self
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard let advanced else { return true }
        switch item.action {
        case #selector(runInWindow), #selector(graphicsDemo), #selector(verifyAgainstHost), #selector(runSettings), #selector(sdCardFolder):
            return !advanced.isRunning
        case #selector(stopRunning): return advanced.isRunning
        default: return true
        }
    }

    // MARK: Actions

    @objc private func checkProgram() {
        let panel = NSOpenPanel()
        panel.message = "Choose a Switch homebrew program (a file ending in .nro) to check."
        panel.prompt = "Check"
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [UTType(filenameExtension: "nro") ?? .data]
        if panel.runModal() == .OK, let url = panel.url { workbench.check(file: url) }
    }

    @objc private func openRecent(_ sender: NSMenuItem) { if let url = sender.representedObject as? URL { workbench.check(file: url) } }
    @objc private func clearRecents() { NSDocumentController.shared.clearRecentDocuments(nil) }

    @objc private func openResults() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a saved results file (result.json from a check, or an Ounce-Verify report). Opening it does not run anything."
        panel.prompt = "Open"
        let checks = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Ounce Core/Checks", isDirectory: true)
        if FileManager.default.fileExists(atPath: checks.path) { panel.directoryURL = checks }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do { results.show(try EvidenceReport.read(url), from: url) }
        catch {
            Notice.show("Ounce Core couldn’t open \(url.lastPathComponent).",
                        "\(error.localizedDescription) Choose a result.json saved by a check, or a report written by Ounce-Verify.", on: workbench.window)
        }
    }

    @objc private func zoomIn() { workbench.zoom(by: 1.1) }
    @objc private func zoomOut() { workbench.zoom(by: 1 / 1.1) }
    @objc private func zoomReset() { workbench.zoom(to: 1) }
    @objc private func runInWindow() { advanced.chooseProgram() }
    @objc private func graphicsDemo() { advanced.graphicsDemo() }
    @objc private func verifyAgainstHost() { advanced.chooseVerification() }
    @objc private func runSettings() { advanced.showSettings() }
    @objc private func sdCardFolder() { advanced.chooseSDFolder() }
    @objc private func showLog() { advanced.showLastLog() }
    @objc private func stopRunning() { advanced.stop() }
    @objc private func reportProblem() { workbench.reportProblem() }
    @objc private func openDocumentation() { NSWorkspace.shared.open(documentation) }

    // MARK: Opening files from Finder or the Dock

    func application(_ sender: NSApplication, openFiles filenames: [String]) {
        let urls = filenames.map { URL(fileURLWithPath: $0) }
        guard let program = urls.first(where: { $0.pathExtension.lowercased() == "nro" }) ?? urls.first else {
            return sender.reply(toOpenOrPrint: .failure)
        }
        sender.reply(toOpenOrPrint: .success)
        if workbench == nil { pendingFiles = [program] } else { workbench.check(file: program) }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { advanced?.terminate(); workbench?.stop() }
}

let application = NSApplication.shared
let delegate = AppDelegate()
application.delegate = delegate
application.run()
