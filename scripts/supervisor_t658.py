#!/usr/bin/env python3
"""Supervisor for Track T658 QEMU 11.1.2 guest flight with 1200s guest target."""
from __future__ import annotations

import datetime
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
FLIGHT = WORKTREE / "flights/flight-01"
LOG = FLIGHT / "uart.log"
STDERR = FLIGHT / "qemu-stderr.log"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
KERNEL = WORKTREE / "guest-inputs/Image"
BASE_IMAGE = WORKTREE / "guest-inputs/t657-base-image.cpio.gz"
IMAGE = WORKTREE / "guest-image/t658-system-server-startotherservices-connectivityservice-noresourcepackage-initramfs.cpio.gz"

TARGET_HOST_SECONDS = 1500
TARGET_GUEST_SECONDS = 1200
POLL_SECONDS = 5

EXPECTED = {
    "kernel": "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
    "base_image": "e19481445ae86bb334d63502c42070d7f0e8637398458aeb19925852db4e4a70",
    "boot_image": "220fcef8fb0536d48f818a04ff0ccecadabaaf8e03a93cf1d0554dfd36e2223a",
}

GUEST_TIME_RE = re.compile(rb"^\[\s*([0-9]+\.[0-9]+)\]")


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def atomic_json(path: Path, value: dict) -> None:
    path = Path(path)
    temp = path.with_suffix(path.suffix + ".partial")
    with open(temp, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2)
        f.write("\n")
    os.replace(temp, path)


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def verify_inputs() -> dict:
    paths = {
        "kernel": KERNEL,
        "qemu": QEMU,
        "base_image": BASE_IMAGE,
        "boot_image": IMAGE,
    }
    records = {}
    for name, path in paths.items():
        if not path.is_file():
            raise FileNotFoundError(f"Required input missing: {path}")
        digest = sha256_file(path)
        if name in EXPECTED:
            expected = EXPECTED[name]
            if digest != expected:
                raise ValueError(f"Input SHA mismatch for {name}: {digest} != {expected}")
        records[name] = {"path": str(path), "sha256": digest, "bytes": path.stat().st_size}
        print(f"[PREFLIGHT] {name}: {digest} ({path.stat().st_size} bytes) OK")
    return records


def guest_log_max_seconds() -> float:
    if not LOG.exists():
        return 0.0
    last = 0.0
    with open(LOG, "rb") as f:
        for line in f:
            match = GUEST_TIME_RE.match(line)
            if match:
                last = max(last, float(match.group(1)))
    return last


def update_heartbeat(status: str, elapsed: float, guest_seconds: float, pid: int | None = None) -> None:
    hb = {
        "track": "T658",
        "timestamp": utc_now(),
        "status": status,
        "flight": "flight-01",
        "current_step": 4,
        "host_elapsed_seconds": round(elapsed, 1),
        "guest_log_max_seconds": round(guest_seconds, 3),
        "target_host_seconds": TARGET_HOST_SECONDS,
        "target_guest_seconds": TARGET_GUEST_SECONDS,
        "supervisor_pid": os.getpid(),
        "qemu_pid": pid,
        "note": f"Flight in progress: {status} host={elapsed:.0f}s guest={guest_seconds:.1f}s",
    }
    atomic_json(WORKTREE / "heartbeat.json", hb)
    if FLIGHT.exists():
        atomic_json(FLIGHT / "heartbeat.json", hb)


