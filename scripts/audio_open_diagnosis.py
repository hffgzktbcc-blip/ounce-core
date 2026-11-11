"""Inspect the specific retained libnx #685 default-stereo padding dossier."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import tempfile

REPO = Path(__file__).resolve().parents[1]
EXAMPLE = REPO / 'docs/verify/audout-padding'
BINARY = '69106c5c4b0485abdbce1ca9f17395822c7c7643d15f57b11b9bfc994d7f252c'
IMAGE = 'sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282'
SOURCE = '55911e5910ba282330dd9ab78b3bf12f3b394e8f'
BASELINE = '0b8b38a35dd73a97209ee43ec818904720bd6432'
FILES = {'before.json', 'after.json', 'before.log', 'after.log', 'canonical-corpus.json'}
EXPECTED = {'requests': 7, 'raw_channels': 65536, 'result': 0, 'channels': 2}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def integer(value):
    return type(value) is int


def observations(document):
    require(document['schema_version'] == 'ounce-verify/audout-padding-v1', 'unsupported observation schema')
    require(document['nro_sha256'] == BINARY, 'wrong NRO identity')
    require(document['input'] == {'after_queued_frame': 3, 'buttons': 1}
            and all(integer(v) for v in document['input'].values()), 'changed audio case input checkpoint')
    require(integer(document['queued_frames']) and document['queued_frames'] == 5, 'changed frame boundary')
    require(document['expected'] == EXPECTED and all(integer(v) for v in document['expected'].values()),
            'changed registered expectations')
    require(len(document['runs']) == 2 and {r['engine'] for r in document['runs']} == {'Interpreter', 'Tier3'},
            'audio case requires exactly one record per engine')
    results = {}
    for run in document['runs']:
        flight = run['flight_record']
        require(flight['engine'] == run['engine'] and flight['stop'] == 'FrameLimit'
                and integer(flight['frames_queued']) and flight['frames_queued'] == 5
                and flight['first_failure'] is None, 'incomplete or incompatible audio case execution')
        require(integer(flight['instructions']) and flight['instructions'] > 0, 'missing guest instruction count')
        attempts = run['audio_open']
        require(len(attempts) == 8, 'audio case requires startup plus seven selected requests')
        for index, attempt in enumerate(attempts):
            require(attempt['input'] is False and integer(attempt['rate']) and attempt['rate'] == 48000,
                    'different audio device direction or sample rate')
            require(integer(attempt['raw_channels']) and attempt['raw_channels'] == (131072 if index == 0 else 65536),
                    'different audio case request sequence')
            require(integer(attempt['result']) and 0 <= attempt['result'] <= 0xFFFFFFFF,
                    'invalid audio case reply result')
            require((integer(attempt['channels']) and attempt['channels'] == 2) if attempt['result'] == 0
                    else attempt['channels'] is None, 'reply channels disagree with result')
        require(attempts[0]['result'] == 0, 'startup audio session failed')
        passed = all(a['result'] == 0 for a in attempts[1:])
        require(type(run['pass']) is bool and run['pass'] == passed, 'audio case pass claim contradicts replies')
        results[run['engine']] = attempts[1:]
    require(type(document['pass']) is bool and document['pass'] == all(r['pass'] for r in document['runs']),
            'audio case overall pass claim contradicts engines')
    return results


def read_case(root):
    try:
        require(root.is_dir(), f'evidence path is not a directory: {root}')
        manifest_path = root / 'manifest.json'
        require(manifest_path.resolve().is_relative_to(root.resolve()), 'audio case manifest path escapes directory')
        require(
            manifest_path.is_file(),
            f'missing manifest.json in {root} '
            '(omit --evidence to inspect the built-in audout-padding dossier)',
        )
        manifest_bytes = manifest_path.read_bytes()
        manifest = json.loads(manifest_bytes)
        require(manifest['schema_version'] == 'ounce-verify/audout-case-dossier-v1', 'unsupported dossier schema')
        require(set(manifest['files_sha256']) == FILES, 'unexpected or missing audio case evidence path')
        require(manifest['binary_sha256'] == BINARY and manifest['source']['commit'] == SOURCE
                and manifest['source']['repository'] == 'https://github.com/ITotalJustice/audout-channel-test'
                and manifest['source']['tracked_source_changed'] is False, 'different source or binary identity')
        require(manifest['toolchain']['image'] == IMAGE, 'different audio case build image')
        require(manifest['registration_commit'] in (BASELINE, BASELINE[:7]), 'different registration revision')
        require(re.fullmatch('[0-9a-f]{40}', manifest['repair_commit']) is not None, 'invalid repair revision')
        files = {'manifest.json': manifest_bytes}
        for name, expected in manifest['files_sha256'].items():
            path = root / name
            require(path.resolve().is_relative_to(root.resolve()), 'audio case evidence path escapes directory')
            data = path.read_bytes()
            require(hashlib.sha256(data).hexdigest() == expected, f'SHA-256 mismatch for {name}')
            files[name] = data
        before, after = (observations(json.loads(files[side + '.json'])) for side in ('before', 'after'))
        require(all(a['result'] == 0xF601 for rows in before.values() for a in rows),
                'before report does not contain the registered padding rejection')
        cases = [{'engine': engine, 'first_rejected_open': 1, 'raw_channels': 65536,
                  'low_channel_halfword': 0, 'padding_halfword': 1,
                  'rejected_before': sum(a['result'] != 0 for a in before[engine]),
                  'stereo_after': sum(a['result'] == 0 and a['channels'] == 2 for a in after[engine])}
                 for engine in ('Interpreter', 'Tier3')]
        report = {'schema_version': 'ounce-diagnose/audio-padding-v1', 'mode': 'offline-recorded-observations',
                  'binary_sha256': BINARY, 'engines': ['Interpreter', 'Tier3'], 'cases': cases,
                  'recorded_repair_matches': all(c['stereo_after'] == 7 for c in cases),
                  'explanation': 'The low channel halfword is zero (default stereo); the upper halfword is padding. '
                                 'The recorded manager rejected all seven padded words before the scoped repair.',
                  'likely_owner': 'IAudioOutManager channel validation',
                  'limits': 'Recorded protocol replies only; no fresh guest execution, binary verification, '
                            'six-channel claim, independent rerun or authenticated manifest.',
                  'files_sha256': {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
        return manifest, files, report
    except (OSError, ValueError, KeyError, TypeError, AttributeError, IndexError) as error:
        raise ValueError(f'audio case: {error}') from error


def write_audio_bundle(root, output):
    manifest, files, report = read_case(root)
    require(not output.exists() and not output.is_symlink(), f'audio case output already exists: {output}; choose a fresh directory')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.audio-case-', dir=output.parent) as temp:
        staging = Path(temp) / 'bundle'
        staging.mkdir()
        for name, data in files.items():
            (staging / name).write_bytes(data)
        shutil.copyfile(REPO / 'LICENSE', staging / 'LICENSE')
        (staging / 'diagnosis.json').write_text(json.dumps(report, indent=2) + '\n')
        (staging / 'NOTICE.md').write_text(
            '# Retained audio-case observation scope\n\n'
            'Repository-generated protocol observations, hashes and runtime text only, under the included MIT license. '
            'No upstream source, executable, linked library or captured frame pixels are included. '
            'The canonical corpus report preserves its historical byte-verification claims; this offline command '
            'does not re-verify absent binaries or certify current code. Hashes detect file changes relative to the '
            'manifest, not edits to a manifest and its files together. Preserve the source Git revision. '
            'Custom evidence and logs require a privacy/distribution review before sharing; no credential scrubber is provided.\n')
        (staging / 'REPLAY.md').write_text(
            '# Replay the selected audio padding interaction\n\n'
            'This dossier is offline inspection. It did not execute or authenticate the guest. '
            'Use macOS/Linux, Rust/Cargo and documented native build dependencies. '
            'The input stays local; no permission to redistribute it is inferred.\n\n'
            f'Source: {manifest["source"]["repository"]} at `{SOURCE}`, unmodified.\n\n'
            f'NRO SHA-256: `{BINARY}`. Build with `make -j2` under `/case` in image '
            f'`{manifest["toolchain"]["image"]}` as recorded in manifest.json. '
            'Newer toolchains may produce different bytes; verify the hash before execution.\n\n'
            f'Failure checkout: `{BASELINE}`. Repair checkout: `{manifest["repair_commit"]}`. '
            'Use separate clean worktrees and target directories for the two revisions.\n\n'
            '```sh\nexport OUNCE_AUDIO_CHANNEL_NRO=/path/to/local/case.nro\n'
            'export OUNCE_REPORT_JSON=/path/to/new-observation.json\n'
            'export CARGO_TARGET_DIR=/path/to/separate-build-target\n'
            'cargo test --release --locked loader::fixture_tests::corpus_runner::external_audout_padding_case '
            '-- --exact --ignored --nocapture --test-threads=1\n```\n\n'
            'Apply an outer process deadline as well as the 180-second per-engine deadline. '
            'The unchanged scenario presses A after frame 3 and stops at frame 5. '
            'Before repair the test exits 101 because all seven padded opens fail; after repair it exits 0. '
            'A build failure is not a reproduced guest failure. Raw JSON/logs must agree on both engines. '
            'The CLI replay subcommand remains SDL2-only; it does not execute this audio recipe.\n')
        staging.rename(output)
    return report
