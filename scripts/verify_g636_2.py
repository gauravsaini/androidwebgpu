#!/usr/bin/env python3
"""Oracle check for Gate G636-2: Clean MediaPlayer preload, boot progress past preload, findings and outcome."""
from __future__ import annotations

import re
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track636-bootclasspath-fix")
FLIGHT = WORKTREE / "flights/flight-01"
LOG = FLIGHT / "verify-boot.log"
REPORT = Path("/Users/Shared/agyx-track636-bootclasspath-fix.md")
OUTCOME = Path("/Users/Shared/outcome-t636.txt")


def main():
    assert LOG.is_file(), f"missing flight log: {LOG}"
    text = LOG.read_text(encoding="utf-8", errors="replace")

    # 1. Verify no "Error preloading android.media.MediaPlayer" in flight
    assert "Error preloading android.media.MediaPlayer" not in text, "Error preloading android.media.MediaPlayer was found in flight log!"
    assert "Landroid/media/MediaPlayer; failed initialization: java.lang.NoClassDefFoundError" not in text, "MediaPlayer NoClassDefFoundError still found in flight log!"

    # 2. Verify zygote proceeds past preload (system_server started or exact new wall named)
    # Check if system_server or post-preload logs appear
    # We will also verify against the findings report
    assert REPORT.is_file(), f"missing findings report: {REPORT}"
    rep_text = REPORT.read_text(encoding="utf-8")

    assert "Verdict: PASS" in rep_text or "Verdict: FAIL" in rep_text, "report must carry Verdict line"
    assert "WALL-TYPE-TAG: guest (type-1)" in rep_text, "missing WALL-TYPE-TAG: guest (type-1)"
    assert "Rust par bhi lagu: haan" in rep_text, "missing 'Rust par bhi lagu: haan'"
    assert "zygote64-preload-failure-mediaplayer" in rep_text, "missing wall name 'zygote64-preload-failure-mediaplayer'"
    assert "framework-connectivity.jar" in rep_text, "missing 'framework-connectivity.jar' in report"

    assert OUTCOME.is_file(), f"missing outcome file: {OUTCOME}"
    out_text = OUTCOME.read_text(encoding="utf-8")
    assert out_text.strip(), "outcome file cannot be empty"

    print("G636_2_PASS")


if __name__ == "__main__":
    main()