def main():
    free_gib = check_disk_space()
    print(f"[SUPERVISOR] Disk check passed: {free_gib:.2f} GiB available on /System/Volumes/Data.")

    FLIGHT.mkdir(parents=True, exist_ok=True)
    inputs = verify_inputs()

    qemu_version = subprocess.run(
        [str(QEMU), "--version"], capture_output=True, text=True, check=True
    ).stdout.splitlines()[0]

    boot_sha = inputs["boot_image"]["sha256"]
    base_sha = inputs["base_image"]["sha256"]
    cmd = [
        str(QEMU),
        "-machine", "virt,gic-version=2,virtio-mmio-transports=1",
        "-cpu", "cortex-a57",
        "-m", "8192",
        "-smp", "4",
        "-kernel", str(KERNEL),
        "-initrd", str(IMAGE),
        "-append", f"console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 androidboot.t657_base_initramfs_sha256={base_sha} androidboot.t658_fix_initramfs_sha256={boot_sha} androidboot.t658_flight_target=1200",
        "-display", "none",
        "-serial", f"file:{LOG}",
        "-monitor", "none",
        "-no-reboot",
    ]

    (FLIGHT / "qemu-command.txt").write_text(" ".join(cmd) + "\n")

    preflight = {
        "track": "T658",
        "flight": "flight-01",
        "created_utc": utc_now(),
        "qemu_version": qemu_version,
        "command": cmd,
        "inputs": inputs,
    }
    atomic_json(FLIGHT / "preflight.json", preflight)

    # Truncate / clean log files before launch
    if LOG.exists():
        LOG.unlink()
    if STDERR.exists():
        STDERR.unlink()

    print(f"[SUPERVISOR] Launching QEMU 11.1.2 guest flight...")
    started_utc = utc_now()
    started = time.monotonic()
    last_hb = -100.0

    with open(STDERR, "wb") as stderr_f:
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=stderr_f)
        update_heartbeat("QEMU guest launched", 0.0, 0.0, proc.pid)
        print(f"[SUPERVISOR] QEMU pid={proc.pid} started at {started_utc}")

        stop_reason = "unknown"
        while True:
            elapsed = time.monotonic() - started
            guest_seconds = guest_log_max_seconds()
            rc = proc.poll()
            if rc is not None:
                stop_reason = f"QEMU process exited with returncode {rc}"
                break
            if guest_seconds >= TARGET_GUEST_SECONDS:
                stop_reason = f"Guest target reached ({guest_seconds:.1f}s >= {TARGET_GUEST_SECONDS}s)"
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                break
            if elapsed >= TARGET_HOST_SECONDS:
                stop_reason = f"Host target reached ({elapsed:.1f}s >= {TARGET_HOST_SECONDS}s)"
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                break

            # Heartbeat every 10 seconds
            if elapsed - last_hb >= 10.0:
                last_hb = elapsed
                free_gib = check_disk_space()
                update_heartbeat("RUNNING", elapsed, guest_seconds, proc.pid)
                print(
                    f"[{utc_now()}] host={elapsed:6.1f}s guest={guest_seconds:6.1f}s "
                    f"pid={proc.pid} disk_free={free_gib:.2f}GiB"
                )

            time.sleep(POLL_SECONDS)

    ended_utc = utc_now()
    final_guest_seconds = guest_log_max_seconds()
    total_elapsed = time.monotonic() - started
    print(f"[SUPERVISOR] Flight ended at {ended_utc} after host={total_elapsed:.1f}s, guest={final_guest_seconds:.1f}s: {stop_reason}")

    postflight = {
        "track": "T658",
        "flight": "flight-01",
        "started_utc": started_utc,
        "ended_utc": ended_utc,
        "host_elapsed_seconds": round(total_elapsed, 2),
        "guest_log_max_seconds": round(final_guest_seconds, 3),
        "stop_reason": stop_reason,
        "qemu_returncode": proc.returncode,
        "uart_log_bytes": LOG.stat().st_size if LOG.exists() else 0,
        "stderr_log_bytes": STDERR.stat().st_size if STDERR.exists() else 0,
    }
    atomic_json(FLIGHT / "flight-end.json", postflight)
    update_heartbeat(f"COMPLETED ({stop_reason})", total_elapsed, final_guest_seconds, None)
    print(f"[SUPERVISOR] Recorded postflight results to {FLIGHT / 'flight-end.json'}")


if __name__ == "__main__":
    main()
