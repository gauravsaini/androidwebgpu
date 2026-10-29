#!/usr/bin/env python3
"""Detached supervisor for T639 merged lineage flight under QEMU 11.1.2."""

from __future__ import annotations

import datetime as dt
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


ROOT = Path(__file__).resolve().parents[1]
INPUTS = ROOT / "guest-inputs"
GUEST_IMAGE = ROOT / "guest-image"
FLIGHT = ROOT / "flights" / "flight-01"
LOG = FLIGHT / "uart.log"
STDERR = FLIGHT / "qemu-stderr.log"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
KERNEL = INPUTS / "Image"
MERGED_IMAGE = GUEST_IMAGE / "t639-merged-initramfs.cpio.gz"
T636_IMAGE = INPUTS / "t636-bootclasspath-fixed-initramfs.cpio.gz"
T637_IMAGE = INPUTS / "t637-gralloc-mapper-fix-initramfs.cpio.gz"

HEARTBEAT = ROOT / "heartbeat.json"
OUTCOME = Path("/Users/Shared/outcome-t639.txt")
FINDINGS = Path("/Users/Shared/agyx-track639-merge-classpath-mapper.md")

TARGET_HOST_SECONDS = 780
TARGET_GUEST_SECONDS = 660
MIN_FREE_BYTES = 2 * 1024**3
CHUNK = 8 * 1024 * 1024

EXPECTED_KERNEL = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_T636 = "2a279db4a3085df1b523afba28cf6994ab3a51d0921c6879db5a8858217ccec0"
EXPECTED_T637 = "dbb22d63d47ddc05c73b0d91c05be01801f93f87a778cc200600402554933c9c"
EXPECTED_MERGED = "c80fb11251cdce4649636cb312b1b6d447980ac71dcf49fe11c73fce9d2da5c3"
EXPECTED_QEMU = "8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869"

GUEST_TIME_RE = re.compile(rb"^\[\s*([0-9]+\.[0-9]+)\]")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(CHUNK):
            digest.update(block)
    return digest.hexdigest()


def atomic_json(path: Path, value: dict) -> None:
    temp = path.with_suffix(path.suffix + ".partial")
    with open(temp, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2)
        f.write("\n")
    os.replace(temp, path)


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_bytes < MIN_FREE_BYTES:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def update_heartbeat(status: str, step: int, note: str, runner_pid: int | None = None, elapsed: float = 0.0, guest_time: float = 0.0):
    payload = {
        "timestamp": utc_now(),
        "lane": "T639",
        "runner_pid": runner_pid,
        "supervisor_pid": os.getpid(),
        "status": status,
        "current_step": step,
        "host_elapsed_seconds": round(elapsed, 1),
        "guest_time_seconds": round(guest_time, 3),
        "note": note,
    }
    atomic_json(HEARTBEAT, payload)
    if FLIGHT.exists():
        atomic_json(FLIGHT / "heartbeat.json", payload)


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


def write_numbered_log(source: Path, target: Path):
    with source.open("rb") as src, target.open("w", encoding="utf-8", errors="replace") as dst:
        for idx, line in enumerate(src, start=1):
            dst.write(f"{idx:6d}: {line.decode('latin1', errors='replace')}")


