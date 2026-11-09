#!/usr/bin/env python3
"""
Verify Track T710 deliverables.
Outputs DELIVERABLES_VERIFIED_OK.
"""
import json
import os
import sys

def verify():
    # 1. findings-t710.md
    findings_path = "findings-t710.md"
    assert os.path.exists(findings_path), f"Missing {findings_path}"
    with open(findings_path, "r", encoding="utf-8") as f:
        content = f.read()
    lines = [l.strip() for l in content.splitlines()]
    assert any(l in ["Verdict: PASS", "Verdict: FAIL"] for l in lines), "Literal Verdict line missing or malformed"
    assert "WALL-TYPE-TAG" in content, "WALL-TYPE-TAG missing"
    assert "Rust par bhi lagu" in content, "Rust par bhi lagu line missing"

    # 2. outcome-t710.txt
    outcome_path = "outcome-t710.txt"
    assert os.path.exists(outcome_path), f"Missing {outcome_path}"
    outcome_data = open(outcome_path, "rb").read()
    assert outcome_data in [b"PASS\n", b"FAIL\n"], f"Invalid outcome file content: {repr(outcome_data)}"

    # 3. DEATH_NOTE-T710.md
    assert os.path.exists("DEATH_NOTE-T710.md"), "Missing DEATH_NOTE-T710.md"

    # 4. progress-t710.txt
    assert os.path.exists("progress-t710.txt"), "Missing progress-t710.txt"

    # 5. TIMELINE.md
    assert os.path.exists("TIMELINE.md"), "Missing TIMELINE.md"
    with open("TIMELINE.md", "r", encoding="utf-8") as f:
        tl = f.read()
    assert "Track T710" in tl or "T710" in tl, "T710 entry missing in TIMELINE.md"

    # 6. heartbeat.json
    assert os.path.exists("heartbeat.json"), "Missing heartbeat.json"
    with open("heartbeat.json", "r") as f:
        hb = json.load(f)
    assert hb.get("status") == "CLOSED", f"heartbeat status not CLOSED: {hb.get('status')}"
    assert hb.get("verdict") in ["PASS", "FAIL"], f"heartbeat verdict missing: {hb.get('verdict')}"

    # 7. Check flight directories
    for flight in ["flight-01", "flight-ab"]:
        for fn in ["uart.log", "uart.log.gz", "uart.log.numbered.txt", "flight-end.json", "heartbeat.json", "preflight.json", "qemu-command.txt"]:
            fp = os.path.join("flights", flight, fn)
            assert os.path.exists(fp), f"Missing flight artifact: {fp}"

    print("DELIVERABLES_VERIFIED_OK")

if __name__ == "__main__":
    verify()
