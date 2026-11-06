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
