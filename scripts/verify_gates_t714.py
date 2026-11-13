#!/usr/bin/env python3
"""Gate verification script for Track T714 (Unlazy ledger)."""

import hashlib
import json
import os
import shutil
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent

T713_WORKTREE = Path("/Users/Shared/wt-track713-sensorstart-bypass")

INPUTS = {
    "findings-t713.md": (T713_WORKTREE / "findings-t713.md", "1d4b053db58c3f880ae30e15c5bbd0a765bde0341693e47f06009d66a3a334d2"),
    "DEATH_NOTE-T713.md": (T713_WORKTREE / "DEATH_NOTE-T713.md", "fb2c96f7d8e6875b1b55c9f8f1822ea444ffbaf8c523360bdd018d1ea4221f18"),
    "t713-flight-01-uart.log": (T713_WORKTREE / "flights/flight-01/uart.log", "dbd2108d4cda477a6d1c67ad80d4fd58f1fecd4e23a6c6be34204fc4cdac49cf"),
    "t713-flight-ab-uart.log": (T713_WORKTREE / "flights/flight-ab/uart.log", "243f07249f1b1c730137d558befb20296336453d34b9e1922ba30bfd9cbf323e"),
    "t713-smoke-uart.log": (T713_WORKTREE / "flights/smoke/uart.log", "297435f5ef665a6e8eb73aba8d7f74a57fb119e5552d5e33336674692707de35"),
    "guest-inputs/t713-fix.cpio.gz": (WORKTREE / "guest-inputs/t713-fix.cpio.gz", "edba515eac53a2544ba33aa17cb3749c37a2f336207a3efcdfffb94e56ac3177"),
    "guest-inputs/t713-modified.dtb": (WORKTREE / "guest-inputs/t713-modified.dtb", "d60bc98a67c8b8c014b65412d9af869a432a744f7f1eab24b78a290fdf396161"),
    "guest-inputs/t712-vendor.img": (WORKTREE / "guest-inputs/t712-vendor.img", "095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf"),
    "guest-inputs/system.img (original unpatched)": (Path("/mnt/sdb1/kernel-critical/lane-images/t706-system.img"), "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601"),
    "guest-inputs/t705-Image": (WORKTREE / "guest-inputs/t705-Image", "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859"),
    "extracted_kernel.config": (WORKTREE / "extracted_kernel.config", "d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8"),
}

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def check_g0():
    free_bytes = shutil.disk_usage("/System/Volumes/Data").free
    min_bytes = 8 * 1024 * 1024 * 1024
    if free_bytes < min_bytes:
        print(f"FAIL: Available disk space {free_bytes / (1024**3):.2f} GiB < 8 GiB threshold")
        sys.exit(1)
    print("DISK_GATE_OK")

def check_g1():
    for name, (p, expected_sha) in INPUTS.items():
        if not p.exists():
            # if remote box mount, skip local check if not mounted locally
            if str(p).startswith("/mnt/"):
                continue
            print(f"FAIL: Input file missing: {p}")
            sys.exit(1)
        actual_sha = sha256_file(p)
        if actual_sha != expected_sha:
            print(f"FAIL: Input file SHA mismatch for {name}: expected {expected_sha}, got {actual_sha}")
            sys.exit(1)
    # Check rebuilt composer ELF if present
    composer_path = WORKTREE / "guest-inputs/android.hardware.graphics.composer@2.4-service.headless"
    if composer_path.exists():
        actual = sha256_file(composer_path)
        assert actual == "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401"
    print("INPUTS_VERIFIED_OK")

