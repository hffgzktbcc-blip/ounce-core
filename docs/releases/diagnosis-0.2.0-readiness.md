# Diagnosis toolkit 0.2.0 release acceptance

Scope: the demonstrated SDL2 retained-report walkthrough and external audio-padding
case, distributed as Python tooling plus repository-generated observation data.
No runtime binary, NRO, upstream source, linked library, font or frame pixels are
part of these assets. E1/E2 and the existing runtime releases remain separate.

`package_diagnosis_release.py` requires a clean committed checkout and copies an
explicit set of Python modules, reviewed reports/notices, the MIT license, a
standalone README and a read-only qualification workflow. It preserves original
source-file digests and the full original commit in SOURCE.json. Unlisted runtime
files never enter the snapshot. GitHub's automatic release source archive will
point at a separate standalone source commit with no runtime tree or parent
history; main and all existing tags remain untouched.

Qualification completed on 2026-11-08:

- [Audio repair CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42447339218) passed all 25 jobs.
- [Final CLI candidate CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42447902066) passed all 25 jobs.
- A clean standalone source export builds the CLI and runs both retained cases.
- [Standalone qualification](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42448502882) passed four jobs: Ubuntu/Python 3.9.25 and 3.13.17, macOS/Python 3.13.17 and Windows/Python 3.13.17. Each built/tested the isolated source and ran both examples.
- Release archive contents match SOURCE.json and the selected source revision.
- Draft assets were downloaded and matched SHA256SUMS; both copied-archive diagnoses passed before publication. Published assets match the same bytes, and GitHub's automatic source archive matches the exact 30-file standalone tree.

[Toolkit 0.2.0 is published](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/diagnosis-v0.2.0) under `diagnosis-v0.2.0`, outside the runtime's `v*` publishing trigger. The tag points at parentless standalone commit `03873727e785c6c5fcd85ec6919124a0868df8f1`, built from `dfc951fe701a6f54b3714ab48ae2aa304bc5a3ad`. The runtime `v0.5.0` remains Latest and main remains `9e813b984930f6280b1a23ac2d3a8ed9d9f5d2e7`. [Publication proof](diagnosis-0.2.0-proof.json) retains asset identities and checks.

If an asset hash or either diagnosis fails, keep the release in draft and repair
before publication. If a published toolkit issue is found, document it and issue
a corrected version; do not overwrite a versioned asset or alter runtime releases.
No on-call notification, maintainer outreach, adoption or independent validation
is part of this internal qualification. No external feedback has been received.

Reproduction from the full development checkout:

```sh
python3 -B scripts/package_diagnosis_release.py --output target/diagnosis-release
```

The resulting `source/` is the standalone distribution tree. It can build/test the
Python archive without the rest of the project. Its copied observation notices
and generated dossier notices describe the limited distribution scope; they do
not resolve historical repository or linked-executable permission questions.
