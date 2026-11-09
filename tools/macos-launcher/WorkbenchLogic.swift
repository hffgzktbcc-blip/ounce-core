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

    /// The request that hands a program to the server. Emulator-only: the Mac reference is a choice made in the page.
    static func uploadRequest(_ address: WorkbenchAddress, name: String, body: Data) -> URLRequest {
        let escaped = name.addingPercentEncoding(withAllowedCharacters: queryAllowed) ?? "program.nro"
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(address.port)/api/checks?name=\(escaped)&native=0")!)
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

    /// Environment for the server process: never inherit an engine choice from the shell.
    static func serverEnvironment(base: [String: String]) -> [String: String] {
        var env = base
        env["OUNCE_CPU"] = nil
        return env
    }
}

struct WorkbenchError: Error, Equatable { let message: String }