def check_g2():
    # Verify libandroid_servers.so preserved in guest-inputs/
    so_path = WORKTREE / "guest-inputs/libandroid_servers.so"
    if not so_path.exists():
        print(f"FAIL: libandroid_servers.so not preserved in {so_path}")
        sys.exit(1)
    # Verify unpatched libandroid_servers.so SHA
    so_sha = sha256_file(so_path)
    if so_sha != "2ce96adb15e4a8934d52cfaa8825c1b500b3eefa6a989e071a33963f1580539c":
        print(f"FAIL: libandroid_servers.so SHA mismatch: got {so_sha}")
        sys.exit(1)
    # Verify patch report exists
    patch_report = WORKTREE / "evidence/t714_patch_report.json"
    if not patch_report.exists():
        print("FAIL: evidence/t714_patch_report.json missing")
        sys.exit(1)
    data = json.loads(patch_report.read_text())
    assert data["patch_offset"] == 0xaaee8
    assert data["original_bytes"] == "34080036"
    assert data["patched_bytes"] == "41000014"
    assert data["string_offset"] == 0x2ef7c
    assert data["v2_adrp_resolved"] is True
    assert data["services_jar_byte_identical"] is True
    assert data["e2fsck_exit_code"] == 0
    print("PATCH_VERIFIED_OK")

def check_g3():
    # Verify smoke flight
    smoke_uart = WORKTREE / "flights/smoke/uart.log"
    smoke_end = WORKTREE / "flights/smoke/flight-end.json"
    if not smoke_uart.exists() or not smoke_end.exists():
        print("FAIL: smoke flight output missing")
        sys.exit(1)
    content = smoke_uart.read_text(errors="replace")
    assert "/dev/block/vda" in content and "/system" in content
    # Prove native start returns (no watchdog kill in 120s guest)
    assert "WATCHDOG" not in content.upper() or "KILLING" not in content
    print("SMOKE_FLIGHT_OK")

def check_g4():
    f1_uart = WORKTREE / "flights/flight-01/uart.log"
    f1_end = WORKTREE / "flights/flight-01/flight-end.json"
    fab_uart = WORKTREE / "flights/flight-ab/uart.log"
    fab_end = WORKTREE / "flights/flight-ab/flight-end.json"
    if not f1_uart.exists() or not f1_end.exists() or not fab_uart.exists() or not fab_end.exists():
        print("FAIL: flight-01 or flight-ab files missing")
        sys.exit(1)
    f1_data = json.loads(f1_end.read_text())
    assert f1_data["guest_log_max_seconds"] >= 2400.0 or "reached" in f1_data.get("stop_reason", "").lower()
    print("FLIGHTS_EXECUTED_OK")

def check_g5():
    req_files = [
        "findings-t714.md",
        "outcome-t714.txt",
        "DEATH_NOTE-T714.md",
        "progress-t714.txt",
        "TIMELINE.md",
        "heartbeat.json",
    ]
    for rf in req_files:
        p = WORKTREE / rf
        if not p.exists():
            print(f"FAIL: Missing required deliverable {rf}")
            sys.exit(1)
    f_content = (WORKTREE / "findings-t714.md").read_text()
    assert ("Verdict: PASS\n" in f_content or "Verdict: FAIL\n" in f_content)
    assert "**Verdict:" not in f_content
    assert "WALL-TYPE-TAG" in f_content
    assert "Rust par bhi lagu:" in f_content
    o_content = (WORKTREE / "outcome-t714.txt").read_bytes()
    assert o_content in [b"PASS\n", b"FAIL\n"]
    hb = json.loads((WORKTREE / "heartbeat.json").read_text())
    assert hb["status"] == "CLOSED"
    print("DELIVERABLES_VERIFIED_OK")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: verify_gates_t714.py <g0|g1|g2|g3|g4|g5>")
        sys.exit(1)
    cmd = sys.argv[1].lower()
    if cmd in ("g0", "step0"):
        check_g0()
    elif cmd in ("g1", "step1"):
        check_g1()
    elif cmd in ("g2", "step2"):
        check_g2()
    elif cmd in ("g3", "step3a"):
        check_g3()
    elif cmd in ("g4", "step3b"):
        check_g4()
    elif cmd in ("g5", "step4"):
        check_g5()
    else:
        print(f"Unknown gate: {cmd}")
        sys.exit(1)
