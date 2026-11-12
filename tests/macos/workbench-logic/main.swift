import Foundation

func check(_ condition: @autoclosure () -> Bool, _ message: String) {
    if !condition() { FileHandle.standardError.write(Data("FAILED: \(message)\n".utf8)); exit(1) }
}

// The address line the server prints.
let line = "  Workbench: http://127.0.0.1:52948/?t=ff5b9d5b50eb55c036e472cd03514618"
let found = WorkbenchLogic.parseAddress(line)
check(found?.port == 52948 && found?.token == "ff5b9d5b50eb55c036e472cd03514618", "address parsed from the printed line")
check(WorkbenchLogic.parseAddress("Checks are saved under /tmp") == nil, "unrelated lines are ignored")
check(WorkbenchLogic.parseAddress("http://127.0.0.1:80/") == nil, "an address without a key is rejected")
check(WorkbenchLogic.parseAddress("Workbench: http://evil.example:80/?t=abc") == nil, "only 127.0.0.1 is accepted")

// Navigation policy: the window shows the workbench and nothing else.
check(WorkbenchLogic.isInternal(URL(string: "http://127.0.0.1:52948/?t=x"), port: 52948), "own page is internal")
check(!WorkbenchLogic.isInternal(URL(string: "http://127.0.0.1:9999/"), port: 52948), "another local port is external")
check(!WorkbenchLogic.isInternal(URL(string: "https://example.com/"), port: 52948), "the web is external")
check(!WorkbenchLogic.isInternal(URL(string: "http://localhost:52948/"), port: 52948), "a different host name is external")
check(WorkbenchLogic.isInternal(URL(string: "about:blank"), port: 52948), "about:blank is internal")
check(WorkbenchLogic.isInternal(nil, port: 52948), "HTML strings have no URL")

// Upload request: key header, escaped name, emulator only.
let address = found!
let request = WorkbenchLogic.uploadRequest(address, name: "my game+v2 & co.nro", body: Data([1, 2, 3]))
check(request.httpMethod == "POST" && request.httpBody == Data([1, 2, 3]), "POST with the file as the body")
check(request.value(forHTTPHeaderField: "X-Ounce-Token") == address.token, "session key sent")
let query = request.url!.absoluteString
check(query.hasSuffix("&native=0"), "never asks for the Mac reference on its own")
check(query.contains("name=my%20game%2Bv2%20%26%20co.nro"), "name is escaped, including + and &: \(query)")
check(request.url!.host == "127.0.0.1" && request.url!.port == 52948, "request goes to the workbench")

// The page's own settings are used when they look like plain query text.
let pageOptions = "engines=tier3%2Ctier4&budget=5000&native=0&reference=1"
check(WorkbenchLogic.checkOptions(fromPage: pageOptions) == pageOptions, "page settings are passed on")
check(WorkbenchLogic.checkOptions(fromPage: nil) == "native=0", "no answer falls back to emulator-only")
check(WorkbenchLogic.checkOptions(fromPage: "engines=tier3&x=1#frag /../") == "native=0", "unexpected characters fall back to the defaults")
check(WorkbenchLogic.checkOptions(fromPage: 42) == "native=0", "a non-string answer falls back")
check(WorkbenchLogic.runsOnThisMac("engines=tier3&native=1") && !WorkbenchLogic.runsOnThisMac("native=0&reference=1"), "native=1 is recognised")
let withOptions = WorkbenchLogic.uploadRequest(address, name: "a.nro", body: Data(), options: pageOptions).url!.absoluteString
check(withOptions.hasSuffix("name=a.nro&" + pageOptions), "upload carries the page settings: \(withOptions)")

// Server arguments: the recorded-reference data is passed when it is bundled.
check(WorkbenchLogic.serverArguments(verifyDir: nil) == ["ui", "--no-open", "--port", "0", "--parent-pipe"], "no verify dir, no option")
check(WorkbenchLogic.serverArguments(verifyDir: URL(fileURLWithPath: "/App/Resources/Verify")).suffix(2) == ["--verify-dir", "/App/Resources/Verify"], "bundled verify dir is passed")

// The page's report message opens the project's issue form and nothing else.
let issue = IssueFormLocation.url + "?template=emulation_bug.yml&observed=at%20/Users/me/x"
let reportMessage: [String: Any] = ["action": "report", "url": issue, "text": "details"]
let destination = WorkbenchLogic.reportDestination(reportMessage, home: "/Users/me")
check(destination?.host == "github.com" && destination?.absoluteString.contains("Users") == false, "report url accepted, home removed: \(String(describing: destination))")
check(WorkbenchLogic.reportDestination(["action": "report", "url": "https://evil.example/issues/new"], home: "/Users/me") == nil, "another site is refused")
check(WorkbenchLogic.reportDestination(["action": "report", "url": IssueFormLocation.url + ".evil.example/x"], home: "") == nil, "a look-alike address is refused")
check(WorkbenchLogic.reportDestination(["action": "open", "url": issue], home: "") == nil, "other actions are ignored")
check(WorkbenchLogic.reportDestination("report", home: "") == nil, "a non-object message is ignored")

