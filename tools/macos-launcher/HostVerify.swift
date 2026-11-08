import Foundation

// Pure logic for "Verify against Host": identify a known harness program and compare its output with the
// committed host reference. The app never decides correctness itself beyond a byte-for-byte comparison.

struct Harness: Equatable {
    let name: String
    let dir: String
    let resultFile: String
    let arm64Reference: Bool
}

enum HarnessCatalog {
    private struct Document: Decodable {
        struct Entry: Decodable {
            let name: String
            let dir: String
            let resultFile: String?
            let arm64Reference: Bool?
            enum CodingKeys: String, CodingKey { case name, dir, resultFile = "result_file", arm64Reference = "arm64_reference" }
        }
        let harnesses: [Entry]
    }

    static func parse(_ data: Data) throws -> [Harness] {
        try JSONDecoder().decode(Document.self, from: data).harnesses.map {
            Harness(name: $0.name, dir: $0.dir, resultFile: $0.resultFile ?? "results.txt", arm64Reference: $0.arm64Reference ?? false)
        }
    }

    private static func normalise(_ text: String) -> String {
        text.lowercased().replacingOccurrences(of: "_", with: "-")
    }

    /// verify_differential.py names prebuilt NROs `<harness>.nro`; the directory name and a `-harness` suffix are accepted too.
    static func match(nroFile: String, in catalog: [Harness]) -> Harness? {
        let stem = normalise((nroFile as NSString).deletingPathExtension)
        return catalog.first { stem == normalise($0.name) || stem == normalise($0.dir) || stem == normalise($0.name) + "-harness" }
    }
}

struct ResultComparison: Equatable {
    let identical: Bool
    let expectedLines: Int
    let actualLines: Int
    let differingLines: Int
    /// 1-based number of the first line that differs, if any.
    let firstDifference: Int?
    /// Up to `sampleLimit` differing lines as (line number, expected, actual).
    let samples: [Sample]
    struct Sample: Equatable { let line: Int; let expected: String; let actual: String }

    static let sampleLimit = 5

    static func compare(expected: String, actual: String) -> ResultComparison {
        let want = lines(expected), got = lines(actual)
        var differing = 0
        var samples: [Sample] = []
        var first: Int?
        for index in 0..<max(want.count, got.count) {
            let a = index < want.count ? want[index] : "(missing)"
            let b = index < got.count ? got[index] : "(missing)"
            if a != b {
                differing += 1
                if first == nil { first = index + 1 }
                if samples.count < sampleLimit { samples.append(Sample(line: index + 1, expected: a, actual: b)) }
            }
        }
        return ResultComparison(identical: differing == 0, expectedLines: want.count, actualLines: got.count,
                                differingLines: differing, firstDifference: first, samples: samples)
    }

    private static func lines(_ text: String) -> [String] {
        var parts = text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        if parts.last == "" { parts.removeLast() }
        return parts
    }

    func summary(harness: Harness, engine: Engine, hostIsARM64: Bool) -> String {
        var out: String
        if identical {
            out = "Match. \(harness.name) on \(engine.rawValue) produced \(actualLines) lines, byte-identical to the committed host reference."
            out += "\n\nThis is one program on one set of inputs. It does not show the program is correct everywhere."
        } else {
            out = "Mismatch. \(differingLines) of \(max(expectedLines, actualLines)) lines differ from the committed host reference (reference \(expectedLines) lines, this run \(actualLines))."
            for s in samples {
                out += "\n\nLine \(s.line)\n  reference: \(s.expected)\n  this run:  \(s.actual)"
            }
            if differingLines > samples.count { out += "\n\n… and \(differingLines - samples.count) more." }
        }
        if harness.arm64Reference && !hostIsARM64 {
            out += "\n\nNote: this harness's reference was recorded on an ARM64 host, so a mismatch here may come from this Mac's CPU, not Ounce Core."
        }
        return out
    }
}
