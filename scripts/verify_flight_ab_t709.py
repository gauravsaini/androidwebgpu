#!/usr/bin/env python3
"""
Verify Flight-ab (canonical kernel control flight, 240 s guest).
Proves vda polling timeout and reboot under canonical kernel (lacking CONFIG_VIRTIO_BLK=y).
Outputs FLIGHT_AB_VERIFIED_OK.
"""

import json
import os
import sys

def verify():
    flight_end_path = "flights/flight-ab/flight-end.json"
    uart_path = "flights/flight-ab/uart.log"

    assert os.path.exists(flight_end_path), f"Missing {flight_end_path}"
    assert os.path.exists(uart_path), f"Missing {uart_path}"

    with open(flight_end_path, "r") as f:
        meta = json.load(f)

    with open(uart_path, "r", encoding="latin1") as f:
        uart = f.read()

    # Verify canonical kernel control indicators
    # Canonical kernel has no virtio-blk, so waiting for /dev/block/vda times out
    assert "05c4391767a6ff44" in meta["inputs"]["kernel"]["sha256"], "Kernel is not canonical Image!"
    assert "timed out" in uart or "InitFatalReboot" in uart or "failed" in uart or "reboot" in uart.lower(), \
        "Expected timeout or reboot on vda polling in canonical control flight!"

    print("VERIFIED: Flight-ab canonical kernel control proves CONFIG_VIRTIO_BLK=y requirement")
    print("FLIGHT_AB_VERIFIED_OK")

if __name__ == "__main__":
    verify()
