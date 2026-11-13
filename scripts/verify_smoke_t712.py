#!/usr/bin/env python3
"""Verify Step 3a Smoke Flight for Track T712 (120 s guest target)."""
import json
import os
import re
import sys

def verify():
    flight_end_path = "flights/smoke/flight-end.json"
    uart_path = "flights/smoke/uart.log"

    assert os.path.exists(flight_end_path), "flights/smoke/flight-end.json missing"
    assert os.path.exists(uart_path), "flights/smoke/uart.log missing"

    with open(flight_end_path, "r") as f:
        meta = json.load(f)

    assert meta["qemu_exit_code"] == 0, f"QEMU exit code non-zero: {meta['qemu_exit_code']}"
    assert meta["guest_log_max_seconds"] >= 120.0 or "target reached" in meta["stop_reason"].lower(), \
        f"Smoke flight did not reach 120s guest target: {meta}"

    with open(uart_path, "r", encoding="latin1") as f:
        uart = f.read()

    # 1. /dev/block/vda mount
    assert "__mount" in uart and "/system" in uart and "Success" in uart, "Missing /system mount success in UART"
    assert "Freeing unused kernel memory" in uart, "Missing Freeing unused kernel memory in UART"
    assert "init: init first stage started!" in uart, "Missing init first stage in UART"
    assert "simple-framebuffer" in uart or "simplefb" in uart, "Missing simplefb in UART"

    # 2. SurfaceFlinger primeCache skipped
    prime_abort_count = uart.count("output buffer not gpu writeable")
    print(f"output buffer not gpu writeable count: {prime_abort_count}")
    assert prime_abort_count == 0, f"SurfaceFlinger still aborted in primeCache! count={prime_abort_count}"

    # 3. Composer client creation failure: should be 0
    composer_fail_count = uart.count("failed to create composer client")
    print(f"failed to create composer client count: {composer_fail_count}")
    assert composer_fail_count == 0, f"failed to create composer client appeared! count={composer_fail_count}"

    # 4. Dummy sensorservice registered on attempt 0 with 0 avc denials
    assert "dummy_sensorservice" in uart, "dummy_sensorservice marker missing in UART!"
    avc_denial_re = re.compile(r"avc:\s+denied\s+\{[^}]*add[^}]*\}.*name=sensorservice")
    avc_denials = avc_denial_re.findall(uart)
    print(f"sensorservice avc add denial count: {len(avc_denials)}")
    assert len(avc_denials) == 0, f"HARD GATE 2 VIOLATION: {len(avc_denials)} sensorservice avc add denials found!"
    reg_success = "Successfully registered sensorservice with ServiceManager" in uart
    print(f"sensorservice registration success in UART: {reg_success}")
    assert reg_success, "Dummy sensorservice failed to register!"

    # 5. T712 Core Check: hals.conf loading and subHal errors
    hals_conf_errors = [line for line in uart.splitlines() if "Failed to load subHal config file" in line]
    print(f"Failed to load subHal config file count: {len(hals_conf_errors)}")
    for err in hals_conf_errors:
        print(f"  HALS.CONF ERROR: {err}")
    assert len(hals_conf_errors) == 0, f"SensorsMultiHal failed to load hals.conf: {hals_conf_errors}"

    # 6. Watchdog kills check in smoke
    watchdog_kills = len(re.findall(r"WATCHDOG KILLING SYSTEM PROCESS", uart)) + len(re.findall(r"Watchdog:.*Killing", uart))
    print(f"Watchdog killing system process count: {watchdog_kills}")

    # 7. System server PID detection
    system_server_pids = set(re.findall(r"Entered the Android system server!\s+\(pid\s+(\d+)\)", uart))
    print(f"System server launches detected: {len(system_server_pids)} (PIDs: {sorted(system_server_pids)})")

    # 8. Check for ISensors or subHal messages
    multihal_lines = [line for line in uart.splitlines() if "SensorsMultiHal" in line or "android.hardware.sensors" in line]
    print(f"SensorsMultiHal log lines ({len(multihal_lines)}):")
    for l in multihal_lines[:15]:
        print(f"  {l}")

    print("T712_SMOKE_FLIGHT_VERIFIED_OK")
    return {
        "avc_denials": len(avc_denials),
        "reg_success": reg_success,
        "hals_conf_errors": len(hals_conf_errors),
        "watchdog_kills": watchdog_kills,
        "system_server_count": len(system_server_pids),
    }

if __name__ == "__main__":
    verify()
