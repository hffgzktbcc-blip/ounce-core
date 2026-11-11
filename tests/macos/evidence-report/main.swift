import Foundation
let valid = Data("""
{"schema_version":"ounce-verify/v0","records":[{"case_id":"corpus/demo/interpreter","target":"original-switch-libnx","verdict":"blocked","execution":{"engine":"interpreter","core_commit":"recorded-revision"},"oracles":[{"name":"first-frame","status":"blocked"}]}]}
""".utf8)
let report = try EvidenceReport(data: valid)
precondition(report.records.count == 1)
precondition(EvidenceReport.explanation(report.records[0].verdict) == "Checks could not finish")
for data in [Data("{}".utf8), Data("{broken".utf8), Data(repeating: 0, count: 8 * 1024 * 1024 + 1)] {
    do { _ = try EvidenceReport(data: data); fatalError("invalid report accepted") }
    catch { precondition(error is EvidenceReport.ReadError) }
}
print("Saved-report decoding checks passed; no verdicts recomputed by the UI.")

let repair = Data("""
{"schema_version":"ounce-verify/repair-v1","program":"sample","qualified":false,"issues":["workload target changed"],"reference":{"core_commit":"before"},"candidate":{"core_commit":"after"}}
""".utf8)
let decision = try EvidenceReport(data: repair)
precondition(decision.records.isEmpty)
precondition(decision.sections[0].lines.contains("workload target changed"))
precondition(decision.overview.contains("cannot approve or apply"))
let run = Data("""
{"schema_version":"ounce-verify/run-v1","core_commit":"recorded","oracle_shards":2,"stages":[{"name":"pull-retry-1","status":"exited","returncode":0}]}
""".utf8)
let history = try EvidenceReport(data: run)
precondition(history.sections[0].title == "pull-retry-1")
precondition(history.overview.contains("not an application compatibility result"))

let sdk = try EvidenceReport.read(URL(fileURLWithPath: "docs/verify/hbx-first-frame-local.json"))
precondition(sdk.sections.count == 2)
precondition(sdk.overview.contains("not Switch 2 GPU-path validation"))

let audioFixture = Data("""
{"schema_version":"ounce-diagnose/audio-padding-v1","mode":"offline-recorded-observations","cases":[{"engine":"Interpreter","rejected_before":7,"stereo_after":7}],"recorded_repair_matches":true,"explanation":"Padding was rejected.","likely_owner":"channel validation","limits":"No fresh guest execution."}
""".utf8)
let audioReport = try EvidenceReport(data: audioFixture)
precondition(audioReport.sections[1].lines[0] == "Rejected before: 7 • Stereo after: 7")
precondition(audioReport.sections[0].lines.contains("Limits: No fresh guest execution."))
let comparisonFixture = Data("""
{"schema_version":"ounce-verify/comparison-v1","baseline":"before","candidate":"after","verdict_changes":[{"case":"demo/interpreter","before":"fail","after":"pass"}],"regressions":[],"diagnosis":{"cases":[]}}
""".utf8)
let comparisonReport = try EvidenceReport(data: comparisonFixture)
precondition(comparisonReport.sections[1].lines[0] == "Earlier result: fail • New result: pass")
for fixture in [audioFixture, comparisonFixture] {
    var object = try JSONSerialization.jsonObject(with: fixture) as! [String: Any]
    object.removeValue(forKey: "cases")
    object.removeValue(forKey: "verdict_changes")
    let malformed = try JSONSerialization.data(withJSONObject: object)
    do { _ = try EvidenceReport(data: malformed); fatalError("incomplete report accepted") }
    catch { precondition(error is EvidenceReport.ReadError) }
}

// Exercise the actual portable CLI output, including both dossier report types.
if CommandLine.arguments.count == 3 {
    let audio = try EvidenceReport.read(URL(fileURLWithPath: CommandLine.arguments[1]))
    precondition(audio.heading == "Saved audio diagnosis")
    precondition(audio.sections.count == 3)
    precondition(audio.sections[1].lines[0] == "Rejected before: 7 • Stereo after: 7")
    precondition(audio.sections[0].lines.contains { $0.contains("no fresh guest execution") })
    let comparison = try EvidenceReport.read(URL(fileURLWithPath: CommandLine.arguments[2]))
    precondition(comparison.heading == "Saved comparison")
    precondition(comparison.sections.contains { $0.lines.contains("Earlier result: fail • New result: pass") })
    precondition(comparison.overview.contains("does not rerun"))
    print("Generated audio and comparison dossiers decoded with recorded-evidence limits.")
}

let beforeAudio = try EvidenceReport.read(URL(fileURLWithPath: "docs/verify/audout-padding/before.json"))
precondition(beforeAudio.heading == "Saved audio observation")
precondition(beforeAudio.sections.count == 2)
precondition(beforeAudio.sections[0].lines.contains { $0.contains("Rejected: 7") })

let replayResult = try EvidenceReport.read(URL(fileURLWithPath: "docs/verify/diagnosis-cli-0.1.0/failure-result.json"))
precondition(replayResult.heading == "Saved replay result")
precondition(replayResult.sections[0].lines.contains { $0.contains("Side: failure") })

let workbenchFixture = Data("""
{"schema_version":"ounce-core/workbench-check-v1","id":"0001-deadbeef","name":"demo.nro","sha256":"deadbeef123456","bytes":1024,"native":true,"status":"done","verdict":{"kind":"fails","headline":"The program called a service Ounce-Core does not handle yet","detail":"Tier-3: [VM] Stopping on unhandled SVC #0x7F."},"instructions":{"checked":120,"mismatched":0},"engines":[{"engine":"tier3","status":"exited","seconds":0.2,"stop":{"kind":"unhandled-svc","line":"[VM] Stopping on unhandled SVC #0x7F."}}]}
""".utf8)
let workbenchReport = try EvidenceReport(data: workbenchFixture)
precondition(workbenchReport.heading == "Saved workbench check")
precondition(workbenchReport.sections.count == 3)

do {
    _ = try EvidenceReport.read(URL(fileURLWithPath: "docs/verify/audout-padding/manifest.json"))
    fatalError("dossier manifest should be rejected with specific guidance")
} catch let error as EvidenceReport.ReadError {
    precondition(error == .manifestSelected)
    precondition(error.localizedDescription.contains("diagnosis.json"))
}
