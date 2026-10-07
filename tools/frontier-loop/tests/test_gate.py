import sys
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPT_DIR))
from gate import evaluate_gate  # noqa: E402


def report(halt_step, git_rev="fix123"):
    return {"binary": {"git_rev": git_rev}, "run": {"halt_step": halt_step}}


class GateTests(unittest.TestCase):
    def test_focused_tests_must_be_green(self):
        result = evaluate_gate(report(10), report(11), "fix123", False, True)
        self.assertEqual(result["status"], "FAIL")
        self.assertIn("focused_tests_failed", result["reason"])
        self.assertTrue(result["checks"]["regression_vector_verified"])
        self.assertTrue(result["checks"]["frontier_advanced"])

    def test_regression_vector_must_fail_before_fix(self):
        result = evaluate_gate(report(10), report(11), "fix123", True, False)
        self.assertEqual(result["status"], "FAIL")
        self.assertIn("regression_vector_missing_or_pre_fix_passed", result["reason"])
        self.assertTrue(result["checks"]["focused_tests_green"])
        self.assertTrue(result["checks"]["frontier_advanced"])

    def test_frontier_must_advance(self):
        result = evaluate_gate(report(11), report(11), "fix123", True, True)
        self.assertEqual(result["status"], "FAIL")
        self.assertIn("frontier_did_not_advance", result["reason"])
        self.assertTrue(result["checks"]["focused_tests_green"])
        self.assertTrue(result["checks"]["regression_vector_verified"])
        self.assertTrue(result["checks"]["binary_matches_fix"])

    def test_frontier_report_must_use_fix_binary(self):
        result = evaluate_gate(report(10), report(11, "oldrev"), "fix123", True, True)
        self.assertEqual(result["status"], "FAIL")
        self.assertIn("new_binary_revision_does_not_match_fix_commit", result["reason"])

    def test_all_three_conditions_pass(self):
        result = evaluate_gate(report(10), report(11), "fix123", True, True)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["reason"], [])


if __name__ == "__main__":
    unittest.main()
