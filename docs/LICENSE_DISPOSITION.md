# Licensing disposition — 8 November 2026

This is the current engineering/notice disposition, not a lawyer's opinion or a
new grant from a third-party rights holder. It supersedes blanket statements that
the earlier inventory or the merged controls have not been performed.

| Material | Disposition and evidence | What remains |
| --- | --- | --- |
| Project-authored source | Project MIT licence in `LICENSE`; retained port attributions in `THIRD_PARTY.md` | The project licence does not relicense third-party material |
| Current default-runtime dependencies | 169 retained notice records for the locked graph; supplemented Objective-C MIT grant and attribution in `licenses/objc2-MIT.txt` and the runtime archive | Preserve the upstream SDK qualification below; do not call an overview alone a complete MIT notice |
| Rust standard/compiler runtime | Release packaging now requires binary-bound notices captured from the actual installed build toolchain, including MIT, Apache and the supplied library/whole-toolchain copyright inventory | Each release build must collect and verify its own notices; no generic licence label or another toolchain's record can substitute |
| Linux system-library notices | v0.6.0 preparation retains actual installed package copyright records and common license texts, bound to the built ELF; LGPL-2.1 included, missing coverage rejected | Shared libraries are OS-provided and not copied into the archive; build-host identity is not an end-user-library version guarantee |
| Bundled DejaVu font | Full retained font notice included in both runtime archive formats by merged PR #203 | Existing published v0.5.0 archives have not been replaced by these changes |
| Standalone Python toolkit 0.2.0 | Published from its reviewed 30-file parentless snapshot; contains project Python and observation data, no runtime, SDK, NRO, fonts or historical tree | Separate release scope; it is not blocked by runtime/fixture history questions |
| Eleven legacy NRO fixtures | All eleven have registered identities and byte-identical rebuild evidence in `tests/fixtures/`; used as test inputs, not included in the runtime packager's explicit archive entries | Rebuild identity does not prove source authorship or all linked-component redistribution rights; GitHub's full-repository source archives still contain these inputs |
| Historical firmware-labelled candidate objects | The later object-level inventory isolates twelve arrays and two standalone objects to retained PR #1; current source and the inspected release archives had no exact matches | Host-managed PR reference treatment remains separate; no public purge is claimed |
| Historical 72-byte outside program | Re-identified from the old source attribution as `pepsipu/ns2jb` `engine/switch2/process/test-function.bin`; the recorded permission search found no grant | [Exact identity, local reachability scope and unsent permission-request draft](permissions/NS2JB_REQUEST_DRAFT.md); no unknown licence is converted to MIT and no history is rewritten |

## Objective-C notices and the SDK distinction

The exact locked objc2 revisions declare MIT for `objc2`, `block2`,
`objc2-foundation` and `objc2-encode`, and a MIT option for the other framework
crates. The root `LICENSE.md` retained in the unchanged aggregate is a licensing
overview rather than the full MIT grant. The supplement retains Steven Sheldon's
original rust-objc MIT notice and records Mads Marquart and project-contributor
attribution from the pinned workspace. The later upstream full MIT text from
commit `ee9a7ada` is retained verbatim in `licenses/objc2-upstream-MIT.txt` and
the runtime archive as well. It does not change the upstream licences.

Sources:

