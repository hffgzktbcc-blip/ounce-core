# Reproduce the registered RomFS frames

Build only the unchanged outside `switch-examples/fs/romfs` sample, run its existing
whole-frame oracle on Interpreter and Tier-3, then validate two canonical
Ounce-Verify records against actual NRO bytes. This experiment does not run or
change the 50-program headline calculation.

**Status: internal recipe; independent reproduction and legal review pending.**
The [reference manifest](reproduce/romfs.json) identifies retained CI evidence,
not a promise of byte-identical builds. Upstream declares Public Domain Mark 1.0;
linked libnx/newlib component terms require review before binary distribution.
See [provenance](PROVENANCE.md) and the [tester protocol](INDEPENDENT_TESTER_CHALLENGE.md).
No bundled fixture, private resource, key or proprietary input is needed.

## Prerequisites and pinned checkout

Linux amd64, Bash, Docker, Git, Python 3.9+, Rust stable/Cargo, `libasound2-dev`
and `libudev-dev`. Debian/Ubuntu: `sudo apt-get install libasound2-dev libudev-dev`.
Allow space for the container and Rust release build. Network access to GitHub,
Docker Hub and image package repositories is needed. Ten to fifteen minutes is a
usability target, not a measured guarantee. ARM container emulation is outside
this recipe's build qualification.

Run all sections in **one Bash shell**, from a writable directory. Outputs go
outside the pinned checkout. The guide may be viewed separately from that checkout.

```bash
set -euo pipefail
export REPRO_OUT="$(mktemp -d "$PWD/romfs-repro.XXXXXX")"
export REPRO_CORE=4dba991f323d93c7702d09a4a79eae4918aa0fed
export REPRO_IMAGE=devkitpro/devkita64@sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282
mkdir -p "$REPRO_OUT/evidence" "$REPRO_OUT/builds/original"
git clone https://github.com/hffgzktbcc-blip/ounce-core.git "$REPRO_OUT/core"
cd "$REPRO_OUT/core"
git checkout --detach "$REPRO_CORE"
git diff --exit-code
{
  git rev-parse HEAD
  uname -sm
  rustc -Vv
  cargo -V
  docker --version
  python3 --version
} > "$REPRO_OUT/evidence/host.txt"
python3 - <<'PY'
import json, os
from pathlib import Path
manifest = json.loads(Path('tests/corpus/manifest.json').read_text())
manifest['samples'] = [s for s in manifest['samples'] if s['id'] == 'romfs']
assert len(manifest['samples']) == 1
p = Path(os.environ['REPRO_OUT']) / 'evidence/build-manifest.json'
p.write_text(json.dumps(manifest, indent=2) + '\n')
PY
```

## Build one outside program

The repository mount is read-only. Python/Git/Make installation follows existing
CI; package versions are retained, but package repositories are not frozen.
The image digest alone therefore does not guarantee a hermetic build.

```bash
docker pull --platform linux/amd64 "$REPRO_IMAGE" \
  2>&1 | tee "$REPRO_OUT/evidence/pull.log"
docker image inspect "$REPRO_IMAGE" > "$REPRO_OUT/evidence/toolchain-image.json"
docker run --rm --platform linux/amd64 \
  -v "$PWD:/repo:ro" -v "$REPRO_OUT/builds:/corpus" \
  -v "$REPRO_OUT/evidence:/evidence" "$REPRO_IMAGE" bash -lc '
    set -euo pipefail
    apt-get update
    apt-get install -y python3 git make
    dkp-pacman -Q > /evidence/toolchain-packages.txt
    python3 /repo/scripts/prepare_external_corpus.py \
      --manifest /evidence/build-manifest.json \
      --output /corpus/original/build --build
  ' 2>&1 | tee "$REPRO_OUT/evidence/build.log"
sha256sum "$REPRO_OUT/builds/original/build/source/fs/romfs/romfs.nro" \
  | tee "$REPRO_OUT/evidence/nro.sha256"
```

Reference NRO SHA-256:
`d22f38b633df556c00ffecf1f435d96e63ccd69352d1f2f4e10cf316de590fb9`.
Record a differing digest as a rebuild discrepancy; RomFS timestamps can differ.
Keep the actual new binary identity and run its unchanged frame oracle. Do not
substitute the reference digest, change expectations or call a differing rebuild
byte-identical.

## Compile and run both engines

Use the same exact test as the corpus runner. Compilation is bounded to 20 minutes;
the outer 480-second timeout bounds both 180-second engine runs plus overhead.
Inherited `OUNCE_*` settings are removed to prevent engine/input overrides.