// Server answers.
check(WorkbenchLogic.parseUploadResponse(Data(#"{"id":"0001-abc123"}"#.utf8)) == .success("0001-abc123"), "id accepted")
check(WorkbenchLogic.parseUploadResponse(Data(#"{"error":"That isn't a homebrew .nro file."}"#.utf8)) == .failure(WorkbenchError(message: "That isn't a homebrew .nro file.")), "server message passed on")
check(WorkbenchLogic.parseUploadResponse(Data(#"{"id":"1');alert(1);('"}"#.utf8)) != .success("1');alert(1);('"), "an id that could break out of the JS call is refused")
check(WorkbenchLogic.parseUploadResponse(Data("nonsense".utf8)) == .failure(WorkbenchError(message: "The workbench sent an answer this app doesn't understand.")), "garbage handled")

// The server never inherits an engine choice.
let env = WorkbenchLogic.serverEnvironment(base: ["OUNCE_CPU": "native", "PATH": "/bin"])
check(env["OUNCE_CPU"] == nil && env["PATH"] == "/bin", "OUNCE_CPU stripped, rest kept")

// Help > Report a Problem uses the page's own report, so both agree.
let pageJSON = #"{"url":"\#(IssueFormLocation.url)?template=emulation_bug.yml&title=Workbench%3A%20Results%20differ%20-%20demo.nro&engine=tier3&observed=SHA-256%3A%20abc%20/Users/me/x","text":"details"}"#
let fromPage = WorkbenchLogic.pageReport(pageJSON, home: "/Users/me")
check(fromPage?.absoluteString.contains("engine=tier3") == true && fromPage?.absoluteString.contains("SHA-256") == true, "page report keeps engine and hash: \(String(describing: fromPage))")
check(fromPage?.absoluteString.contains("Users") == false, "page report hides the home folder")
check(WorkbenchLogic.pageReport("null", home: "") == nil, "no finished check, no page report")
check(WorkbenchLogic.pageReport(#"{"url":"https://evil.example/x"}"#, home: "") == nil, "a page report for another site is refused")
check(WorkbenchLogic.pageReport(nil, home: "") == nil, "a missing answer is no report")
let blank = WorkbenchLogic.problemReportURL(appVersion: "0.5.0", hostOS: "macOS 26")!.absoluteString
check(blank.contains("template=emulation_bug.yml") && blank.contains("0.5.0") && !blank.contains("title="), "report without a check still opens the form")

// Saving a report from the page.
let saved = WorkbenchLogic.saveRequest(["action": "save", "name": "../demo - report.txt", "text": "from /Users/me/x"], home: "/Users/me")
check(saved?.name == "demo - report.txt" && saved?.text == "from ~/x", "save keeps a plain file name and hides the home folder: \(String(describing: saved))")
check(WorkbenchLogic.saveRequest(["action": "save", "name": ".hidden", "text": "x"], home: "") == nil, "a hidden file name is refused")
check(WorkbenchLogic.saveRequest(["action": "report", "name": "a.txt", "text": "x"], home: "") == nil, "other actions are not saves")

// Program problems: say what happened and what to do.
let dir = FileManager.default.temporaryDirectory.appendingPathComponent("wb-problem-\(UUID().uuidString)")
try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: dir) }
func make(_ name: String, _ bytes: [UInt8]) -> URL { let u = dir.appendingPathComponent(name); try? Data(bytes).write(to: u); return u }
let nroBytes: [UInt8] = Array(repeating: 0, count: 16) + Array("NRO0".utf8)
check(WorkbenchLogic.programProblem(make("ok.nro", nroBytes)) == nil, "a valid NRO has no problem")
func elf(machine: UInt8, cls: UInt8 = 2, data: UInt8 = 1) -> [UInt8] {
    var h = [UInt8](repeating: 0, count: 24)
    h.replaceSubrange(0..<4, with: [0x7F, 0x45, 0x4C, 0x46]); h[4] = cls; h[5] = data; h[18] = machine
    return h
}
check(WorkbenchLogic.programProblem(make("linux.elf", elf(machine: 0xB7))) == nil, "a static AArch64 ELF is accepted")
check(WorkbenchLogic.programProblem(make("lz4", elf(machine: 0xB7))) == nil, "an extensionless AArch64 ELF is accepted")
check(WorkbenchLogic.programKind(header: Data(elf(machine: 0xB7))) == .linux, "ELF kind is judged by content")
check(WorkbenchLogic.programKind(header: Data(nroBytes)) == .homebrew, "NRO kind is judged by content")
check(WorkbenchLogic.programProblem(make("x86.elf", elf(machine: 0x3E)))?.what.contains("not for 64-bit ARM") == true, "an x86-64 ELF is refused with the reason")
check(WorkbenchLogic.programProblem(make("arm32.elf", elf(machine: 0xB7, cls: 1))) != nil, "a 32-bit ELF is refused")
check(WorkbenchLogic.programProblem(make("be.elf", elf(machine: 0xB7, data: 2))) != nil, "a big-endian ELF is refused")
check(WorkbenchLogic.programProblem(make("short.elf", [0x7F, 0x45, 0x4C, 0x46, 2, 1, 1])) != nil, "a truncated ELF header is refused")
check(WorkbenchLogic.programProblem(make("bad.nro", [1, 2, 3]))?.what.contains("isn’t a program Ounce Core can check") == true, "a short .nro is refused")
check(WorkbenchLogic.programProblem(make("data.bin", nroBytes)) != nil, "a wrong extension is refused")
check(WorkbenchLogic.programProblem(dir.appendingPathComponent("missing.nro"))?.what.contains("couldn’t read") == true, "a missing file is explained")
check(WorkbenchLogic.options("engines=tier3,tier4&native=1&reference=1", for: .linux) == "engines=tier3,tier4&native=0&reference=1", "a Linux program never asks for a Mac run")
check(WorkbenchLogic.options("engines=tier3,tier4&native=1", for: .homebrew) == "engines=tier3,tier4&native=1", "an NRO keeps its settings")
check(!WorkbenchLogic.runsOnThisMac(WorkbenchLogic.options("native=1", for: .linux)), "no Mac confirmation for a Linux program")
print("workbench-logic: all checks passed")