- [Pinned objc2 licensing overview](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md)
- [Pinned workspace attribution and licence declarations](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/Cargo.toml)
- [Upstream full MIT text](https://github.com/madsmtm/objc2/blob/ee9a7ada2131f5944b8750428e265c15632f2a19/LICENSE-MIT.txt)
- [Inherited original MIT notice](https://github.com/SSheldon/rust-objc/blob/9961247c1a82027d6edbe6c516011b1363b9354c/LICENSE.txt)

In [upstream issue #826](https://github.com/madsmtm/objc2/issues/826#issuecomment-6103078978),
the maintainer confirms that older crate versions have their declared licences.
The [follow-up](https://github.com/madsmtm/objc2/issues/826#issuecomment-6107377999)
does not confirm completeness of a notice naming only that maintainer because
other contributors exist. Both the upstream text and inherited notice are
therefore retained; no claim of exhaustive contributor clearance is made.

The upstream overview expressly says that SDK-derived bindings may have separate
licensing implications. Apple's [Xcode and Apple SDKs Agreement](https://www.apple.com/legal/sla/docs/xcode.pdf),
sections 2.2 and 2.4, distinguishes macOS application distribution from distributing
SDK material or other-platform applications. Section 2.4 allows compliant macOS
applications/libraries to be distributed without a separate agreement. That does
not itself settle the provenance of each generated binding. The runtime packager
does not distribute SDK headers, framework libraries or Xcode. Adding MIT text is
not an Apple permission determination, and this record does not pretend otherwise.

## Rust notices are tied to builds

Rust's [copyright record](https://github.com/rust-lang/rust/blob/main/COPYRIGHT)
explains the per-release `COPYRIGHT.html` and standard-library-only
`COPYRIGHT-library.html` inventories. The collector uses files from the installed
sysroot used by `rustc`, retains their original bytes, records `rustc -Vv`, target,
hashes and the binary hash, and omits local sysroot paths. A whole-toolchain
copyright file is retained conservatively when a library-only file is unavailable.
Both the older top-level notice layout and current REUSE-managed
`share/doc/rust/licenses/*.txt` layout are supported. Every supplied REUSE
licence is retained with its relative path, with exact MIT/Apache aliases for
consistent archive validation. Missing notices fail closed. The release workflow installs the matching Rust docs
component; both Unix and Windows packaging pass the collected directory explicitly.

```sh
python scripts/collect_toolchain_notices.py --binary PATH_TO_BINARY \
  --target EXACT_TARGET --output FRESH_NOTICE_DIRECTORY
python scripts/package_release.py --binary PATH_TO_BINARY --output release.zip \
  --toolchain-notices FRESH_NOTICE_DIRECTORY --require-reviewed
```

The publication review gate remains intact. Tests use a clearly labelled synthetic
review fixture; no test result or agent name is substituted for an actual
recorded owner distribution decision or completed rights review. Notice/binary tampering, missing copyright coverage and traversal
paths are rejected before an archive is written.

## Historical records are evidence, not recurring work orders

PR #203 and its final hosted qualification implemented fixture identity controls,
dependency/font notice packaging and sensitive-path review requests. PR #205
integrated those controls and passed all 33 candidate checks. Claims in the older
October report that these inventories are absent are superseded by those results.

The dated object-level report narrows the historical case substantially. It does
not justify rewriting all main/tag history based only on reachability of an old
commit. No rewrite, asset deletion/replacement, Support message or third-party
contact was performed here. Preserve the existing private evidence and use exact
object/ref scope for any later approved action. Do not reopen the broad audit
without new material evidence or a relevant distribution change.

## Normal release qualification — 9 November 2026

The v0.6.0 candidate passed release testing and archive verification on Linux
x86-64, macOS arm64 and Windows x86-64 in run 42569508955. PR #273 passed all
40 checks and merged. Exact reports are under
[the release evidence directory](releases/v0.6.0-qualification/). This satisfies
the recorded technical artifact-qualification item; the owner subsequently delegated the documented distribution choice on
9 November 2026. The [recorded decision](releases/v0.6.0-distribution-decision.md)
selects the normal default-feature CLI release with all upstream qualifications
retained. This is an owner-directed decision, not a specialist legal opinion or
new third-party grant. Actual publication is established separately by the
completed tag workflow and release assets.

Linux publication additionally requires the installed native-library notice
inventory. On the same Debian/Ubuntu host that built the trusted executable:

```sh
python scripts/collect_native_notices.py --binary PATH_TO_BINARY \
  --output FRESH_NATIVE_NOTICE_DIRECTORY
python scripts/package_release.py --binary PATH_TO_BINARY --output release.tar.gz \
  --toolchain-notices FRESH_RUST_NOTICE_DIRECTORY \
  --native-notices FRESH_NATIVE_NOTICE_DIRECTORY --require-reviewed
```

The collector uses `ldd` only on our trusted build, resolves installed package
owners and retains their actual copyright records and common license files.
It does not copy the shared libraries. The package verifier checks the runtime
hash, required coverage, notice bytes and path containment. A recorded distribution decision with actual reviewer/evidence and passing
artifact checks is still required for the final command to succeed.
