#!/usr/bin/env python3
"""Unit tests for issue_triage helper."""
import unittest

from issue_triage import (
    TRIAGE_MARKER,
    check_issue_fields,
    detect_problem_report,
    find_possible_duplicates,
    format_triage_comment,
    parse_issue_form_sections,
    rule_based_label_suggestions,
)


class TestIssueTriage(unittest.TestCase):

    def test_parse_issue_form_sections(self):
        body = (
            "### Program and version\n\n"
            "hello-world v1.0.0\n\n"
            "### Exact command\n\n"
            "ounce-core run-nro hello.nro\n\n"
            "### What Ounce-Core did\n\n"
            "Crash at 0x80000\n"
        )
        sections = parse_issue_form_sections(body)
        self.assertEqual(sections.get("Program and version"), "hello-world v1.0.0")
        self.assertEqual(sections.get("Exact command"), "ounce-core run-nro hello.nro")
        self.assertEqual(sections.get("What Ounce-Core did"), "Crash at 0x80000")

    def test_check_issue_fields_all_present(self):
        body = (
            "### Program and version\n\n"
            "hello-world.nro commit 1234abc\n\n"
            "### Exact command\n\n"
            "ounce-core run-nro hello-world.nro\n\n"
            "### What Ounce-Core did\n\n"
            "[VM Error] Unhandled SVC 0x18\n"
        )
        missing, details = check_issue_fields("Emulation bug", body)
        self.assertEqual(missing, [])
        self.assertTrue(details["program"])
        self.assertTrue(details["version"])
        self.assertTrue(details["command"])
        self.assertTrue(details["output"])

    def test_check_issue_fields_missing_all(self):
        missing, _ = check_issue_fields("It is broken", "")
        self.assertIn("program", missing)
        self.assertIn("version", missing)
        self.assertIn("command", missing)
        self.assertIn("output", missing)

    def test_check_issue_fields_no_response(self):
        body = (
            "### Program and version\n\n"
            "_No response_\n\n"
            "### Exact command\n\n"
            "_No response_\n\n"
            "### What Ounce-Core did\n\n"
            "_No response_\n"
        )
        missing, _ = check_issue_fields("Bug", body)
        self.assertEqual(set(missing), {"program", "version", "command", "output"})

    def test_detect_problem_report(self):
        report = (
            "Report This Problem\n"
            "Process: ounce-core [12345]\n"
            "Thread 0 Crashed::\n"
            "0 libsystem_kernel.dylib 0x1234 __pthread_kill + 8\n"
        )
        self.assertTrue(detect_problem_report(report))
        self.assertFalse(detect_problem_report("Just a regular question about build."))

    def test_find_possible_duplicates(self):
        open_issues = [
            {"number": 101, "title": "applet software keyboard deadlocks in swkbdShow"},
            {"number": 102, "title": "audio out padding discrepancy in Audren"},
            {"number": 103, "title": "software keyboard swkbdCreate failing"},
        ]
        dups = find_possible_duplicates(
            "Software keyboard hang in swkbdShow",
            "Deadlock occurs when calling swkbdShow",
            open_issues,
            current_number=101,
            limit=3
        )
        # 101 should be excluded as current_number
        dup_numbers = [d["number"] for d in dups]
        self.assertNotIn(101, dup_numbers)
        self.assertIn(103, dup_numbers)

    def test_rule_based_label_suggestions_only_existing(self):
        existing = {"bug", "area: loader", "differential"}
        suggestions = rule_based_label_suggestions(
            "NRO loader crashes on invalid PFS0 magic",
            "When running ounce-core with malformed nro, bug happens",
            existing
        )
        for label in suggestions:
            self.assertIn(label, existing)
        self.assertIn("bug", suggestions)
        self.assertIn("area: loader", suggestions)

    def test_format_triage_comment_no_emoji_and_capped(self):
        comment = format_triage_comment(
            missing_fields=["command", "output"],
            suggested_labels=["bug", "area: loader"],
            duplicates=[{"number": 42, "title": "Related issue"}],
            problem_summary="Segmentation fault occurred during loader parsing.",
            likely_code_area="src/loader/"
        )
        self.assertIn(TRIAGE_MARKER, comment)
        self.assertIn("### Missing reproduction details", comment)
        self.assertIn("- **Command**:", comment)
        self.assertIn("- **Output**:", comment)
        self.assertIn("### Suggested labels", comment)
        self.assertIn("`bug`", comment)
        self.assertIn("`area: loader`", comment)
        self.assertIn("### Potential duplicate open issues", comment)
        self.assertIn("#42: Related issue", comment)
        self.assertIn("### Problem report summary", comment)
        self.assertIn("Likely code area: `src/loader/`", comment)

        # Check no emoji
        for char in comment:
            # Emoji unicode blocks
            code = ord(char)
            self.assertFalse(0x1F300 <= code <= 0x1FAD6 or 0x1F600 <= code <= 0x1F64F)


if __name__ == "__main__":
    unittest.main()
