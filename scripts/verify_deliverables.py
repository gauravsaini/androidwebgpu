#!/usr/bin/env python3
import json
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")

# Check outcome-t705.txt
outcome_file = WORKTREE / "outcome-t705.txt"
assert outcome_file.exists(), "outcome-t705.txt missing"
outcome_bytes = outcome_file.read_bytes()
assert outcome_bytes in [b"PASS\n", b"FAIL\n"], f"outcome-t705.txt content invalid or not 5 bytes: {outcome_bytes!r}"
verdict_str = outcome_bytes.decode("ascii").strip()

# Check findings-t705.md
findings_file = WORKTREE / "findings-t705.md"
assert findings_file.exists(), "findings-t705.md missing"
findings_text = findings_file.read_text(encoding="utf-8")
assert f"Verdict: {verdict_str}" in findings_text, f"Literal 'Verdict: {verdict_str}' line missing in findings-t705.md"
# Must not have markdown bold around verdict line
assert f"**Verdict: {verdict_str}**" not in findings_text, "Verdict line must not have markdown bold"
assert "WALL-TYPE-TAG" in findings_text, "WALL-TYPE-TAG missing in findings"
assert "Rust par bhi lagu:" in findings_text, "'Rust par bhi lagu:' line missing in findings"

# Check DEATH_NOTE-T705.md
death_note = WORKTREE / "DEATH_NOTE-T705.md"
assert death_note.exists(), "DEATH_NOTE-T705.md missing"
assert death_note.stat().st_size > 100, "DEATH_NOTE-T705.md too small"

# Check heartbeat.json
hb_file = WORKTREE / "heartbeat.json"
assert hb_file.exists(), "heartbeat.json missing"
hb = json.loads(hb_file.read_text())
assert hb["status"] == "CLOSED", f"heartbeat status not CLOSED: {hb.get('status')}"
assert hb["verdict"] == verdict_str, f"heartbeat verdict {hb.get('verdict')} != {verdict_str}"

# Check TIMELINE.md
timeline_file = WORKTREE / "TIMELINE.md"
assert timeline_file.exists(), "TIMELINE.md missing"
timeline_text = timeline_file.read_text(encoding="utf-8")
assert "T705" in timeline_text, "T705 entry missing in TIMELINE.md"

print(f"DELIVERABLES_PASS: verdict={verdict_str}")
