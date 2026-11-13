#!/usr/bin/env python3
"""Unit tests for ci_explainer helper."""
import unittest

from ci_explainer import (
    EXPLAINER_MARKER,
    determine_repro_command,
    explain_failure_with_model,
    extract_relevant_log_lines,
    format_ci_comment,
    strip_ansi,
)


class TestCiExplainer(unittest.TestCase):

    def test_strip_ansi(self):
        text = "\x1B[31mError:\x1B[0m command failed\x1B[2K"
        self.assertEqual(strip_ansi(text), "Error: command failed")

    def test_extract_relevant_log_lines_with_error_marker(self):
        log = (
            "Line 1\n"
            "Line 2\n"
            "##[error]Process completed with exit code 101.\n"
            "panic: assertion failed\n"
            "Trailing line\n"
        )
        snippet = extract_relevant_log_lines(log)
        self.assertIn("##[error]Process completed with exit code 101.", snippet)
        self.assertIn("panic: assertion failed", snippet)

    def test_extract_relevant_log_lines_empty(self):
        self.assertEqual(extract_relevant_log_lines(""), "No log output captured.")

    def test_determine_repro_command(self):
        # Quick check for tooling / guardrails / legal
        self.assertEqual(
            determine_repro_command("Workflow and evidence tooling", "Check local workflow and evidence tools"),
            "scripts/gate.sh --check quick"
        )
        self.assertEqual(
            determine_repro_command("Legal guardrails", "Fail on key files"),
            "scripts/gate.sh --check quick"
        )

        # Runtime checks
        self.assertEqual(
            determine_repro_command("Runtime (ubuntu-latest)", "Run unit tests"),
            "scripts/gate.sh --check runtime"
        )
        self.assertEqual(
            determine_repro_command("Runtime (macos-latest)", "Run 25-stage integration verification suite"),
            "scripts/gate.sh --check runtime"
        )

        # Full checks for feature flags
        self.assertEqual(
            determine_repro_command("Feature tests", "Test gpu-present"),
            "scripts/gate.sh --check full"
        )

        # Default fallback
        self.assertEqual(
            determine_repro_command("Other job", "Other step"),
            "scripts/gate.sh"
        )

    def test_explain_failure_fallback_without_token(self):
        explanation = explain_failure_with_model("Runtime (ubuntu-latest)", "Run unit tests", "cargo test failed", None)
        self.assertIn("Run unit tests", explanation)
        self.assertIn("Runtime (ubuntu-latest)", explanation)

    def test_format_ci_comment_contents_and_no_emoji(self):
        comment = format_ci_comment(
            job_name="Runtime (ubuntu-latest)",
            step_name="Run unit tests",
            log_snippet="cargo test exited with 101\nassertion `left == right` failed",
            repro_command="scripts/gate.sh --check runtime",
            explanation="The unit test suite failed because an assertion failed in the kernel module.",
            head_sha="abcdef123456"
        )
        self.assertIn(EXPLAINER_MARKER, comment)
        self.assertIn("## CI Failure Explanation for commit `abcdef1`", comment)
        self.assertIn("- **Failed Job**: Runtime (ubuntu-latest)", comment)
        self.assertIn("- **Failing Step**: Run unit tests", comment)
        self.assertIn("- **Local Reproduction**: `scripts/gate.sh --check runtime`", comment)
        self.assertIn("### Explanation", comment)
        self.assertIn("### Relevant Log Output", comment)

        # Verify no emoji
        for char in comment:
            code = ord(char)
            self.assertFalse(0x1F300 <= code <= 0x1FAD6 or 0x1F600 <= code <= 0x1F64F)


if __name__ == "__main__":
    unittest.main()
