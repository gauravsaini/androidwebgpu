#!/usr/bin/env python3
"""T699 flight supervisor for Route B Image and A/B control (Mac QEMU 11.1.2).

Adapted from scripts/supervisor_t697.py:
- Machine shape: virt,gic-version=2,virtio-mmio-transports=1, -smp 4
- Initramfs relocated: loader at INITRD_ADDR 0x110000000
- Kernel: guest-inputs/routeB-Image (SHA 3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3)
- Drive id: t699system (virtio-blk-device)
- Append tags: androidboot.t699_kernel_sha256 and androidboot.t699_fix_initramfs_sha256 updated
- Host cap: 1200 s (configurable via T699_HOST_CAP)
- Guest target: 1200 s (configurable via T699_GUEST_TARGET)
- GDB port: unique per flight (configurable via T699_GDB_PORT, default 1242)
- Fully detached: setsid + nohup, stdin /dev/null
- Track/flight constants renamed T697 -> T699
"""
from __future__ import annotations

import datetime
import gzip
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
TRACK = "T699"

KERNEL = Path(os.environ.get("T699_KERNEL", str(WORKTREE / "guest-inputs/routeB-Image")))
KERNEL_SHA = os.environ.get(
    "T699_KERNEL_SHA",
    "3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3",
)
FLIGHT_NAME = os.environ.get("T699_FLIGHT", "flight-01")
FLIGHT = WORKTREE / "flights" / FLIGHT_NAME
LOG = Path(os.environ.get("T699_LOG", str(FLIGHT / "uart.log")))
STDERR = FLIGHT / "qemu-stderr.log"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
DTB = Path(os.environ.get("T699_DTB", str(WORKTREE / "guest-inputs/modified.dtb")))
BOOT_IMAGE = Path(os.environ.get("T699_BOOT_IMAGE", str(WORKTREE / "guest-image/t699-systemserver-fb0-fix.cpio.gz")))
SYSTEM_IMG = WORKTREE / "guest-inputs/system.img"

TARGET_HOST_SECONDS = int(os.environ.get("T699_HOST_CAP", "1200"))
TARGET_GUEST_SECONDS = int(os.environ.get("T699_GUEST_TARGET", "1200"))
POLL_SECONDS = 5
GDB_PORT = os.environ.get("T699_GDB_PORT", "1242")

INITRD_ADDR = "0x110000000"

