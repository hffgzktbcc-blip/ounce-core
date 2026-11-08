# Diagnosis toolkit 0.2.0 release readiness

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

Qualification before publication:

- Existing runtime/audio repair candidate remains covered by its hosted run.
- The final CLI candidate's applicable hosted checks must pass.
- A clean standalone source export builds the CLI and runs both retained cases.
- The standalone source branch runs the focused package tests and both examples
  on Ubuntu/Python 3.9 and 3.13, macOS/Python 3.13 and Windows/Python 3.13.
- Release archive contents match SOURCE.json and the selected source revision.
- A draft release's downloaded code/source assets match SHA256SUMS and pass a
  fresh copied-archive diagnosis before the release is made public.

Use a `diagnosis-v0.2.0` tag, outside the runtime's `v*` publishing trigger, and
leave the runtime release marked Latest. The source snapshot has its own isolated
release branch. Qualification evidence links and published asset identities are
recorded after the checks complete; this document does not claim they passed yet.

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
