#!/usr/bin/env python3
"""Detached supervisor for T636 QEMU 11.1.2 guest fix flight."""
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

WORKTREE = Path("/Users/Shared/wt-track636-bootclasspath-fix")
FLIGHT = WORKTREE / "flights/flight-01"
LOG = FLIGHT / "verify-boot.log"
STDERR = FLIGHT / "qemu-stderr.log"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
KERNEL = WORKTREE / "guest-inputs/Image"
BASE_IMAGE = WORKTREE / "guest-inputs/t628-iallocator-service-initramfs.cpio.gz"
IMAGE = WORKTREE / "guest-image/t636-bootclasspath-fixed-initramfs.cpio.gz"

TARGET_HOST_SECONDS = 680
TARGET_GUEST_SECONDS = 660
POLL_SECONDS = 5

EXPECTED = {
    "kernel": "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
    "qemu": "8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869",
    "base_image": "6b0b0d715b92e97b9a997c60f7c5e06cff81d7178af92d1ed28fc55a95414e17",
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
        "track": "T636",
        "status": status,
        "updated_utc": utc_now(),
        "flight": "flight-01",
        "host_elapsed_seconds": round(elapsed, 1),
        "guest_log_max_seconds": round(guest_seconds, 3),
        "target_host_seconds": TARGET_HOST_SECONDS,
        "target_guest_seconds": TARGET_GUEST_SECONDS,
        "supervisor_pid": os.getpid(),
        "qemu_pid": pid,
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

    cmd = [
        str(QEMU),
        "-machine", "virt,gic-version=2,virtio-mmio-transports=1",
        "-cpu", "cortex-a57",
        "-m", "8192",
        "-smp", "4",
        "-kernel", str(KERNEL),
        "-initrd", str(IMAGE),
        "-append", "console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0",
        "-display", "none",
        "-serial", f"file:{LOG}",
        "-monitor", "none",
        "-no-reboot",
    ]

    preflight = {
        "track": "T636",
        "flight": "flight-01",
        "created_utc": utc_now(),
        "worktree": str(WORKTREE),
        "files": inputs,
        "qemu_version": qemu_version,
        "target_host_seconds": TARGET_HOST_SECONDS,
        "target_guest_seconds": TARGET_GUEST_SECONDS,
        "initramfs_decision": "single fix initramfs overlay (t636-bootclasspath-fixed-initramfs.cpio.gz) on pinned T624-fixed lineage base",
        "command": cmd,
    }
    atomic_json(FLIGHT / "preflight.json", preflight)
    (FLIGHT / "preflight.sha256").write_text(
        "\n".join(f"{item['sha256']}  {item['path']}" for item in inputs.values()) + "\n",
        encoding="utf-8",
    )

    if LOG.exists() or STDERR.exists():
        raise FileExistsError(f"Refusing to overwrite existing flight-01 logs in {FLIGHT}")

    started_utc = utc_now()
    started = time.monotonic()
    print(f"[SUPERVISOR] Launching QEMU at {started_utc}...")
    with open(STDERR, "wb") as qemu_stderr:
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=qemu_stderr,
            start_new_session=False,
        )
        atomic_json(FLIGHT / "flight-start.json", {
            "track": "T636",
            "flight": "flight-01",
            "start_utc": started_utc,
            "supervisor_pid": os.getpid(),
            "qemu_pid": proc.pid,
            "target_host_seconds": TARGET_HOST_SECONDS,
            "target_guest_seconds": TARGET_GUEST_SECONDS,
            "boot_image_path": str(IMAGE),
            "boot_image_sha256": inputs["boot_image"]["sha256"],
            "kernel_sha256": inputs["kernel"]["sha256"],
            "qemu_sha256": inputs["qemu"]["sha256"],
        })
        update_heartbeat("QEMU guest flight running", 0.0, 0.0, proc.pid)
        last_hb = 0.0
        stop_reason = "QEMU exited early"
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
            if elapsed - last_hb >= 15:
                update_heartbeat("QEMU guest flight running", elapsed, guest_seconds, proc.pid)
                print(
                    f"[SUPERVISOR] flight-01 host={elapsed:.0f}s guest-log={guest_seconds:.1f}s pid={proc.pid}",
                    flush=True,
                )
                last_hb = elapsed
            time.sleep(POLL_SECONDS)

    elapsed = time.monotonic() - started
    guest_seconds = guest_log_max_seconds()
    log_bytes = LOG.stat().st_size if LOG.exists() else 0
    log_sha = sha256_file(LOG) if LOG.exists() else None
    stderr_sha = sha256_file(STDERR)
    decoded = LOG.read_text(encoding="utf-8", errors="replace") if LOG.exists() else ""

    # Gzip UART log for disk hygiene rule
    log_gz = LOG.with_suffix(".log.gz")
    if LOG.exists():
        with open(LOG, "rb") as f_in, gzip.open(log_gz, "wb", compresslevel=6) as f_out:
            shutil.copyfileobj(f_in, f_out)
        print(f"[SUPERVISOR] Gzipped UART log to {log_gz} ({log_gz.stat().st_size} bytes)")

    # Analyze guest findings
    zygote_processes = re.findall(r"T636_ZYGOTE_PROCESS_FOUND pid=(\d+) comm=(\S+) cmd=(.*)", decoded)
    zygote_bcp_lines = re.findall(r"T636_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=(\d+) (BOOTCLASSPATH=.*)", decoded)
    zygote_bcp_items = re.findall(r"T636_ZYGOTE_BCP_ITEM pid=(\d+) item=(.*)", decoded)

    media_failure_lines = [
        {"line": i, "text": line}
        for i, line in enumerate(decoded.splitlines(), 1)
        if "MediaPlayer" in line and (
            "NoClassDefFoundError" in line
            or "failed initialization" in line
            or "Error preloading" in line
            or "ClassNotFoundException" in line
        )
    ]

    art_evidence_lines = [
        {"line": i, "text": line}
        for i, line in enumerate(decoded.splitlines(), 1)
        if any(tag in line for tag in [
            "art :", "art:", "dex2oat", "Zygote  :", "zygote64:", "T636_BOOTCLASSPATH", "T636_SOURCE_BCP", "T636_UPDATED_BCP"
        ])
    ]

    system_server_lines = [
        {"line": i, "text": line}
        for i, line in enumerate(decoded.splitlines(), 1)
        if any(tag in line for tag in [
            "system_server", "T636_SYSTEM_SERVER", "Entered the Android system server!"
        ])
    ]

    ended_utc = utc_now()
    flight_end = {
        "track": "T636",
        "flight": "flight-01",
        "started_utc": started_utc,
        "ended_utc": ended_utc,
        "host_elapsed_seconds": round(elapsed, 2),
        "guest_log_max_seconds": round(guest_seconds, 3),
        "stop_reason": stop_reason,
        "log": {
            "path": str(LOG),
            "bytes": log_bytes,
            "sha256": log_sha,
            "gz_path": str(log_gz),
            "gz_bytes": log_gz.stat().st_size if log_gz.exists() else 0,
        },
        "stderr": {
            "path": str(STDERR),
            "bytes": STDERR.stat().st_size,
            "sha256": stderr_sha,
        },
        "findings": {
            "zygote_processes_detected": len(zygote_processes),
            "zygote_bcp_verbatim_count": len(zygote_bcp_lines),
            "zygote_bcp_items_count": len(zygote_bcp_items),
            "mediaplayer_failure_count": len(media_failure_lines),
            "system_server_lines_count": len(system_server_lines),
            "sample_zygote_bcp": zygote_bcp_lines[0] if zygote_bcp_lines else None,
            "sample_media_failures": media_failure_lines[:5],
        },
    }
    atomic_json(FLIGHT / "flight-end.json", flight_end)
    update_heartbeat(f"COMPLETED ({stop_reason})", elapsed, guest_seconds)
    print(f"[SUPERVISOR] Flight completed: {stop_reason} in {elapsed:.1f}s (guest={guest_seconds:.1f}s)")


if __name__ == "__main__":
    main()
