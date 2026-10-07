#!/usr/bin/env python3
"""Validate a frontier boot report and recompute its halt classification."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

try:
    import jsonschema
except ImportError:  # pragma: no cover - exercised only in minimal Python installs
    jsonschema = None


SCRIPT_DIR = Path(__file__).resolve().parent
SCHEMA_PATH = SCRIPT_DIR / "schema.json"

RULES = (
    (r"unmapped_mmio_(?:read|write)", "unmapped_mmio", "device", "qemu_mmio_trace", "high"),
    (r"(?:illegal_insn|decode_trap)", "unknown_insn", "decode", "objdump_capstone", "high"),
    (r"unhandled_sysreg", "sysreg", "sysreg", "qemu_sysreg_dump", "high"),
    (r"(?:data_abort|prefetch_abort)", "fault_divergence", "divergence", "qemu_divergence", "medium"),
    (r"spin_detected", "spin", "spin", "symbolize_poll", "high"),
)


def load_and_validate(report_path: str | Path) -> dict[str, Any]:
    """Load JSON and validate it against the checked-in T70 report schema."""
    try:
        report = json.loads(Path(report_path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read boot report {report_path}: {exc}") from exc

    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    if jsonschema is None:
        validate_contract_without_dependency(report)
    else:
        try:
            jsonschema.Draft202012Validator.check_schema(schema)
            jsonschema.Draft202012Validator(schema).validate(report)
        except jsonschema.ValidationError as exc:
            location = ".".join(str(part) for part in exc.absolute_path) or "<root>"
            raise ValueError(f"boot report schema validation failed at {location}: {exc.message}") from exc
        except jsonschema.SchemaError as exc:
            raise ValueError(f"invalid checked-in boot report schema: {exc.message}") from exc
    return report


def validate_contract_without_dependency(report: Any) -> None:
    """Enforce this schema's report contract when jsonschema is unavailable."""
    def fail(location: str, message: str) -> None:
        raise ValueError(f"boot report schema validation failed at {location}: {message}")

    def object_at(value: Any, location: str, required: tuple[str, ...]) -> dict[str, Any]:
        if not isinstance(value, dict):
            fail(location, "must be an object")
        for key in required:
            if key not in value:
                fail(location, f"missing required property {key!r}")
        return value

    def string_at(value: Any, location: str, nonempty: bool = False) -> None:
        if not isinstance(value, str) or (nonempty and not value):
            fail(location, "must be a nonempty string" if nonempty else "must be a string")

    def hash_at(value: Any, location: str) -> None:
        if not isinstance(value, str) or re.fullmatch(r"[A-Fa-f0-9]{64}", value) is None:
            fail(location, "must be a 64-character SHA-256 hex string")

    def resource_at(value: Any, location: str) -> None:
        if isinstance(value, str):
            hash_at(value, location)
            return
        resource = object_at(value, location, ("sha256",))
        hash_at(resource["sha256"], f"{location}.sha256")

    def int_at(value: Any, location: str, nullable: bool = False) -> None:
        if nullable and value is None:
            return
        if type(value) is not int or value < 0:
            fail(location, "must be a nonnegative integer")

    root = object_at(
        report,
        "<root>",
        ("schema", "binary", "guest", "run", "halt", "uart", "spin", "classification"),
    )
    if root["schema"] != "frontier-boot-report/1":
        fail("schema", "must equal 'frontier-boot-report/1'")

    binary = object_at(root["binary"], "binary", ("git_rev", "sha256", "profile"))
    string_at(binary["git_rev"], "binary.git_rev", nonempty=True)
    hash_at(binary["sha256"], "binary.sha256")
    string_at(binary["profile"], "binary.profile", nonempty=True)

    guest = object_at(root["guest"], "guest", ())
    nested = all(key in guest for key in ("image", "initramfs", "dtb"))
    flat = all(key in guest for key in ("image_sha256", "initramfs_sha256", "dtb_sha256"))
    if not nested and not flat:
        fail("guest", "must contain image/initramfs/dtb artifacts or their *_sha256 fields")
    if nested and not flat:
        for key in ("image", "initramfs", "dtb"):
            resource_at(guest[key], f"guest.{key}")
    if flat:
        for key in ("image_sha256", "initramfs_sha256", "dtb_sha256"):
            hash_at(guest[key], f"guest.{key}")

    run = object_at(root["run"], "run", ("steps", "halt_step", "wall_s", "resumed_from_snapshot"))
    int_at(run["steps"], "run.steps")
    int_at(run["halt_step"], "run.halt_step", nullable=True)
    if isinstance(run["wall_s"], bool) or not isinstance(run["wall_s"], (int, float)) or run["wall_s"] < 0:
        fail("run.wall_s", "must be a nonnegative number")
    if not isinstance(run["resumed_from_snapshot"], (str, bool)) and run["resumed_from_snapshot"] is not None:
        fail("run.resumed_from_snapshot", "must be a string, boolean, or null")

    halt = object_at(root["halt"], "halt", ("pc", "symbol", "insn_hex", "disasm", "reason", "detail"))
    for key in ("symbol", "insn_hex", "disasm"):
        if halt[key] is not None and not isinstance(halt[key], str):
            fail(f"halt.{key}", "must be a string or null")
    if halt["pc"] is not None and (isinstance(halt["pc"], bool) or not isinstance(halt["pc"], (str, int))):
        fail("halt.pc", "must be a string, integer, or null")
    string_at(halt["reason"], "halt.reason")
    if halt["detail"] is not None and not isinstance(halt["detail"], (str, dict, list)):
        fail("halt.detail", "must be a string, object, array, or null")

    uart = object_at(root["uart"], "uart", ("bytes", "init_marker", "tail"))
    int_at(uart["bytes"], "uart.bytes")
    if not isinstance(uart["init_marker"], bool):
        fail("uart.init_marker", "must be a boolean")
    if not isinstance(uart["tail"], (str, list)):
        fail("uart.tail", "must be a string or array")
    if root["spin"] is not None and not isinstance(root["spin"], (dict, bool)):
        fail("spin", "must be an object, boolean, or null")

    classification = object_at(
        root["classification"],
        "classification",
        ("category", "confidence", "oracle", "agent_kind"),
    )
    for key in ("category", "oracle", "agent_kind"):
        string_at(classification[key], f"classification.{key}")
    if classification["confidence"] not in {"high", "medium", "low"}:
        fail("classification.confidence", "must be high, medium, or low")


def classify_report(report: dict[str, Any]) -> dict[str, str]:
    """Return a fresh classification derived from halt reason/detail and UART."""
    halt = report["halt"]
    detail = halt.get("detail")
    text = " ".join(
        (
            str(halt.get("reason", "")),
            detail if isinstance(detail, str) else json.dumps(detail, sort_keys=True),
        )
    ).lower()

    for pattern, category, agent_kind, oracle, confidence in RULES:
        if re.search(pattern, text):
            return {
                "category": category,
                "confidence": confidence,
                "oracle": oracle,
                "agent_kind": agent_kind,
            }

    if "completed" in text and report["uart"]["init_marker"] is True:
        return {
            "category": "done",
            "confidence": "high",
            "oracle": "uart_init_marker",
            "agent_kind": "none",
        }

    return {
        "category": "unknown",
        "confidence": "low",
        "oracle": "none",
        "agent_kind": "human",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("boot_report", help="frontier-boot-report/1 JSON file")
    args = parser.parse_args(argv)
    try:
        report = load_and_validate(args.boot_report)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    print(json.dumps(classify_report(report), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
