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

// Server answers.
check(WorkbenchLogic.parseUploadResponse(Data(#"{"id":"0001-abc123"}"#.utf8)) == .success("0001-abc123"), "id accepted")
check(WorkbenchLogic.parseUploadResponse(Data(#"{"error":"That isn't a homebrew .nro file."}"#.utf8)) == .failure(WorkbenchError(message: "That isn't a homebrew .nro file.")), "server message passed on")
check(WorkbenchLogic.parseUploadResponse(Data(#"{"id":"1');alert(1);('"}"#.utf8)) != .success("1');alert(1);('"), "an id that could break out of the JS call is refused")
check(WorkbenchLogic.parseUploadResponse(Data("nonsense".utf8)) == .failure(WorkbenchError(message: "The workbench sent an answer this app doesn't understand.")), "garbage handled")

// The server never inherits an engine choice.
let env = WorkbenchLogic.serverEnvironment(base: ["OUNCE_CPU": "native", "PATH": "/bin"])
check(env["OUNCE_CPU"] == nil && env["PATH"] == "/bin", "OUNCE_CPU stripped, rest kept")
print("workbench-logic: all checks passed")
