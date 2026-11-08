#!/usr/bin/env python3
import json
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")
LOG = WORKTREE / "flights/smoke/uart.log"
END = WORKTREE / "flights/smoke/flight-end.json"

assert LOG.exists(), "Smoke uart.log missing"
assert END.exists(), "Smoke flight-end.json missing"

data = json.loads(END.read_text())
assert data["qemu_exit_code"] == 0, f"QEMU exited with non-zero code {data['qemu_exit_code']}"
assert data["guest_log_max_seconds"] >= 120.0, f"Guest seconds {data['guest_log_max_seconds']} < 120.0"

log_text = LOG.read_text(encoding="latin1", errors="replace")
assert "__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success" in log_text, "vda mount not found in smoke log"
assert "fb0: simplefb registered!" in log_text, "fb0 registration missing"
assert "Freeing unused kernel memory: 960K" in log_text, "Freeing unused kernel memory missing"
assert "init: init first stage started!" in log_text, "init first stage missing"

print(f"SMOKE_MOUNT_VDA_PASS: guest_seconds={data['guest_log_max_seconds']}, lines={data['log_lines']}, sha256={data['log_sha256']}")
