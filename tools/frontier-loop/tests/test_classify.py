import json
import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1]
FIXTURES = Path(__file__).resolve().parent / "fixtures"
sys.path.insert(0, str(SCRIPT_DIR))
import classify  # noqa: E402


class ClassifyTests(unittest.TestCase):
    def test_nonconforming_report_is_a_hard_error(self):
        report = json.loads((FIXTURES / "done.json").read_text())
        del report["binary"]["sha256"]
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as stream:
            json.dump(report, stream)
            stream.flush()
            with self.assertRaisesRegex(ValueError, "schema validation failed"):
                classify.load_and_validate(stream.name)

    def test_schema_contract_works_without_jsonschema_package(self):
        with patch.object(classify, "jsonschema", None):
            self.assertEqual(classify.load_and_validate(FIXTURES / "done.json")["schema"], "frontier-boot-report/1")
            report = json.loads((FIXTURES / "done.json").read_text())
            report["uart"]["init_marker"] = "yes"
            with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as stream:
                json.dump(report, stream)
                stream.flush()
                with self.assertRaisesRegex(ValueError, "schema validation failed"):
                    classify.load_and_validate(stream.name)


def fixture_test(filename, expected):
    def test(self):
        report = classify.load_and_validate(FIXTURES / filename)
        actual = classify.classify_report(report)
        self.assertEqual(
            (actual["category"], actual["agent_kind"], actual["oracle"], actual["confidence"]),
            expected,
        )

    return test


for fixture, expected in {
    "unmapped_mmio.json": ("unmapped_mmio", "device", "qemu_mmio_trace", "high"),
    "unknown_insn.json": ("unknown_insn", "decode", "objdump_capstone", "high"),
    "sysreg.json": ("sysreg", "sysreg", "qemu_sysreg_dump", "high"),
    "fault_divergence.json": ("fault_divergence", "divergence", "qemu_divergence", "medium"),
    "spin.json": ("spin", "spin", "symbolize_poll", "high"),
    "done.json": ("done", "none", "uart_init_marker", "high"),
}.items():
    setattr(ClassifyTests, f"test_fixture_{Path(fixture).stem}", fixture_test(fixture, expected))


if __name__ == "__main__":
    unittest.main()
