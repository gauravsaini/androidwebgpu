#!/usr/bin/env python3
import json
import os
import sys

def verify():
    # 1. findings-t708.md
    findings_path = "findings-t708.md"
    assert os.path.exists(findings_path), f"Missing {findings_path}"
    with open(findings_path, "r", encoding="utf-8") as f:
        content = f.read()
    lines = [l.strip() for l in content.splitlines()]
    assert any(l in ["Verdict: PASS", "Verdict: FAIL"] for l in lines), "Literal Verdict line missing or malformed"
    assert "WALL-TYPE-TAG" in content, "WALL-TYPE-TAG missing"
    assert "Rust par bhi lagu" in content, "Rust par bhi lagu line missing"

    # 2. outcome-t708.txt
    outcome_path = "outcome-t708.txt"
    assert os.path.exists(outcome_path), f"Missing {outcome_path}"
    outcome_data = open(outcome_path, "rb").read()
    assert outcome_data in [b"PASS\n", b"FAIL\n"], f"Invalid outcome file content: {repr(outcome_data)}"

    # 3. DEATH_NOTE-T708.md
    assert os.path.exists("DEATH_NOTE-T708.md"), "Missing DEATH_NOTE-T708.md"

    # 4. progress-t708.txt
    assert os.path.exists("progress-t708.txt"), "Missing progress-t708.txt"

    # 5. TIMELINE.md
    assert os.path.exists("TIMELINE.md"), "Missing TIMELINE.md"
    with open("TIMELINE.md", "r", encoding="utf-8") as f:
        tl = f.read()
    assert "Track T708" in tl or "T708" in tl, "T708 entry missing in TIMELINE.md"

    # 6. heartbeat.json
    assert os.path.exists("heartbeat.json"), "Missing heartbeat.json"
    with open("heartbeat.json", "r") as f:
        hb = json.load(f)
    assert hb.get("status") == "CLOSED", f"heartbeat status not CLOSED: {hb.get('status')}"
    assert hb.get("verdict") in ["PASS", "FAIL"], f"heartbeat verdict missing: {hb.get('verdict')}"

    print("DELIVERABLES_VERIFIED_OK")

if __name__ == "__main__":
    verify()
