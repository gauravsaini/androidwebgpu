#!/usr/bin/env python3
"""Check the T499 differential evidence and its default-off interventions."""

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FINDINGS = Path("/Users/Shared/codex-track499-matchedoracle.md")


def require(path: Path, *needles: str) -> None:
    text = path.read_text(errors="replace")
    missing = [needle for needle in needles if needle not in text]
    if missing:
        raise SystemExit(f"{path}: missing evidence {missing!r}")


manifest = json.loads((ROOT / "MANIFEST.json").read_text())
if manifest["rust_binary_rev"] != "bebb568cb2862a49a82f1eb8933959929b2439b4":
    raise SystemExit("manifest Rust revision is not the committed source revision")

report_lines = FINDINGS.read_text().splitlines()
if report_lines.count("CSINV verdict: EMULATOR-BUG") != 1:
    raise SystemExit("findings must contain exactly one required verdict line")

require(
    ROOT / "evidence/t499/qemu-csinv-live-hit2.log",
    "QEMU LIVE HIT 2 BEFORE: pc=0xffffff8008354018 pstate=0xa00001c5",
    "QEMU LIVE HIT 2 AFTER: pc=0xffffff800835401c",
    "x24=0x0000000040000000",
)
require(
    ROOT / "evidence/t499/rust-csinv-live-hit2.log",
    "[break] before pc=0xffffff8008354018 word=0xda818058",
    "pstate=0x60000000",
    "x24=0xffffffffbfffffff",
)
require(
    ROOT / "evidence/t499/qemu-csinv-micro.log",
    "QEMU instruction bytes at 0x40354018: 588081da",
    "x24=0x0000000040000000",
)
require(
    ROOT / "evidence/t499/rust-csinv-micro.log",
    "Rust BEFORE: pc=0x0000000040354018",
    "Rust AFTER: pc=0x000000004035401c",
    "x24=0x0000000040000000",
)

lib = (ROOT / "units/u12-orchestrator/src/lib.rs").read_text()
runner = (ROOT / "units/u12-orchestrator/src/bin/run_kernel.rs").read_text()
if lib.count("if self.guest_quirks_enabled") < 2:
    raise SystemExit("both guest-state interventions must be guarded")
if "let mut guest_quirks_enabled = false;" not in runner:
    raise SystemExit("guest quirks must default to disabled")
require(ROOT / "evidence/t499/rust-csinv-live-hit2.log", "[guest-quirks] enabled=false")

print("G499_3_EVIDENCE")
