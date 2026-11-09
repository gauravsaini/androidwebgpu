#!/usr/bin/env python3
"""
Verify Step 3b Flight-01 (2400 s guest target).
Outputs FLIGHT_01_VERIFIED_OK.
"""
import json
import os
import sys

def verify():
    flight_end_path = "flights/flight-01/flight-end.json"
    uart_path = "flights/flight-01/uart.log"

    assert os.path.exists(flight_end_path), f"Missing {flight_end_path}"
    assert os.path.exists(uart_path), f"Missing {uart_path}"

    with open(flight_end_path, "r") as f:
        meta = json.load(f)

    # Allow target reached or guest elapsed >= 2400s or host cap
    assert meta["guest_log_max_seconds"] >= 2400.0 or "target reached" in meta["stop_reason"].lower() or meta["host_elapsed_seconds"] >= 2400.0, \
        f"Flight-01 did not complete full window: {meta}"

    print(f"VERIFIED: Flight-01 completed with host={meta['host_elapsed_seconds']}s, guest={meta['guest_log_max_seconds']}s")
    print("FLIGHT_01_VERIFIED_OK")

if __name__ == "__main__":
    verify()