EXPECTED = {
    "kernel": KERNEL_SHA,
    "dtb": "e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389",
    "system_img": "98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d",
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
    path.parent.mkdir(parents=True, exist_ok=True)
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
        "dtb": DTB,
        "boot_image": BOOT_IMAGE,
        "system_img": SYSTEM_IMG,
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
        print(f"[PREFLIGHT] {name}: {digest} ({path.stat().st_size} bytes) OK", flush=True)
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
    step_num = 3 if "flight-01" in FLIGHT_NAME else 4
    hb = {
        "timestamp_utc": utc_now(),
        "status": status,
        "track": TRACK,
        "flight": FLIGHT_NAME,
        "current_step": step_num,
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
    print(f"[SUPERVISOR] Disk check passed: {free_gib:.2f} GiB available on /System/Volumes/Data.", flush=True)

    FLIGHT.mkdir(parents=True, exist_ok=True)
    LOG.parent.mkdir(parents=True, exist_ok=True)
    inputs = verify_inputs()

    boot_sha = inputs["boot_image"]["sha256"]

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
        "-dtb", str(DTB),
        "-device", f"loader,file={BOOT_IMAGE},addr={INITRD_ADDR},force-raw=on",
        "-drive", f"if=none,id=t699system,file={SYSTEM_IMG},format=raw,readonly=on",
        "-device", "virtio-blk-device,drive=t699system",
        "-append", (
            "console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk "
            "androidboot.hardware=virt androidboot.selinux=permissive "
            "androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 "
            "brd.rd_nr=1 brd.rd_size=524288 "
            f"androidboot.t699_fix_initramfs_sha256={boot_sha} "
            f"androidboot.t699_flight_target={TARGET_GUEST_SECONDS} "
            "androidboot.t699_dtb_sha256=e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389 "
            "androidboot.t699_system_sha256=98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d "
            f"androidboot.t699_kernel_sha256={KERNEL_SHA} "
            f"androidboot.t683_fix_initramfs_sha256={boot_sha} "
            f"androidboot.t683_flight_target={TARGET_GUEST_SECONDS} "
            "androidboot.t683_dtb_sha256=e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389 "
            "androidboot.t683_system_sha256=98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d"
        ),
        "-gdb", f"tcp:127.0.0.1:{GDB_PORT}",
        "-display", "none",
        "-serial", f"file:{LOG}",
        "-monitor", "none",
        "-no-reboot",
    ]

    (FLIGHT / "qemu-command.txt").write_text(" ".join(cmd) + "\n")

    preflight = {
        "track": TRACK,
        "flight": FLIGHT_NAME,
        "created_utc": utc_now(),
        "qemu_version": qemu_version,
        "command": cmd,
        "inputs": inputs,
    }
    atomic_json(FLIGHT / "preflight.json", preflight)

    for p in [
        LOG,
        STDERR,
        FLIGHT / "flight-end.json",
        FLIGHT / "uart.log.numbered.txt",
        FLIGHT / "uart.log.gz",
    ]:
        if p.exists():
            p.unlink()

    print(f"[SUPERVISOR] Launching QEMU 11.1.2 {TRACK} {FLIGHT_NAME} guest flight...", flush=True)
    started_utc = utc_now()
    started = time.monotonic()
    last_hb = -100.0

    with open(STDERR, "wb") as stderr_f:
        proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=stderr_f)
        update_heartbeat("QEMU guest launched", 0.0, 0.0, proc.pid)
        print(f"[SUPERVISOR] QEMU pid={proc.pid} started at {started_utc}", flush=True)

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
                    f"[SUPERVISOR {utc_now()}] host_elapsed={elapsed:.1f}s guest_max={guest_seconds:.1f}s pid={proc.pid}",
                    flush=True,
                )
                update_heartbeat("Flight in progress", elapsed, guest_seconds, proc.pid)

            time.sleep(POLL_SECONDS)

    ended_utc = utc_now()
    host_elapsed = time.monotonic() - started
    final_guest = guest_log_max_seconds()
    print(f"[SUPERVISOR] Flight ended at {ended_utc} ({stop_reason})", flush=True)
    print(f"[SUPERVISOR] Host elapsed: {host_elapsed:.2f}s, Guest log max: {final_guest:.2f}s", flush=True)

    log_bytes = LOG.stat().st_size if LOG.exists() else 0
    log_sha = sha256_file(LOG) if LOG.exists() else ""
    log_lines = 0

    if LOG.exists():
        print("[SUPERVISOR] Generating uart.log.numbered.txt and uart.log.gz...", flush=True)
        numbered_path = FLIGHT / "uart.log.numbered.txt"
        gz_path = FLIGHT / "uart.log.gz"
        with open(LOG, "rb") as fin, open(numbered_path, "w", encoding="utf-8", errors="replace") as fnum, gzip.open(gz_path, "wb") as fgz:
            for idx, raw_line in enumerate(fin, start=1):
                log_lines += 1
                fgz.write(raw_line)
                decoded = raw_line.decode("latin1", errors="replace").rstrip("\r\n")
                fnum.write(f"{idx:6d}: {decoded}\n")

    flight_end = {
        "track": TRACK,
        "flight": FLIGHT_NAME,
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
    print(f"[SUPERVISOR] flight-end.json written. Flight {FLIGHT} complete.", flush=True)


if __name__ == "__main__":
    main()
