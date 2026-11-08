#!/usr/bin/env python3
import json
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track706-clean-gsi-hwc-hotplug")

# Check Flight-01
f01_dir = WORKTREE / "flights/flight-01"
f01_log = f01_dir / "uart.log"
f01_end = f01_dir / "flight-end.json"
f01_gz = f01_dir / "uart.log.gz"
f01_num = f01_dir / "uart.log.numbered.txt"

assert f01_log.exists() and f01_log.stat().st_size > 0, "Flight-01 uart.log missing or empty"
assert f01_end.exists(), "Flight-01 flight-end.json missing"
assert f01_gz.exists() and f01_gz.stat().st_size > 0, "Flight-01 uart.log.gz missing"
assert f01_num.exists() and f01_num.stat().st_size > 0, "Flight-01 uart.log.numbered.txt missing"

f01_data = json.loads(f01_end.read_text())
assert f01_data["qemu_exit_code"] == 0, f"Flight-01 non-zero exit code: {f01_data['qemu_exit_code']}"
assert f01_data["guest_log_max_seconds"] >= 2400.0, f"Flight-01 guest seconds {f01_data['guest_log_max_seconds']} < 2400.0"

f01_text = f01_log.read_text(encoding="latin1", errors="replace")
assert "__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success" in f01_text, "Flight-01 failed to mount /dev/block/vda"

# Check Flight-ab
fab_dir = WORKTREE / "flights/flight-ab"
fab_log = fab_dir / "uart.log"
fab_end = fab_dir / "flight-end.json"
fab_gz = fab_dir / "uart.log.gz"
fab_num = fab_dir / "uart.log.numbered.txt"

assert fab_log.exists() and fab_log.stat().st_size > 0, "Flight-ab uart.log missing or empty"
assert fab_end.exists(), "Flight-ab flight-end.json missing"
assert fab_gz.exists() and fab_gz.stat().st_size > 0, "Flight-ab uart.log.gz missing"
assert fab_num.exists() and fab_num.stat().st_size > 0, "Flight-ab uart.log.numbered.txt missing"

fab_data = json.loads(fab_end.read_text())
assert fab_data["qemu_exit_code"] == 0, f"Flight-ab non-zero exit code: {fab_data['qemu_exit_code']}"

fab_text = fab_log.read_text(encoding="latin1", errors="replace")
assert "partition(s) not found after polling timeout: vda" in fab_text, "Flight-ab did not demonstrate canonical missing vda control failure"
assert "InitFatalReboot" in fab_text, "Flight-ab did not reboot on mount failure"

print(f"FLIGHTS_RECORDED_OK: flight-01={f01_data['guest_log_max_seconds']}s ({f01_data['log_lines']} lines, mount PASS), flight-ab={fab_data['guest_log_max_seconds']}s ({fab_data['log_lines']} lines, canonical control mount FAIL proven)")
