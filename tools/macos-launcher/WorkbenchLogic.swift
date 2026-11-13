import Foundation

/// Where the local workbench server is listening and the key that opens it.
struct WorkbenchAddress: Equatable {
    let url: URL
    let port: Int
    let token: String
}

/// Pure logic for the workbench window (no AppKit), covered by `tests/macos/workbench-logic/main.swift`.
enum WorkbenchLogic {
    /// Finds the address in a line the server printed ("  Workbench: http://127.0.0.1:PORT/?t=KEY").
    static func parseAddress(_ line: String) -> WorkbenchAddress? {
        guard let range = line.range(of: "http://127.0.0.1:") else { return nil }
        let text = line[range.lowerBound...].trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: text), url.host == "127.0.0.1", let port = url.port,
              let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let token = components.queryItems?.first(where: { $0.name == "t" })?.value, !token.isEmpty
        else { return nil }
        return WorkbenchAddress(url: url, port: port, token: token)
    }

    /// Only the workbench itself may load in the window. Everything else opens in the browser.
    static func isInternal(_ url: URL?, port: Int) -> Bool {
        guard let url else { return true }  // loadHTMLString pages have no URL
        if url.scheme == "about" { return true }
        return url.scheme == "http" && url.host == "127.0.0.1" && url.port == port
    }

    private static let queryAllowed: CharacterSet = {
        var set = CharacterSet.urlQueryAllowed
        set.remove(charactersIn: "+&=#%")
        return set
    }()

    /// Settings used when the page can't supply its own: emulator-only, no Mac reference run.
    static let defaultOptions = "native=0"

    /// The page's own settings (`OunceWorkbench.queryString()`) if they look like plain query text, else the defaults.
    /// The page is local, but the text goes into a URL, so anything unexpected is dropped rather than passed on.
    static func checkOptions(fromPage value: Any?) -> String {
        guard let text = value as? String, !text.isEmpty, text.count <= 200,
              text.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || "=&,%-_.".contains($0)) }) else { return defaultOptions }
        return text
    }

    /// True when the settings ask for the program to run directly on this Mac (`native=1`).
    static func runsOnThisMac(_ options: String) -> Bool { options.split(separator: "&").contains("native=1") }

    /// A Linux program has no run on this Mac to compare with (`run-elf` has only Tier-3 and Tier-4), so the settings
    /// for it never ask for one, and the app never shows the "run on this Mac" confirmation for it.
    static func options(_ options: String, for kind: ProgramKind?) -> String {
        guard kind == .linux else { return options }
        return options.split(separator: "&").map { $0 == "native=1" ? "native=0" : String($0) }.joined(separator: "&")
    }

    /// The request that hands a program to the server, with the page's settings appended.
    static func uploadRequest(_ address: WorkbenchAddress, name: String, body: Data, options: String = defaultOptions) -> URLRequest {
        let escaped = name.addingPercentEncoding(withAllowedCharacters: queryAllowed) ?? "program.nro"
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(address.port)/api/checks?name=\(escaped)&\(options)")!)
        request.httpMethod = "POST"
        request.httpBody = body
        request.setValue(address.token, forHTTPHeaderField: "X-Ounce-Token")
        request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        return request
    }

    /// The id of a started check, or the server's own message when it refused the file.
    static func parseUploadResponse(_ data: Data) -> Result<String, WorkbenchError> {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return .failure(WorkbenchError(message: "The workbench sent an answer this app doesn't understand."))
        }
        if let id = object["id"] as? String, id.allSatisfy({ $0.isNumber || $0.isLetter || $0 == "-" }) { return .success(id) }
        return .failure(WorkbenchError(message: (object["error"] as? String) ?? "The workbench didn't accept that file."))
    }

    /// Arguments for the bundled server. `verifyDir` is the bundled recorded-reference data, when it is present.
    static func serverArguments(verifyDir: URL?) -> [String] {
        var arguments = ["ui", "--no-open", "--port", "0", "--parent-pipe"]
        if let verifyDir { arguments += ["--verify-dir", verifyDir.path] }
        return arguments
    }

    /// The issue-form address in a page's `{action: "report", url, text}` message, or nil for anything else.
    /// Only the project's own issue form is opened. Home folders are replaced by `~`.
    static func reportDestination(_ body: Any, home: String) -> URL? {
        guard let message = body as? [String: Any], message["action"] as? String == "report",
              let text = message["url"] as? String, text.hasPrefix(IssueFormLocation.url + "?") || text == IssueFormLocation.url,
              var parts = URLComponents(string: text) else { return nil }
        if !home.isEmpty, let query = parts.percentEncodedQuery {
            // Decode each value, hide the home folder, and encode it again ourselves: URLComponents would leave a
            // literal "+" (which GitHub reads as a space) and "&" handling to chance.
            let keep = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._~"))
            func decode(_ text: Substring) -> String { String(text).replacingOccurrences(of: "+", with: " ").removingPercentEncoding ?? String(text) }
            parts.percentEncodedQuery = query.split(separator: "&", omittingEmptySubsequences: true).map { pair -> String in
                let kv = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                let value = decode(kv.count > 1 ? kv[1] : "").replacingOccurrences(of: home, with: "~")
                return "\(kv[0])=\(value.addingPercentEncoding(withAllowedCharacters: keep) ?? "")"
            }.joined(separator: "&")
        }
        return parts.url
    }

    /// Environment for the server process: never inherit an engine choice from the shell.
    static func serverEnvironment(base: [String: String]) -> [String: String] {
        var env = base
        env["OUNCE_CPU"] = nil
        return env
    }
}

