#!/usr/bin/env python3
"""Verify Step 3a Smoke Flight (120 s guest target)."""
import json
import os
import sys

def verify():
    flight_end_path = "flights/smoke/flight-end.json"
    uart_path = "flights/smoke/uart.log"

    assert os.path.exists(flight_end_path), "flights/smoke/flight-end.json missing"
    assert os.path.exists(uart_path), "flights/smoke/uart.log missing"

    with open(flight_end_path, "r") as f:
        meta = json.load(f)

    assert meta["guest_log_max_seconds"] >= 120.0 or "target reached" in meta["stop_reason"].lower(), \
        f"Smoke flight did not reach 120s guest target: {meta}"

    with open(uart_path, "r", encoding="latin1") as f:
        uart = f.read()

    # Check /dev/block/vda mount
    assert "__mount" in uart and "/system" in uart and "Success" in uart, "Missing /system mount success in UART"
    assert "Freeing unused kernel memory" in uart, "Missing Freeing unused kernel memory in UART"
    assert "init: init first stage started!" in uart, "Missing init first stage in UART"
    assert "simple-framebuffer" in uart or "simplefb" in uart, "Missing simplefb in UART"

    # Check primeCache skipped:
    prime_abort_count = uart.count("output buffer not gpu writeable")
    print(f"output buffer not gpu writeable count: {prime_abort_count}")
    assert prime_abort_count == 0, f"SurfaceFlinger still aborted in primeCache! count={prime_abort_count}"

    # Check Composer client creation failure: should be 0
    composer_fail_count = uart.count("failed to create composer client")
    print(f"failed to create composer client count: {composer_fail_count}")
    assert composer_fail_count == 0, f"failed to create composer client appeared! count={composer_fail_count}"

    # Check dummy_sensorservice / streamer active
    assert "dummy_sensorservice" in uart, "dummy_sensorservice marker missing in UART!"

    print("SMOKE_FLIGHT_VERIFIED_OK")

if __name__ == "__main__":
    verify()
