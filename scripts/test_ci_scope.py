import unittest
import json
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import ci_scope
from ci_scope import classify


class ScopeTest(unittest.TestCase):
    def test_metadata_does_not_need_rust_or_corpus(self):
        result = classify(['PLAN.md', 'AGENTS.md', 'README.md', 'docs/guide.md'])
        self.assertFalse(result['runtime'])
        self.assertFalse(result['corpus'])

    def test_fixture_notices_and_records_do_not_need_rust_but_binaries_do(self):
        for path in ['tests/fixtures/nro/NOTICES.md', 'tests/fixtures/provenance.json',
                     'tests/fixtures/nro/hello/README.md']:
            with self.subTest(path=path):
                self.assertFalse(classify([path])['runtime'])
        for path in ['tests/fixtures/nro/hello/hello.nro', 'tests/fixtures/nro/hello/main.c',
                     'tests/fixtures/nro/hello/Makefile']:
            with self.subTest(path=path):
                self.assertTrue(classify([path])['runtime'])

    def test_runtime_and_policy_changes_keep_full_regression(self):
        for path in ['src/cpu/decoder.rs', 'crates/ounce-nce/src/exec.rs', 'Cargo.lock',
                     'tests/corpus/oracles.json', '.github/workflows/ci.yml', 'scripts/ci_scope.py']:
            with self.subTest(path=path):
                result = classify([path])
                self.assertTrue(result['runtime'])
                self.assertTrue(result['corpus'])

    def test_workflow_tools_and_demo_changes_run_relevant_checks(self):
        self.assertFalse(classify(['scripts/agent_workflow.py'])['runtime'])
        self.assertTrue(classify(['scripts/corpus_verify.py'])['corpus'])
        self.assertTrue(classify(['tools/macos-launcher/main.swift'])['runtime'])

    def test_compiled_font_changes_and_deletions_require_runtime_and_corpus(self):
        scope = classify(['assets/fonts/DejaVuSans.ttf'])
        self.assertTrue(scope['runtime'])
        self.assertTrue(scope['corpus'])

    def test_headline_changes_need_corpus_but_prose_changes_do_not(self):
        before = '<!-- corpus-headline -->50/50<!-- /corpus-headline --> first--frame_oracles-50%2F50_pass'
        self.assertFalse(ci_scope.headline_changed(before, before + '\nBetter instructions.'))
        self.assertTrue(ci_scope.headline_changed(before, before.replace('50/50', '49/50')))
        self.assertTrue(ci_scope.headline_changed(before, before.replace('50%2F50', '49%2F50')))
        self.assertTrue(ci_scope.headline_changed(before, 'markers removed'))

    def test_required_ci_summaries_accept_only_applicable_successes(self):
        workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/ci.yml').read_text()
        script = textwrap.dedent(workflow.split("python3 - <<'PYTHON'\n", 1)[1].split('          PYTHON', 1)[0])
        results = {name: {'result': 'skipped'} for name in
                   ('changes', 'legal-guardrails', 'tooling', 'release-notice-packaging', 'test', 'gpu-present', 'native-jit', 'nce', 'corpus')}
        for name in ('changes', 'legal-guardrails', 'tooling', 'release-notice-packaging'):
            results[name]['result'] = 'success'
        results['changes']['outputs'] = {'runtime': 'false', 'corpus': 'false'}

        def run():
            return subprocess.run([sys.executable, '-c', script], capture_output=True, text=True,
                                  env=dict(os.environ, RESULTS=json.dumps(results))).returncode

        self.assertEqual(run(), 0)  # docs: heavy jobs intentionally skipped
        results['release-notice-packaging']['result'] = 'failure'
        self.assertNotEqual(run(), 0)
        results['release-notice-packaging']['result'] = 'success'
        results['tooling']['result'] = 'failure'
        self.assertNotEqual(run(), 0)
        results['tooling']['result'] = 'success'
        results['changes']['outputs'] = {'runtime': 'true', 'corpus': 'true'}
        self.assertNotEqual(run(), 0)  # skipped runtime/corpus is not a pass
        for job in results:
            results[job]['result'] = 'success'
        self.assertEqual(run(), 0)
        results['corpus']['result'] = 'cancelled'
        self.assertNotEqual(run(), 0)


if __name__ == '__main__':
    unittest.main()
