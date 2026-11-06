import Foundation

// Read-only projections of saved evidence. Verdict rules remain in Ounce-Verify.
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
    struct Oracle: Decodable { let name: String; let status: String }
    struct Section { let title: String; let lines: [String] }
    private struct Version: Decodable {
        let schema: String
        enum CodingKeys: String, CodingKey { case schema = "schema_version" }
    }
    private struct Document: Decodable { let records: [Case] }
    private struct Diagnosis: Decodable {
        struct Item: Decodable {
            struct Observations: Decodable {
                let reference: String
                let candidate: String
                enum CodingKeys: String, CodingKey { case reference = "reference_verdict", candidate = "candidate_verdict" }
            }
            struct Hypothesis: Decodable {
                let owner: String
                let next: String
                enum CodingKeys: String, CodingKey { case owner = "likely_owner", next = "next_check" }
            }
            let caseID: String
            let observations: Observations
            let hypothesis: Hypothesis
            enum CodingKeys: String, CodingKey { case caseID = "case_id", observations, hypothesis }
        }
        let cases: [Item]
    }
    private struct Repair: Decodable {
        struct Revision: Decodable {
            let commit: String
            enum CodingKeys: String, CodingKey { case commit = "core_commit" }
        }
        let program: String
        let qualified: Bool
        let issues: [String]
        let reference: Revision
        let candidate: Revision
    }
    private struct Run: Decodable {
        struct Stage: Decodable { let name: String; let status: String; let returncode: Int? }
        let commit: String
        let shards: Int
        let stages: [Stage]
        enum CodingKeys: String, CodingKey { case commit = "core_commit", shards = "oracle_shards", stages }
    }
    enum ReadError: LocalizedError {
        case invalid
        var errorDescription: String? {
            "Choose a saved Ounce-Verify corpus, diagnosis, repair or run report (.json), up to 8 MB and 1,000 results. This file could not be read as a supported report."
        }
    }
    let records: [Case]
    let sections: [Section]
    let heading: String
    let overview: String
    init(data: Data) throws {
        guard data.count <= 8 * 1024 * 1024 else { throw ReadError.invalid }
        let decoder = JSONDecoder()
        do {
            let version = try decoder.decode(Version.self, from: data)
            switch version.schema {
            case "ounce-verify/v0":
                let report = try decoder.decode(Document.self, from: data)
                guard !report.records.isEmpty, report.records.count <= 1000,
                      Set(report.records.map(\.caseID)).count == report.records.count,
                      report.records.allSatisfy({ !$0.caseID.isEmpty && $0.caseID.count <= 512 &&
                          $0.target.count <= 64 && $0.execution.engine.count <= 64 && $0.execution.commit.count <= 64 &&
                          $0.oracles.count <= 64 && $0.oracles.allSatisfy({ $0.name.count <= 256 && $0.status.count <= 64 }) &&
                          ["pass", "fail", "blocked", "not-assessed"].contains($0.verdict) }) else { throw ReadError.invalid }
                records = report.records.sorted { $0.caseID < $1.caseID }
                sections = []
                heading = "Saved check results"
                overview = "Pass: recorded checks passed. Failed: an observed mismatch. Blocked: checks could not finish. Not assessed: more evidence is needed."
            case "ounce-verify/diagnosis-v1":
                let report = try decoder.decode(Diagnosis.self, from: data)
                guard report.cases.count <= 1000 else { throw ReadError.invalid }
                records = []
                sections = report.cases.map { item in Section(title: item.caseID, lines: [
                    "Earlier result: \(item.observations.reference) • New result: \(item.observations.candidate)",
                    "Suggested area to inspect (hypothesis): \(item.hypothesis.owner)",
                    "Suggested next check: \(item.hypothesis.next)"
                ]) }
                heading = "Saved diagnosis"
                overview = "Suggested causes are hypotheses, not proof that a particular engine or component is wrong."
            case "ounce-verify/repair-v1":
                let report = try decoder.decode(Repair.self, from: data)
                guard report.issues.count <= 1000 else { throw ReadError.invalid }
                records = []
                sections = [Section(title: report.program, lines: [
                    report.qualified ? "The saved report records this repair as qualified." : "The saved report records this repair as rejected.",
                    "Earlier revision: \(report.reference.commit)", "Proposed revision: \(report.candidate.commit)"
                ] + report.issues)]
                heading = "Saved repair decision"
                overview = "This view cannot approve or apply a repair. Independent qualification must run against unchanged oracles and complete evidence."
            case "ounce-verify/run-v1":
                let report = try decoder.decode(Run.self, from: data)
                guard report.stages.count <= 1000 else { throw ReadError.invalid }
                records = []
                sections = report.stages.map { stage in Section(title: stage.name, lines: [
                    "Recorded process state: \(stage.status)" + (stage.returncode.map { " • Exit code: \($0)" } ?? "")
                ]) }
                heading = "Saved run and recovery history"
                overview = "Revision: \(report.commit) • Parallel groups: \(report.shards). Process completion is not an application compatibility result. Retries appear as separate recorded steps."
            default: throw ReadError.invalid
            }
            guard sections.allSatisfy({ $0.title.count <= 512 && $0.lines.allSatisfy({ $0.count <= 4096 }) }) else { throw ReadError.invalid }
        } catch { throw ReadError.invalid }
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
