#!/usr/bin/env python3
"""T727 service-flight supervisor for the toybox-service discriminator (Mac QEMU).

Same stack as the preceding lanes (kernel t720-Image, CPIO t723-fix.cpio.gz, DTB
t723-modified.dtb) EXCEPT the system image is t727-system.img: the T721
revert image plus /system/etc/init/t727probe.rc, a trigger file on the
proven prior-lane shape (one service running /system/bin/toybox sh -c echo to
/dev/kmsg, two actions on property:sys.boot_completed=1 and on boot that
both write the marker T727_PROBE_OK to /dev/kmsg). Vendor.img is the T722
vendor image (patched stub) as a consistency check only: the guest loop-mounts
the CPIO-embedded copy.

T722 LESSON BUILT IN: the T720 supervisor's mid-flight check_disk_space()
called sys.exit(10) while QEMU was running, orphaning QEMU (manual SIGTERM
needed). Here the mid-flight disk gate terminates QEMU cleanly, waits for
it, writes flight-end.json, and THEN exits 10. No exit path leaves QEMU
running.

A flight re-run NEVER reuses a completed flight's folder: re-runs use a
new T727_FLIGHT name (e.g. service-flight-2), never the same name.
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
TRACK = "T727"

KERNEL = Path(os.environ.get("T727_KERNEL", str(WORKTREE / "guest-inputs/t720-Image")))
KERNEL_SHA = os.environ.get(
    "T727_KERNEL_SHA",
    "70d3859d6aed1906d27217aa3fdfd9bb953a57f4490c89c51efa4f6490441e04",
)
FLIGHT_NAME = os.environ.get("T727_FLIGHT", "service-flight")
FLIGHT = WORKTREE / "flights" / FLIGHT_NAME
LOG = Path(os.environ.get("T727_LOG", str(FLIGHT / "uart.log")))
STDERR = FLIGHT / "qemu-stderr.log"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
DTB = Path(os.environ.get("T727_DTB", str(WORKTREE / "guest-inputs/t723-modified.dtb")))
BOOT_IMAGE = Path(os.environ.get("T727_BOOT_IMAGE", str(WORKTREE / "guest-inputs/t723-fix.cpio.gz")))
SYSTEM_IMG = Path(os.environ.get("T727_SYSTEM_IMG", str(WORKTREE / "guest-inputs/t727-system.img")))
VENDOR_IMG = Path(os.environ.get("T727_VENDOR_IMG", str(WORKTREE / "guest-inputs/t722-vendor.img")))

HOST_CAP_SECONDS = int(os.environ.get("T727_HOST_CAP", "700"))
TARGET_GUEST_SECONDS = int(os.environ.get("T727_GUEST_TARGET", "600"))
TARGET_HOST_SECONDS = HOST_CAP_SECONDS
POLL_SECONDS = 5
GDB_PORT = os.environ.get("T727_GDB_PORT", "1258")

INITRD_ADDR = "0x110000000"

EXPECTED = {
    "kernel": KERNEL_SHA,
    "dtb": os.environ.get("T727_DTB_SHA", "73d1904b3191edc3c36082f4cec744944ff56fe6b947dd6910d25d4b2a858eea"),
    "system_img": os.environ.get("T727_SYSTEM_IMG_SHA", "8e9914e3444bdac2cad65771dc8cbc1d8a1490fc5ba26fa276a7ba8878fe9bd7"),
    "boot_image": os.environ.get("T727_BOOT_IMAGE_SHA", "a9d6040a3984e9d60837187d3b5d02211002966e4f051f23765d1f05390f3521"),
    "vendor_img": os.environ.get("T727_VENDOR_IMG_SHA", "5e0607ada51367fbe9f7e5dd7586afbeb9216f478f00a31baa5179eb73f476e0"),
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
    temp.replace(path)

def free_gib() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    return (stat.f_bavail * stat.f_frsize) / (1024 ** 3)

def check_disk_space() -> float:
    free = free_gib()
    if free < 8.0:
        print(f"[FATAL] Disk space below 8 GiB threshold: {free:.2f} GiB available.", file=sys.stderr)
        sys.exit(10)
    return free

def verify_inputs() -> dict:
    files = {
        "kernel": KERNEL,
        "qemu": QEMU,
        "dtb": DTB,
        "boot_image": BOOT_IMAGE,
        "system_img": SYSTEM_IMG,
        "vendor_img": VENDOR_IMG,
    }
    inputs_meta = {}
    for name, path in files.items():
        if not path.exists():
            raise FileNotFoundError(f"Missing required input: {name} at {path}")
        sha = sha256_file(path)
        expected = EXPECTED.get(name)
        if expected and sha != expected:
            raise ValueError(f"Checksum mismatch for {name}: expected {expected}, got {sha}")
        inputs_meta[name] = {
            "path": str(path),
            "sha256": sha,
            "bytes": path.stat().st_size,
        }
    return inputs_meta

def guest_log_max_seconds() -> float:
    if not LOG.exists():
        return 0.0
    max_sec = 0.0
    try:
        with open(LOG, "rb") as f:
            f.seek(max(0, LOG.stat().st_size - 1024 * 1024))
            for line in f:
                m = GUEST_TIME_RE.match(line)
                if m:
                    sec = float(m.group(1))
                    if sec > max_sec:
                        max_sec = sec
    except Exception:
        pass
    return max_sec

def update_heartbeat(status: str, elapsed: float, guest_seconds: float, pid: int | None) -> None:
    hb = {
        "timestamp_utc": utc_now(),
        "status": status,
        "track": TRACK,
        "flight": FLIGHT_NAME,
        "current_step": f"Flight {FLIGHT_NAME} ({TARGET_GUEST_SECONDS}s guest target)",
        "verdict": None,
        "host_elapsed_seconds": round(elapsed, 2),
        "guest_log_max_seconds": round(guest_seconds, 3),
        "target_host_seconds": TARGET_HOST_SECONDS,
        "target_guest_seconds": TARGET_GUEST_SECONDS,
        "supervisor_pid": os.getpid(),
        "qemu_pid": pid,
        "note": f"Running {TRACK} {FLIGHT_NAME} flight",
    }
    atomic_json(WORKTREE / "heartbeat.json", hb)

def stop_qemu(proc: subprocess.Popen) -> None:
    proc.terminate()
    try:
        proc.wait(timeout=15)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()

def main() -> None:
    check_disk_space()
    FLIGHT.mkdir(parents=True, exist_ok=True)

    print(f"[{TRACK} SUPERVISOR] Verifying flight inputs...", flush=True)
    inputs = verify_inputs()
    qemu_version_out = subprocess.check_output([str(QEMU), "--version"], text=True)
    qemu_version = qemu_version_out.splitlines()[0]

    cmd = [
        str(QEMU),
        "-machine", "virt,gic-version=2,virtio-mmio-transports=1",
        "-cpu", "cortex-a57",
        "-m", "8192",
        "-smp", "4",
        "-kernel", str(KERNEL),
        "-dtb", str(DTB),
        "-device", f"loader,file={BOOT_IMAGE},addr={INITRD_ADDR},force-raw=on",
        "-drive", f"if=none,id=t720system,file={SYSTEM_IMG},format=raw,readonly=on",
        "-device", "virtio-blk-device,drive=t720system",
        "-append",
        (
            f"console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk "
            f"androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 "
            f"printk.ratelimit=0 brd.rd_nr=1 brd.rd_size=524288 "
            f"androidboot.t720_fix_initramfs_sha256={inputs['boot_image']['sha256']} "
            f"androidboot.t720_flight_target={TARGET_GUEST_SECONDS} "
            f"androidboot.t720_dtb_sha256={inputs['dtb']['sha256']} "
            f"androidboot.t720_system_sha256={inputs['system_img']['sha256']} "
            f"androidboot.t720_kernel_sha256={inputs['kernel']['sha256']}"
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

    print(f"[SUPERVISOR] Launching QEMU {TRACK} {FLIGHT_NAME} guest flight...", flush=True)
    started_utc = utc_now()
    started = time.monotonic()
    last_hb = -100.0
    last_progress_txt_hb = time.monotonic()
    disk_gate_fired = False

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
                stop_qemu(proc)
                break
            if elapsed >= TARGET_HOST_SECONDS:
                stop_reason = f"Host timeout reached ({elapsed:.1f}s >= {TARGET_HOST_SECONDS}s)"
                stop_qemu(proc)
                break

            if elapsed - last_hb >= 15.0:
                last_hb = elapsed
                free = free_gib()
                if free < 8.0:
                    stop_reason = f"Disk gate fired mid-flight ({free:.2f} GiB < 8 GiB)"
                    print(f"[SUPERVISOR] {stop_reason}; terminating QEMU cleanly.", flush=True)
                    stop_qemu(proc)
                    disk_gate_fired = True
                    break
                print(
                    f"[SUPERVISOR {utc_now()}] host_elapsed={elapsed:.1f}s guest_max={guest_seconds:.1f}s pid={proc.pid}",
                    flush=True,
                )
                update_heartbeat("Flight in progress", elapsed, guest_seconds, proc.pid)

            if elapsed - (last_progress_txt_hb - started) >= 300.0:
                last_progress_txt_hb = time.monotonic()
                with open(WORKTREE / "progress-t727.txt", "a", encoding="utf-8") as pf:
                    pf.write(f"Flight in progress ({FLIGHT_NAME}): host={elapsed:.0f}s guest={guest_seconds:.1f}s pid={proc.pid}. {utc_now()}\n")

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

    exit_code = proc.returncode
    if "reached" in stop_reason.lower() and exit_code in (0, -15, 15):
        exit_code = 0

    flight_end = {
        "track": TRACK,
        "flight": FLIGHT_NAME,
        "stop_reason": stop_reason,
        "started_utc": started_utc,
        "ended_utc": ended_utc,
        "host_elapsed_seconds": round(host_elapsed, 2),
        "guest_log_max_seconds": round(final_guest, 3),
        "qemu_exit_code": exit_code,
        "log_bytes": log_bytes,
        "log_lines": log_lines,
        "log_sha256": log_sha,
        "inputs": inputs,
    }
    atomic_json(FLIGHT / "flight-end.json", flight_end)
    update_heartbeat(f"Flight complete ({stop_reason})", host_elapsed, final_guest, None)
    print(f"[SUPERVISOR] flight-end.json written. Flight {FLIGHT} complete.", flush=True)
    if disk_gate_fired:
        sys.exit(10)

if __name__ == "__main__":
    main()
