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
        let schema: String?
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
    private struct AudioDiagnosis: Decodable {
        struct Item: Decodable {
            let engine: String
            let rejected_before: Int
            let stereo_after: Int
        }
        let mode: String
        let cases: [Item]
        let recorded_repair_matches: Bool
        let explanation: String
        let likely_owner: String
        let limits: String
    }
    private struct Comparison: Decodable {
        struct Change: Decodable {
            let `case`: String
            let before: String
            let after: String
        }
        let baseline: String
        let candidate: String
        let verdict_changes: [Change]
        let regressions: [Change]
        let diagnosis: Diagnosis
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
    private struct SDKReport: Decodable {
        struct Item: Decodable {
            let engine: String
            let pass: Bool
            let error: String?
        }
        let target: String
        let runs: [Item]
    }
    private struct AudioObservation: Decodable {
        struct RunItem: Decodable {
            struct Attempt: Decodable {
                let rate: Int
                let raw_channels: Int
                let result: Int
                let channels: Int?
            }
            let engine: String
            let pass: Bool
            let audio_open: [Attempt]
        }
        let nro_sha256: String
        let queued_frames: Int
        let pass: Bool
        let runs: [RunItem]
    }
    private struct ReplayResult: Decodable {
        let side: String
        let core_commit: String
        let binary_sha256: String
        let timeout_seconds: Double
        let matched: Bool
        let status: String
        let exit_code: Int
        let observation_error: String?
    }
    private struct WorkbenchCheck: Decodable {
        struct EngineItem: Decodable {
            struct Stop: Decodable { let kind: String; let line: String }
            let engine: String
            let status: String
            let seconds: Double
            let stop: Stop
        }
        struct Verdict: Decodable {
            let kind: String
            let headline: String
            let detail: String
        }
        struct Instructions: Decodable {
            let checked: Int
            let mismatched: Int
        }
        let name: String?
        let sha256: String?
        let status: String
        let engines: [EngineItem]
        let verdict: Verdict
        let instructions: Instructions?
    }
    enum ReadError: LocalizedError {
        case invalid
        case manifestSelected
        var errorDescription: String? {
            switch self {
            case .manifestSelected:
                return "This file is a dossier manifest (manifest.json) that lists file hashes. Open diagnosis.json, comparison.json, before.json, after.json, or canonical-corpus.json in the same folder instead."
            case .invalid:
                return "Choose a saved Ounce-Verify corpus, diagnosis, comparison, repair or run report, an Ounce-Diagnose audio or replay report, or a Workbench result.json (.json), up to 8 MB and 1,000 results. This file could not be read as a supported report."
            }
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
            switch version.schema ?? "ounce-core/workbench-check-v1" {
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
            case "ounce-diagnose/audio-padding-v1":
                let report = try decoder.decode(AudioDiagnosis.self, from: data)
                guard report.cases.count <= 1000 else { throw ReadError.invalid }
                records = []
                sections = [Section(title: "Recorded audio diagnosis", lines: [
                    "Mode: \(report.mode)", report.explanation,
                    "Suggested area to inspect (hypothesis): \(report.likely_owner)",
                    report.recorded_repair_matches ? "Recorded repair matches the saved observations." : "Recorded repair does not match the saved observations.",
                    "Limits: \(report.limits)"
                ])] + report.cases.map { item in Section(title: item.engine, lines: [
                    "Rejected before: \(item.rejected_before) • Stereo after: \(item.stereo_after)"
                ]) }
                heading = "Saved audio diagnosis"
                overview = "Recorded protocol observations only; this view does not execute a guest or independently qualify a repair."
            case "ounce-verify/comparison-v1":
                let report = try decoder.decode(Comparison.self, from: data)
                guard report.verdict_changes.count <= 1000, report.regressions.count <= 1000,
                      report.diagnosis.cases.count <= 1000 else { throw ReadError.invalid }
                records = []
                sections = [Section(title: "Recorded revisions", lines: [
                    "Earlier revision: \(report.baseline)", "New revision: \(report.candidate)",
                    "Recorded regressions: \(report.regressions.count)"
                ])] + report.verdict_changes.map { item in Section(title: item.case, lines: [
                    "Earlier result: \(item.before) • New result: \(item.after)"
                ]) } + report.diagnosis.cases.map { item in Section(title: item.caseID, lines: [
                    "Suggested area to inspect (hypothesis): \(item.hypothesis.owner)",
                    "Suggested next check: \(item.hypothesis.next)"
                ]) }
                heading = "Saved comparison"
                overview = "Saved verdict changes and diagnosis hypotheses; this view does not rerun checks or establish compatibility."
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
            case "ounce-verify/hbx-first-frame-v1":
                let report = try decoder.decode(SDKReport.self, from: data)
                guard report.runs.count <= 1000 else { throw ReadError.invalid }
                records = []
                sections = report.runs.map { run in Section(title: run.engine, lines: [
                    run.pass ? "The saved report records passed pixel checks." : "The saved report records failed pixel checks.",
                    run.error ?? "No execution error was recorded."
                ]) }
                heading = "Saved Switch 2 SDK-path results"
                overview = "Recorded target: \(report.target). This path uses HBLauncher host UI callbacks; it is not Switch 2 GPU-path validation."
            case "ounce-verify/audout-padding-v1":
                let report = try decoder.decode(AudioObservation.self, from: data)
                guard !report.runs.isEmpty, report.runs.count <= 1000,
                      report.runs.allSatisfy({ $0.audio_open.count <= 1000 }) else { throw ReadError.invalid }
                records = []
                sections = report.runs.map { run in
                    let selected = Array(run.audio_open.dropFirst())
                    let rejected = selected.filter { $0.result != 0 }.count
                    let stereo = selected.filter { $0.result == 0 && $0.channels == 2 }.count
                    return Section(title: run.engine, lines: [
                        run.pass ? "Recorded run passed all open checks." : "Recorded run rejected \(rejected) of \(selected.count) padded default-stereo opens.",
                        "Selected opens: \(selected.count) • Rejected: \(rejected) • Stereo replies: \(stereo)"
                    ])
                }
                heading = "Saved audio observation"
                overview = "Binary SHA-256: \(report.nro_sha256.prefix(12)) • Queued frames: \(report.queued_frames). Recorded observation only; opening this file does not execute a guest."
            case "ounce-diagnose/replay-v1":
                let report = try decoder.decode(ReplayResult.self, from: data)
                records = []
                var lines = [
                    "Side: \(report.side) • Process status: \(report.status) (exit code \(report.exit_code))",
                    "Revision: \(report.core_commit) • Binary SHA-256: \(report.binary_sha256.prefix(12))",
                    report.matched ? "Recorded observations matched this replay." : "Recorded observations did not match this replay."
                ]
                if let err = report.observation_error { lines.append("Observation note: \(err)") }
                sections = [Section(title: "Bounded SDL2 replay", lines: lines)]
                heading = "Saved replay result"
                overview = "Timeout limit: \(Int(report.timeout_seconds)) s. This view shows a saved replay result and does not re-run Cargo."
            case "ounce-core/workbench-check-v1":
                let report = try decoder.decode(WorkbenchCheck.self, from: data)
                guard !report.engines.isEmpty, report.engines.count <= 32 else { throw ReadError.invalid }
                records = []
                var list = [Section(title: report.verdict.headline, lines: [report.verdict.detail])]
                if let ic = report.instructions {
                    list.append(Section(title: "Instruction check", lines: [
                        "Sampled instructions checked: \(ic.checked) • Mismatched: \(ic.mismatched)"
                    ]))
                }
                list += report.engines.map { eng in Section(title: eng.engine, lines: [
                    "Stop: \(eng.stop.kind) (\(eng.seconds) s)",
                    eng.stop.line.isEmpty ? "No stop message recorded." : eng.stop.line
                ]) }
                sections = list
                heading = "Saved workbench check"
                let title = report.name ?? "program.nro"
                let sha = report.sha256.map { " (\($0.prefix(12)))" } ?? ""
                overview = "Program: \(title)\(sha). Saved one-run comparison from the Ounce Core Workbench; not a proof of correctness."
            case "ounce-verify/retained-example-v1", "ounce-verify/audout-case-dossier-v1":
                throw ReadError.manifestSelected
            default: throw ReadError.invalid
            }
            guard overview.count <= 4096, sections.allSatisfy({ $0.title.count <= 512 && $0.lines.allSatisfy({ $0.count <= 4096 }) }) else { throw ReadError.invalid }
        } catch let error as ReadError {
            throw error
        } catch {
            throw ReadError.invalid
        }
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
