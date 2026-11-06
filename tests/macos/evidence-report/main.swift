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
