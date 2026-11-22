#!/usr/bin/env python3
"""Run the versioned one-instruction seed through a trace adapter (no shell)."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COUNTS = {
    'host executed (not SIGILL)': 1, 'host UNDEFINED (SIGILL)': 0,
    'emulator unimplemented': 0, 'emulator other errors': 0,
    'emulator executed': 1, 'exact match (X,V,NZCV,QC)': 1,
    'MISMATCH': 0, 'emulator runs a word the host calls UNDEFINED': 0,
}


def verdict(stdout, returncode):
    counts = {}
    for label, expected in COUNTS.items():
        values = re.findall(r'^[ \t]*' + re.escape(label) + r':(.*)$', stdout, re.M)
        if len(values) != 1:
            return False, counts, 'missing or duplicate count: ' + label
        value = re.fullmatch(r'[ \t]*([0-9]+)(?:[ \t].*)?', values[0])
        if value is None:
            return False, counts, 'malformed count: ' + label
        counts[label] = int(value[1])
    passed = returncode == 0 and counts == COUNTS
    return passed, counts, 'pass' if passed else 'adapter reported a nonpass'


def validate_trace(raw, word, expected_fpcr=None):
    """Validate a single-instruction register-state trace (`T` for FPCR=0, `TF <fpcr>` for non-default FPCR)."""
    fields = raw.decode('ascii').split()
    if not fields:
        raise ValueError('expected one T or TF register-state instruction')
    if fields[0] == 'T':
        if len(fields) != 132 or fields[66] != '=>':
            raise ValueError('expected one T register-state instruction')
        trace_fpcr = 0
        trace_word = fields[1].lower()
        values = fields[2:66] + fields[67:]
    elif fields[0] == 'TF':
        if len(fields) != 133 or fields[67] != '=>':
            raise ValueError('expected one TF register-state instruction')
        if not re.fullmatch(r'[0-9a-fA-F]{1,8}', fields[1]):
            raise ValueError('invalid FPCR field')
        trace_fpcr = int(fields[1], 16)
        if trace_fpcr == 0:
            raise ValueError('TF trace must have non-zero FPCR')
        trace_word = fields[2].lower()
        values = fields[3:67] + fields[68:]
    else:
        raise ValueError('expected one T or TF register-state instruction')
    if not re.fullmatch(r'[0-9a-f]{8}', word) or trace_word != word:
        raise ValueError('trace word differs from manifest')
    if expected_fpcr is None:
        if trace_fpcr != 0:
            raise ValueError('non-zero FPCR trace requires manifest fpcr field')
    else:
        if not isinstance(expected_fpcr, str) or not re.fullmatch(r'[0-9a-f]{8}', expected_fpcr):
            raise ValueError('invalid manifest fpcr field')
        if trace_fpcr != int(expected_fpcr, 16):
            raise ValueError('trace FPCR differs from manifest')
    widths = [4] + [64] * 31 + [128] * 32 + [4, 32] + [64] * 31 + [128] * 32
    for value, width in zip(values, widths):
        if not re.fullmatch(r'[0-9a-fA-F]+', value) or int(value, 16) >= 1 << width:
            raise ValueError('invalid register-state field')
    return f'{trace_fpcr:08x}'


def run(suite, command):
    raw = (suite / 'suite.json').read_bytes()
    manifest = json.loads(raw)
    if manifest['schema'] not in ('ounce-core/conformance-v1', 'ounce-core/conformance-v2') or not manifest['cases']:
        raise ValueError('unsupported or empty suite')
    validated = []
    ids = set()
    for case in manifest['cases']:
        if case['id'] in ids:
            raise ValueError('duplicate case id')
        ids.add(case['id'])
        path = (suite / case['path']).resolve()
        if not path.is_relative_to(suite.resolve()):
            raise ValueError('case path escapes suite')
        trace = path.read_bytes()
        if hashlib.sha256(trace).hexdigest() != case['sha256']:
            raise ValueError('case hash mismatch: ' + case['id'])
        fpcr_hex = validate_trace(trace, case['word'], case.get('fpcr'))
        validated.append((case, path, fpcr_hex))
    observations = []
    for case, path, fpcr_hex in validated:
        result = subprocess.run([*command, str(path)], capture_output=True, text=True, timeout=60)
        passed, counts, reason = verdict(result.stdout, result.returncode)
        observations.append({'id': case['id'], 'trace_sha256': case['sha256'],
                             'fpcr': fpcr_hex,
                             'passed': passed, 'reason': reason, 'counts': counts,
                             'returncode': result.returncode, 'stdout': result.stdout,
                             'stderr': result.stderr})
    return {'schema': 'ounce-core/conformance-result-v1', 'suite_version': manifest['version'],
            'suite_manifest_sha256': hashlib.sha256(raw).hexdigest(), 'adapter_argv': command,
            'scope': 'adapter-reported internal result; independence not established',
            'cases': observations, 'passed': all(o['passed'] for o in observations)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', type=Path, default=ROOT / 'conformance/arm64-v0.1.0')
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('adapter', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.adapter[1:] if args.adapter[:1] == ['--'] else args.adapter
    if not command:
        parser.error('provide the trace adapter command after --')
    try:
        if args.report.exists():
            raise ValueError('report already exists; preserve prior evidence')
        report = run(args.suite, command)
        with args.report.open('x') as stream:
            json.dump(report, stream, indent=2)
            stream.write('\n')
        print('PASS' if report['passed'] else 'NONPASS', args.report)
        return 0 if report['passed'] else 1
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