struct WorkbenchError: Error, Equatable { let message: String }

extension WorkbenchLogic {
    /// What the page's `OunceWorkbench.reportRequest()` returned (JSON text `{url, text}`), as the issue-form address.
    /// This is the same prefilled report as the page's Report This Problem button, so Help > Report a Problem agrees with it.
    /// Nil when there is no finished check yet or the answer is not understood.
    static func pageReport(_ value: Any?, home: String) -> URL? {
        guard let text = value as? String, text.count <= 20_000,
              let object = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any] else { return nil }
        return reportDestination(["action": "report", "url": object["url"] as Any], home: home)
    }

    /// The generic issue form, for when no check has finished. Opening it sends nothing: the person reviews and submits it.
    static func problemReportURL(appVersion: String, hostOS: String, hostChip: String = "") -> URL? {
        var parts = URLComponents(string: IssueFormLocation.url)
        parts?.queryItems = [
            URLQueryItem(name: "template", value: "emulation_bug.yml"),
            URLQueryItem(name: "observed", value: "Describe what happened and what you expected."),
            URLQueryItem(name: "environment", value: ["Ounce Core app \(appVersion)", hostOS, hostChip].filter { !$0.isEmpty }.joined(separator: ", ")),
        ]
        return parts?.url
    }

    /// The Mac's chip, for example "Apple M2", or an empty string when the system does not say.
    static func chipName() -> String {
        var size = 0
        guard sysctlbyname("machdep.cpu.brand_string", nil, &size, nil, 0) == 0, size > 1 else { return "" }
        var buffer = [CChar](repeating: 0, count: size)
        guard sysctlbyname("machdep.cpu.brand_string", &buffer, &size, nil, 0) == 0 else { return "" }
        return String(cString: buffer)
    }

    /// A report the page asks the app to save: a plain file name and text, with home folders replaced by `~`.
    static func saveRequest(_ body: Any, home: String) -> (name: String, text: String)? {
        guard let message = body as? [String: Any], message["action"] as? String == "save",
              let name = message["name"] as? String, var text = message["text"] as? String, text.utf8.count <= 200_000 else { return nil }
        let safe = (name as NSString).lastPathComponent.filter { !$0.isNewline && $0 != "/" && $0 != ":" }
        guard !safe.isEmpty, !safe.hasPrefix(".") else { return nil }
        if !home.isEmpty { text = text.replacingOccurrences(of: home, with: "~") }
        return (String(safe.prefix(120)), text)
    }
}

enum IssueFormLocation {
    static let url = "https://github.com/hffgzktbcc-blip/ounce-core/issues/new"
}

/// What happened and what to do next, for a dialog.
struct ProgramProblem: Equatable { let what: String; let next: String }

extension WorkbenchLogic {
    /// What the workbench can check: a Switch homebrew program or a static AArch64 Linux program.
    enum ProgramKind: Equatable { case homebrew, linux }

    /// The kind of program in these leading bytes (at least 20), judged by content: the NRO magic sits at offset
    /// 0x10; a Linux program is a 64-bit little-endian ELF whose machine (offset 18) is AArch64 (0xB7).
    /// A Linux program that is another kind of ELF (x86-64, 32-bit) is `nil`, like any other file.
    static func programKind(header: Data) -> ProgramKind? {
        let bytes = [UInt8](header.prefix(20))
        if bytes.count >= 20, Array(bytes[16..<20]) == Array("NRO0".utf8) { return .homebrew }
        if bytes.count >= 20, Array(bytes[0..<4]) == [0x7F, 0x45, 0x4C, 0x46],
           bytes[4] == 2, bytes[5] == 1, bytes[18] == 0xB7, bytes[19] == 0 { return .linux }
        return nil
    }

    /// Why a file can't be checked, or nil when it looks like a Switch homebrew program (.nro) or a static AArch64
    /// Linux program. Reads at most 20 bytes.
    static func programProblem(_ url: URL) -> ProgramProblem? {
        let name = url.lastPathComponent
        let header: Data
        do {
            let file = try FileHandle(forReadingFrom: url)
            defer { try? file.close() }
            header = try file.read(upToCount: 20) ?? Data()
        } catch {
            return ProgramProblem(what: "Ounce Core couldn’t read \(name).", next: "Check that the file is still there and that you can open it, then try again.")
        }
        switch programKind(header: header) {
        case .linux: return nil
        case .homebrew:
            if url.pathExtension.lowercased() == "nro" { return nil }
        case nil:
            if header.starts(with: Data([0x7F, 0x45, 0x4C, 0x46])) {
                return ProgramProblem(what: "\(name) is a Linux program, but not for 64-bit ARM (AArch64).",
                                      next: "Ounce Core checks static AArch64 Linux programs and Switch homebrew files ending in .nro. Programs built for Intel (x86-64) or 32-bit ARM can’t be checked here.")
            }
        }
        return ProgramProblem(what: "\(name) isn’t a program Ounce Core can check.",
                              next: "Choose a Switch homebrew file ending in .nro, or a static Linux ARM64 program (.elf, or a file with no extension). Files built for other systems can’t be checked here.")
    }
}
