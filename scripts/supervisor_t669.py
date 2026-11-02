#!/usr/bin/env python3
"""Supervisor for Track T669 QEMU 11.1.2 guest flight with 1200s guest target."""
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
BASE_IMAGE = WORKTREE / "guest-inputs/t668-base-image.cpio.gz"
IMAGE = WORKTREE / "guest-image/t669-biometrics-fingerprint-aidl-lazy-hal-and-idmap-propertyset-watchdog-60s-initramfs.cpio.gz"

TARGET_HOST_SECONDS = 1500
TARGET_GUEST_SECONDS = 1200
POLL_SECONDS = 5

EXPECTED = {
    "kernel": "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
    "base_image": "7b93fdb23af4edef3300af94fc49743e41256d771b5982db1f70427c45849155",
    "boot_image": "c6c431f49f5e24f11cc1a289f8bfade7cdc75b6adc69ee7b42f3de9f21593548",
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
                try:
                    t = float(match.group(1))
                    if t > last:
                        last = t
                except ValueError:
                    pass
    return last


def update_heartbeat(status: str, elapsed: float, guest_seconds: float, pid: int | None = None) -> None:
    hb = {
        "timestamp_utc": utc_now(),
        "status": status,
        "track": "T669",
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
        "-append", f"console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 brd.rd_nr=1 brd.rd_size=524288 androidboot.t668_base_initramfs_sha256={base_sha} androidboot.t669_fix_initramfs_sha256={boot_sha} androidboot.t669_flight_target=1200",
        "-display", "none",
        "-serial", f"file:{LOG}",
        "-monitor", "none",
        "-no-reboot",
    ]

    (FLIGHT / "qemu-command.txt").write_text(" ".join(cmd) + "\n")

    preflight = {
        "track": "T669",
        "flight": "flight-01",
        "created_utc": utc_now(),
        "qemu_version": qemu_version,
        "command": cmd,
        "inputs": inputs,
    }
    atomic_json(FLIGHT / "preflight.json", preflight)

    # Clean existing flight outputs if running fresh
    for p in [
        LOG,
        STDERR,
        FLIGHT / "flight-end.json",
        FLIGHT / "uart.log.numbered.txt",
        FLIGHT / "uart.log.gz",
    ]:
        if p.exists():
            p.unlink()

    print("[SUPERVISOR] Launching QEMU 11.1.2 guest flight...")
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
                stop_reason = f"Host timeout reached ({elapsed:.1f}s >= {TARGET_HOST_SECONDS}s)"
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                break

            if elapsed - last_hb >= 15.0:
                last_hb = elapsed
                check_disk_space()
                print(
                    f"[SUPERVISOR {utc_now()}] host_elapsed={elapsed:.1f}s guest_max={guest_seconds:.1f}s pid={proc.pid}"
                )
                update_heartbeat("Flight in progress", elapsed, guest_seconds, proc.pid)

            time.sleep(POLL_SECONDS)

    ended_utc = utc_now()
    host_elapsed = time.monotonic() - started
    final_guest = guest_log_max_seconds()
    print(f"[SUPERVISOR] Flight ended at {ended_utc} ({stop_reason})")
    print(f"[SUPERVISOR] Host elapsed: {host_elapsed:.2f}s, Guest log max: {final_guest:.2f}s")

    # Generate compressed log and numbered line log for rapid analysis
    log_bytes = LOG.stat().st_size if LOG.exists() else 0
    log_sha = sha256_file(LOG) if LOG.exists() else ""
    log_lines = 0

    if LOG.exists():
        print(f"[SUPERVISOR] Generating uart.log.numbered.txt and uart.log.gz...")
        numbered_path = FLIGHT / "uart.log.numbered.txt"
        gz_path = FLIGHT / "uart.log.gz"
        with open(LOG, "rb") as fin, open(numbered_path, "w", encoding="utf-8", errors="replace") as fnum, gzip.open(gz_path, "wb") as fgz:
            for idx, raw_line in enumerate(fin, start=1):
                log_lines += 1
                fgz.write(raw_line)
                decoded = raw_line.decode("latin1", errors="replace").rstrip("\r\n")
                fnum.write(f"{idx:6d}: {decoded}\n")

    flight_end = {
        "track": "T669",
        "flight": "flight-01",
        "stop_reason": stop_reason,
        "started_utc": started_utc,
        "ended_utc": ended_utc,
        "host_elapsed_seconds": round(host_elapsed, 2),
        "guest_log_max_seconds": round(final_guest, 3),
        "qemu_exit_code": proc.returncode,
        "log_bytes": log_bytes,
        "log_lines": log_lines,
        "log_sha256": log_sha,
        "inputs": inputs,
    }
    atomic_json(FLIGHT / "flight-end.json", flight_end)
    update_heartbeat(f"Flight complete ({stop_reason})", host_elapsed, final_guest, None)
    print(f"[SUPERVISOR] flight-end.json written. Flight {FLIGHT} complete.")


if __name__ == "__main__":
    main()
