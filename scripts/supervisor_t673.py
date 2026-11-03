#!/usr/bin/env python3
"""Supervisor for Track T673 QEMU 11.1.2 guest flight with 1200s guest target.
Stage 5 Display: Frame-commit verification into SimpleFB (fb0 at 0x5F000000).
"""
from __future__ import annotations

import datetime
import gzip
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
FLIGHT = WORKTREE / "flights/flight-01"
LOG = FLIGHT / "uart.log"
STDERR = FLIGHT / "qemu-stderr.log"
QMP_SOCK = FLIGHT / "qmp.sock"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
KERNEL = WORKTREE / "guest-inputs/Image"
FLIGHT_IMAGE = WORKTREE / "guest-inputs/t670-base-image.cpio.gz"

TARGET_HOST_SECONDS = 1500
TARGET_GUEST_SECONDS = 1200
POLL_SECONDS = 5

EXPECTED = {
    "kernel": "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
    "flight_image": "afb2991f59288d1de1b9855b6cbcb6e8050bde3e9a23d2391d3f17c892a3d022",
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
        "flight_image": FLIGHT_IMAGE,
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
    try:
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
    except Exception as e:
        print(f"[WARN] Error reading guest log: {e}", flush=True)
    return last


def update_heartbeat(status: str, elapsed: float, guest_seconds: float, pid: int | None = None) -> None:
    hb = {
        "timestamp_utc": utc_now(),
        "status": status,
        "track": "T673",
        "flight": "flight-01",
        "current_step": 3,
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


class QMPClient:
    """Minimal QMP client for taking physical framebuffer dumps."""
    def __init__(self, sock_path: Path):
        self.sock_path = sock_path
        self.sock: socket.socket | None = None
        self.connected = False

    def connect(self, timeout: float = 2.0) -> bool:
        if self.connected:
            return True
        if not self.sock_path.exists():
            return False
        try:
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.settimeout(timeout)
            s.connect(str(self.sock_path))
            # Read greeting
            data = b""
            while b"\r\n" not in data:
                chunk = s.recv(1024)
                if not chunk:
                    break
                data += chunk
            # Negotiate capabilities
            s.sendall(json.dumps({"execute": "qmp_capabilities"}).encode("utf-8") + b"\r\n")
            resp = b""
            while b"\r\n" not in resp:
                chunk = s.recv(1024)
                if not chunk:
                    break
                resp += chunk
            self.sock = s
            self.connected = True
            print("[QMP] Connected and negotiated capabilities successfully", flush=True)
            return True
        except Exception as e:
            print(f"[QMP] Connection error: {e}", flush=True)
            if self.sock:
                try:
                    self.sock.close()
                except Exception:
                    pass
                self.sock = None
            self.connected = False
            return False

    def pmemsave(self, addr: int, size: int, out_path: Path) -> bool:
        if not self.connected:
            if not self.connect():
                return False
        cmd = f'pmemsave {hex(addr)} {size} "{out_path}"'
        req = {
            "execute": "human-monitor-command",
            "arguments": {"command-line": cmd}
        }
        try:
            self.sock.sendall(json.dumps(req).encode("utf-8") + b"\r\n")
            resp = b""
            while b"\r\n" not in resp:
                chunk = self.sock.recv(4096)
                if not chunk:
                    break
                resp += chunk
            res = json.loads(resp.decode("utf-8"))
            if "error" in res:
                print(f"[QMP] pmemsave error: {res['error']}", flush=True)
                return False
            print(f"[QMP] pmemsave({hex(addr)}, {size}) -> {out_path} OK", flush=True)
            return True
        except Exception as e:
            print(f"[QMP] Error executing pmemsave: {e}", flush=True)
            self.connected = False
            return False

    def close(self):
        if self.sock:
            try:
                self.sock.close()
            except Exception:
                pass
            self.sock = None
        self.connected = False


def main():
    free_gib = check_disk_space()
    print(f"[SUPERVISOR] Disk check passed: {free_gib:.2f} GiB available on /System/Volumes/Data.", flush=True)

    FLIGHT.mkdir(parents=True, exist_ok=True)
    inputs = verify_inputs()

    qemu_version = subprocess.run(
        [str(QEMU), "--version"], capture_output=True, text=True, check=True
    ).stdout.splitlines()[0]

    flight_sha = inputs["flight_image"]["sha256"]
    cmd = [
        str(QEMU),
        "-machine", "virt,gic-version=2,virtio-mmio-transports=1",
        "-cpu", "cortex-a57",
        "-m", "8192",
        "-smp", "4",
        "-kernel", str(KERNEL),
        "-initrd", str(FLIGHT_IMAGE),
        "-append", f"console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 brd.rd_nr=1 brd.rd_size=524288 androidboot.t670_fix_initramfs_sha256={flight_sha} androidboot.t673_flight_target=1200 androidboot.t673_stage5_display=1",
        "-display", "none",
        "-serial", f"file:{LOG}",
        "-qmp", f"unix:{QMP_SOCK},server,nowait",
        "-no-reboot",
    ]

    (FLIGHT / "qemu-command.txt").write_text(" ".join(cmd) + "\n")

    preflight = {
        "track": "T673",
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
        QMP_SOCK,
        FLIGHT / "flight-end.json",
        FLIGHT / "uart.log.numbered.txt",
        FLIGHT / "uart.log.gz",
        FLIGHT / "simplefb-snapshot-01.bin",
        FLIGHT / "simplefb-snapshot-02.bin",
        FLIGHT / "simplefb-snapshot-03.bin",
    ]:
        if p.exists():
            p.unlink()

    print("[SUPERVISOR] Launching QEMU 11.1.2 guest flight...", flush=True)
    started_utc = utc_now()
    started = time.monotonic()
    last_hb = -100.0

    qmp = QMPClient(QMP_SOCK)
    snapshot_times = [600, 900, 1150]
    snapshots_taken = {}

    with open(STDERR, "wb") as stderr_f:
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=stderr_f)
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

            # Try taking scheduled snapshots via QMP
            for target_sec in snapshot_times:
                snap_name = f"snapshot_{target_sec}s"
                if guest_seconds >= target_sec and snap_name not in snapshots_taken:
                    snap_idx = len(snapshots_taken) + 1
                    snap_path = FLIGHT / f"simplefb-snapshot-0{snap_idx}.bin"
                    print(f"[SUPERVISOR] Taking SimpleFB snapshot at guest_uptime={guest_seconds:.1f}s -> {snap_path.name}", flush=True)
                    ok = qmp.pmemsave(0x5F000000, 3145728, snap_path)
                    snapshots_taken[snap_name] = {
                        "guest_seconds": guest_seconds,
                        "host_elapsed": elapsed,
                        "path": str(snap_path),
                        "success": ok,
                    }

            if guest_seconds >= TARGET_GUEST_SECONDS:
                stop_reason = f"Guest target reached ({guest_seconds:.1f}s >= {TARGET_GUEST_SECONDS}s)"
                # Final snapshot before terminating
                final_snap_path = FLIGHT / f"simplefb-snapshot-final.bin"
                qmp.pmemsave(0x5F000000, 3145728, final_snap_path)
                qmp.close()
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                break
            if elapsed >= TARGET_HOST_SECONDS:
                stop_reason = f"Host timeout reached ({elapsed:.1f}s >= {TARGET_HOST_SECONDS}s)"
                qmp.close()
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
                    flush=True
                )
                update_heartbeat("Flight in progress", elapsed, guest_seconds, proc.pid)

            time.sleep(POLL_SECONDS)

    qmp.close()
    ended_utc = utc_now()
    host_elapsed = time.monotonic() - started
    final_guest = guest_log_max_seconds()
    print(f"[SUPERVISOR] Flight ended at {ended_utc} ({stop_reason})", flush=True)
    print(f"[SUPERVISOR] Host elapsed: {host_elapsed:.2f}s, Guest log max: {final_guest:.2f}s", flush=True)

    # Process and record snapshot evidence
    snapshot_records = {}
    for snap_file in sorted(FLIGHT.glob("simplefb-snapshot-*.bin")):
        snap_sha = sha256_file(snap_file)
        snap_sz = snap_file.stat().st_size
        snapshot_records[snap_file.name] = {
            "path": str(snap_file),
            "bytes": snap_sz,
            "sha256": snap_sha,
        }
        print(f"[SNAPSHOT] {snap_file.name}: {snap_sz} bytes, SHA-256 {snap_sha}", flush=True)

    # Compare snapshots if multiple exist
    snapshot_diff_regions = 0
    snapshot_differing_bytes = 0
    snap_list = list(snapshot_records.keys())
    if len(snap_list) >= 2:
        file1 = FLIGHT / snap_list[0]
        file2 = FLIGHT / snap_list[1]
        with open(file1, "rb") as f1, open(file2, "rb") as f2:
            b1 = f1.read()
            b2 = f2.read()
            diff_bytes = sum(1 for x, y in zip(b1, b2) if x != y)
            snapshot_differing_bytes = diff_bytes
            print(f"[SNAPSHOT] Comparison between {file1.name} and {file2.name}: {diff_bytes} differing bytes", flush=True)

    # Generate compressed log and numbered line log for rapid analysis
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
        "track": "T673",
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
        "snapshots": snapshot_records,
        "snapshot_differing_bytes": snapshot_differing_bytes,
        "inputs": inputs,
    }
    atomic_json(FLIGHT / "flight-end.json", flight_end)
    update_heartbeat(f"Flight complete ({stop_reason})", host_elapsed, final_guest, None)
    print(f"[SUPERVISOR] flight-end.json written. Flight {FLIGHT} complete.", flush=True)


if __name__ == "__main__":
    main()