```bash
python3 - <<'PY'
import json, os, subprocess
from pathlib import Path
out = Path(os.environ['REPRO_OUT'])
e = out / 'evidence'
env = {k: v for k, v in os.environ.items() if not k.startswith('OUNCE_')}
with (e / 'compile.jsonl').open('w') as log, (e / 'compile.log').open('w') as err:
    subprocess.run(['cargo', 'test', '--locked', '--release', '--no-run',
                    '--message-format=json'], env=env, stdout=log, stderr=err,
                   timeout=1200, check=True)
messages = [json.loads(line) for line in (e / 'compile.jsonl').read_text().splitlines()]
executables = [m['executable'] for m in messages
               if m.get('reason') == 'compiler-artifact' and m.get('executable')
               and m.get('profile', {}).get('test') and m['target']['name'] == 'ounce-core']
assert len(executables) == 1, executables
env.update(OUNCE_CORPUS_ROOT=str(out / 'builds'), OUNCE_ORACLE_ID='romfs',
           OUNCE_ORACLE_RESULT=str(e / 'romfs.oracle.json'), OUNCE_ORACLE_SECONDS='180')
test = 'loader::fixture_tests::corpus_runner::corpus_first_frame_oracle'
with (e / 'romfs.oracle.log').open('w') as log:
    try:
        done = subprocess.run([executables[0], test, '--exact', '--ignored',
                               '--nocapture', '--test-threads=1'], env=env,
                              stdout=log, stderr=subprocess.STDOUT, timeout=480)
        status = 'exited-zero' if done.returncode == 0 else f'exited-{done.returncode}'
    except subprocess.TimeoutExpired:
        status = 'outer-timeout'
(e / 'host-status.json').write_text(json.dumps({'host_status': status}) + '\n')
print(status, '— retain all evidence, including failures')
PY
```

## Canonical evidence and byte validation

Use the existing adapter's `records_for` on the unchanged registry entry, without
making a smaller replacement registry. Run this section even after a nonzero test
exit. Missing observations mean an unsuccessful/inconclusive experiment.

```bash
PYTHONPATH=scripts python3 - <<'PY'
import json, os, subprocess
from pathlib import Path
from corpus_verify import records_for
out = Path(os.environ['REPRO_OUT'])
e = out / 'evidence'
oracle = next(o for o in json.loads(Path('tests/corpus/oracles.json').read_text())['oracles']
              if o['id'] == 'romfs')
inventory = json.loads((out / 'builds/original/build/inventory.json').read_text())
verdict = json.loads((e / 'romfs.oracle.json').read_text())
status = json.loads((e / 'host-status.json').read_text())['host_status']
verdict.update(host_status=status, host_timeout_seconds=180)
assert status == 'exited-zero' or not verdict['pass'], 'inconsistent process result; retain as invalid evidence'
image = json.loads((e / 'toolchain-image.json').read_text())[0]['Id']
tools = inventory['tools']
libnx = inventory['toolchain_inputs_sha256']['libnx/lib/libnx.a']
toolchain = f"{tools['compiler']}; devkitpro/devkita64 image {image}; libnx.a sha256 {libnx}"
core = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
assert core == os.environ['REPRO_CORE']
records = records_for(oracle, verdict, inventory, toolchain, core)
for record in records:
    record['artifacts'] = [{'role': 'binary',
        'path': 'original/build/source/fs/romfs/romfs.nro',
        'sha256': record['source']['binary_sha256']}]
(e / 'records.json').write_text(json.dumps(records, indent=2) + '\n')
PY
python3 tools/ounce_verify.py "$REPRO_OUT/evidence/records.json" \
  --artifact-root "$REPRO_OUT/builds" \
  --output "$REPRO_OUT/evidence/canonical-report.json"
python3 - <<'PY'
import json, os
from pathlib import Path
p = Path(os.environ['REPRO_OUT'])
records = json.loads((p / 'evidence/canonical-report.json').read_text())['records']
assert len(records) == 2
assert {r['case_id'] for r in records} == {'corpus/romfs/interpreter', 'corpus/romfs/tier3'}
assert all(r['verdict'] == 'pass' and r['artifact_integrity'] == 'verified' for r in records)
print('Two byte-checked registered frame passes; independent status requires tester disclosure.')
PY
```

Expected: both engines stop at `FrameLimit`, queue five 1280×720 frames, and match
the registered whole-frame hash `0x3bd649bbc4f50b85` on frames 1–5. Agreement
between engines alone is insufficient. A validator exit of zero establishes valid
evidence, not necessarily passing frames; the final assertions check pass verdicts.

## Failures, limitations and sharing

- No Docker/image/network: preserve pull/build logs and report blocked. Do not
  replace the pinned image with `latest`.
- Compilation error/timeout: retain `compile.log` and `compile.jsonl`.
- Mismatch, timeout or missing report: retain raw observations, report failure or
  inconclusive, and never edit the registered expectations to obtain a pass.
- This tests bounded software-rendered startup text, not physical GPU execution,
  later interaction, audio, storage persistence or broad compatibility.
- This recipe has not been rebuilt end to end on a clean Linux host. Local recipe
  qualification and an unaffiliated tester's rerun must be reported separately.

Retain build `inventory.json`, package/image metadata, build logs, host/toolchain
versions, raw engine observations and canonical records. Redact usernames, private
paths, tokens and proxy details in public copies. Do not upload source checkouts or
binaries until redistribution terms are reviewed. Submit sanitized evidence with
the [tester template](INDEPENDENT_TESTER_CHALLENGE.md#submit-evidence), then link it
in [issue #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190). Disclose
independence; an agent or maintainer run does not close PLAN step 15.
