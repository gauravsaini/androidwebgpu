#!/usr/bin/env python3
import json
import os
import sys

def verify():
    # 1. Verify Flight-01
    f01_end = "flights/flight-01/flight-end.json"
    f01_uart = "flights/flight-01/uart.log"
    assert os.path.exists(f01_end), f"Missing {f01_end}"
    assert os.path.exists(f01_uart), f"Missing {f01_uart}"

    with open(f01_end, "r") as f:
        meta01 = json.load(f)

    assert meta01["guest_log_max_seconds"] >= 2400.0 or "target reached" in meta01["stop_reason"].lower(), \
        f"Flight-01 did not reach target: {meta01}"

    # 2. Verify Flight-ab (canonical control)
    fab_end = "flights/flight-ab/flight-end.json"
    fab_uart = "flights/flight-ab/uart.log"
    assert os.path.exists(fab_end), f"Missing {fab_end}"
    assert os.path.exists(fab_uart), f"Missing {fab_uart}"

    with open(fab_end, "r") as f:
        meta_ab = json.load(f)

    print("FLIGHTS_COMPLETED_OK")

if __name__ == "__main__":
    verify()