def main() -> int:
    FLIGHT.mkdir(parents=True, exist_ok=True)
    update_heartbeat("preflight", 2, "Starting T639 merged runner preflight")

    # 1. Preflight hash verification
    free_gib = check_disk_space()
    print(f"[PREFLIGHT] Disk space available: {free_gib:.2f} GiB")

    hashes = {
        "kernel_Image": sha256(KERNEL),
        "t636_initramfs": sha256(T636_IMAGE),
        "t637_initramfs": sha256(T637_IMAGE),
        "merged_initramfs": sha256(MERGED_IMAGE),
        "qemu_binary": sha256(QEMU),
    }
    assert hashes["kernel_Image"] == EXPECTED_KERNEL, "Kernel hash mismatch"
    assert hashes["t636_initramfs"] == EXPECTED_T636, "T636 hash mismatch"
    assert hashes["t637_initramfs"] == EXPECTED_T637, "T637 hash mismatch"
    assert hashes["merged_initramfs"] == EXPECTED_MERGED, "Merged initramfs hash mismatch"
    assert hashes["qemu_binary"] == EXPECTED_QEMU, "QEMU hash mismatch"
    print("[PREFLIGHT] All image and binary hashes verified OK")

    qemu_version_out = subprocess.run([str(QEMU), "--version"], capture_output=True, text=True, check=True).stdout
    qemu_version = qemu_version_out.splitlines()[0]
    print(f"[PREFLIGHT] QEMU Oracle: {qemu_version}")

    marker = f"androidboot.t639_merged_initramfs_sha256={hashes['merged_initramfs']}"
    cmd = [
        str(QEMU),
        "-machine", "virt,gic-version=2,virtio-mmio-transports=1",
        "-cpu", "cortex-a57",
        "-m", "8192",
        "-smp", "4",
        "-kernel", str(KERNEL),
        "-initrd", str(MERGED_IMAGE),
        "-append",
        "console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk "
        "androidboot.hardware=virt androidboot.selinux=permissive "
        "androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 " + marker,
        "-display", "none",
        "-serial", f"file:{LOG}",
        "-monitor", "none",
        "-no-reboot",
    ]

    preflight_info = {
        "track": "T639",
        "flight": "flight-01",
        "created_utc": utc_now(),
        "hashes": hashes,
        "qemu_version": qemu_version,
        "marker": marker,
        "command": cmd,
    }
    atomic_json(FLIGHT / "preflight.json", preflight_info)
    (FLIGHT / "qemu-command.txt").write_text(" ".join(cmd) + "\n")

    # Remove previous uart.log if any
    if LOG.exists():
        LOG.unlink()
    if STDERR.exists():
        STDERR.unlink()

    # 2. Spawn QEMU
    print(f"[SUPERVISOR] Launching QEMU at {utc_now()}...")
    started = time.monotonic()
    with open(STDERR, "wb") as qemu_stderr:
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=qemu_stderr,
        )
        update_heartbeat("running", 3, f"QEMU running pid={proc.pid}", runner_pid=proc.pid, elapsed=0.0, guest_time=0.0)
        atomic_json(FLIGHT / "flight-start.json", {
            "track": "T639",
            "flight": "flight-01",
            "start_utc": utc_now(),
            "qemu_pid": proc.pid,
            "hashes": hashes,
        })

        stop_reason = "timeout"
        last_hb = 0.0
        seen_mapper_load = False
        seen_zygote_bcp = False

        while True:
            elapsed = time.monotonic() - started
            guest_seconds = guest_log_max_seconds()
            rc = proc.poll()
            if rc is not None:
                stop_reason = f"QEMU process exited early with returncode {rc}"
                break

            # Check log for milestones
            if LOG.exists():
                try:
                    tail = LOG.read_bytes()
                    if not seen_mapper_load and (b"AIMapper_loadIMapper" in tail or b"T637_MAPPER" in tail):
                        seen_mapper_load = True
                        print(f"[{elapsed:.1f}s / g:{guest_seconds:.1f}s] MILESTONE: mapper.default.so loaded by SurfaceFlinger!")
                    if not seen_zygote_bcp and b"T636_ZYGOTE_BOOTCLASSPATH_VERBATIM" in tail:
                        seen_zygote_bcp = True
                        print(f"[{elapsed:.1f}s / g:{guest_seconds:.1f}s] MILESTONE: Zygote BOOTCLASSPATH captured verbatim!")
                except Exception:
                    pass

            # Completion condition: both milestones observed and guest target reached (or 660s guest target)
            if guest_seconds >= TARGET_GUEST_SECONDS:
                stop_reason = f"Target guest time reached ({guest_seconds:.1f}s >= {TARGET_GUEST_SECONDS}s)"
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                break

            if elapsed >= TARGET_HOST_SECONDS:
                stop_reason = f"Target host time reached ({elapsed:.1f}s >= {TARGET_HOST_SECONDS}s)"
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                break

            if elapsed - last_hb >= 15:
                free_disk = check_disk_space()
                update_heartbeat(
                    "running", 3,
                    f"QEMU running for {int(elapsed)}s; guest_time={guest_seconds:.1f}s; mapper={seen_mapper_load}; zygote={seen_zygote_bcp}; free_gib={free_disk:.1f}",
                    runner_pid=proc.pid,
                    elapsed=elapsed,
                    guest_time=guest_seconds
                )
                print(f"[SUPERVISOR] host={elapsed:.0f}s guest={guest_seconds:.1f}s mapper={seen_mapper_load} zygote={seen_zygote_bcp} pid={proc.pid}", flush=True)
                last_hb = elapsed

            time.sleep(3)

    # 3. Post-flight processing
    elapsed = time.monotonic() - started
    guest_seconds = guest_log_max_seconds()
    print(f"[SUPERVISOR] QEMU stopped after host={elapsed:.1f}s, guest={guest_seconds:.1f}s. Stop reason: {stop_reason}")
    update_heartbeat("postflight", 4, "Processing UART logs and producing deliverables", elapsed=elapsed, guest_time=guest_seconds)

    # Create numbered log
    numbered_log = FLIGHT / "uart.log.numbered.txt"
    if LOG.exists():
        write_numbered_log(LOG, numbered_log)
        # Gzip UART log for disk hygiene rule
        log_gz = FLIGHT / "uart.log.gz"
        with LOG.open("rb") as f_in, gzip.open(log_gz, "wb", compresslevel=6) as f_out:
            shutil.copyfileobj(f_in, f_out)
        print(f"[SUPERVISOR] Gzipped UART log to {log_gz} ({log_gz.stat().st_size} bytes)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
