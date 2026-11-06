import Foundation

// A read-only projection of saved evidence. Verdict rules remain in Ounce-Verify.
struct EvidenceReport {
    struct Case: Decodable {
        let caseID: String
        let target: String
        let verdict: String
        let execution: Execution
        let oracles: [Oracle]
        enum CodingKeys: String, CodingKey { case caseID = "case_id", target, verdict, execution, oracles }
    }
    struct Execution: Decodable {
        let engine: String
        let commit: String
        enum CodingKeys: String, CodingKey { case engine; case commit = "core_commit" }
    }
    struct Oracle: Decodable {
        let name: String
        let status: String
    }
    private struct Document: Decodable {
        let schema: String
        let records: [Case]
        enum CodingKeys: String, CodingKey { case schema = "schema_version", records }
    }
    enum ReadError: LocalizedError {
        case invalid
        var errorDescription: String? {
            "Choose a saved Ounce-Verify corpus report (.json), up to 8 MB and 1,000 results. This file could not be read as a supported report."
        }
    }
    let records: [Case]
    init(data: Data) throws {
        guard data.count <= 8 * 1024 * 1024,
              let report = try? JSONDecoder().decode(Document.self, from: data),
              report.schema == "ounce-verify/v0", !report.records.isEmpty, report.records.count <= 1000,
              Set(report.records.map(\.caseID)).count == report.records.count,
              report.records.allSatisfy({ !$0.caseID.isEmpty &&
                  ["pass", "fail", "blocked", "not-assessed"].contains($0.verdict) }) else {
            throw ReadError.invalid
        }
        records = report.records.sorted { $0.caseID < $1.caseID }
    }
    static func read(_ url: URL) throws -> EvidenceReport {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let data = try handle.read(upToCount: 8 * 1024 * 1024 + 1) ?? Data()
        return try EvidenceReport(data: data)
    }
    static func explanation(_ verdict: String) -> String {
        switch verdict {
        case "pass": return "Passed the recorded checks"
        case "fail": return "A recorded check failed"
        case "blocked": return "Checks could not finish"
        default: return "Not enough evidence to assess"
        }
    }
}
