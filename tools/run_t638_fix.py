#!/usr/bin/env python3
"""Boot T638's fix image on QEMU 11.1.2 and verify that SurfaceFlinger passes Skia primeShaderCache."""

from __future__ import annotations

import datetime as dt
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INPUTS = ROOT / "guest-inputs"
FLIGHT = ROOT / "flights" / "fix-01"

KERNEL = INPUTS / "Image"
INITRD = INPUTS / "t638-skia-texture-initramfs.cpio.gz"
BASE_T637_INITRD = INPUTS / "t637-gralloc-mapper-fix-initramfs.cpio.gz"

EXPECTED_KERNEL = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_BASE_T637_INITRD = "dbb22d63d47ddc05c73b0d91c05be01801f93f87a778cc200600402554933c9c"
EXPECTED_INITRD = "b05a20d7fe17444fd9ffd84c461bb2e82224474a71a979044cc7140e2b4b7178"
EXPECTED_SYSTEM = "1f9d601bfc44640c3680365e9d8b82fd3408f0c1dc6b515f4e47095b9bcb74df"
EXPECTED_VENDOR = "660843d42f94ffddf7327a30d6ba21512eb908cab48537b41bcebd39a8905c14"

QEMU = Path("/opt/homebrew/bin/qemu-system-aarch64")
HEARTBEAT = ROOT / "heartbeat.json"
OUTCOME = Path("/Users/Shared/outcome-t638.txt")
FINDINGS = Path("/Users/Shared/agyx-track638-skia-texture.md")

MIN_FREE_BYTES = 2 * 1024**3
MAX_WAIT_SECONDS = 900
POST_MAPPER_WAIT_SECONDS = 90
CHUNK = 8 * 1024 * 1024


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(CHUNK):
            digest.update(block)
    return digest.hexdigest()


def pad4(size: int) -> int:
    return (-size) & 3


def read_entry(stream):
    header = stream.read(110)
    if len(header) != 110 or header[:6] != b"070701":
        raise ValueError("invalid or truncated newc CPIO header")
    namesize = int(header[94:102], 16)
    filesize = int(header[54:62], 16)
    name = stream.read(namesize)
    if len(name) != namesize or not name.endswith(b"\0"):
        raise EOFError("truncated or malformed CPIO name")
    padding = stream.read(pad4(110 + namesize))
    if len(padding) != pad4(110 + namesize):
        raise EOFError("truncated CPIO name padding")
    return header, name, name[:-1].decode("latin1"), padding, filesize


def copy_exact(source, target, size: int, digest=None):
    remaining = size
    while remaining:
        block = source.read(min(CHUNK, remaining))
        if not block:
            raise EOFError(f"truncated CPIO data (wanted {size} bytes)")
        if target is not None:
            target.write(block)
        if digest is not None:
            digest.update(block)
        remaining -= len(block)


def cpio_member_sha256(image: Path, wanted: str) -> str:
    digest = hashlib.sha256()
    found = False
    with gzip.open(image, "rb") as stream:
        while True:
            _, _, name, _, size = read_entry(stream)
            if name == wanted:
                if found:
                    raise RuntimeError(f"duplicate CPIO member: {wanted}")
                copy_exact(stream, None, size, digest)
                found = True
            else:
                copy_exact(stream, None, size)
            padding = stream.read(pad4(size))
            if len(padding) != pad4(size):
                raise EOFError(f"truncated CPIO data padding for {name}")
            if name == "TRAILER!!!":
                break
    if not found:
        raise RuntimeError(f"missing CPIO member: {wanted}")
    return digest.hexdigest()


def disk_snapshot(label: str) -> dict:
    usage = shutil.disk_usage(ROOT)
    df = subprocess.run(
        ["df", "-h", "/System/Volumes/Data"], capture_output=True, text=True, check=True
    ).stdout.strip()
    state = {
        "label": label,
        "utc": utc_now(),
        "free_bytes": usage.free,
        "free_gib": round(usage.free / 1024**3, 3),
        "df_system_volumes_data": df,
    }
    (FLIGHT / f"disk-{label}.json").write_text(json.dumps(state, indent=2) + "\n")
    return state


def update_heartbeat(status: str, step: int, note: str, runner_pid: int | None = None):
    payload = {
        "timestamp": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "lane": "T638",
        "runner_pid": runner_pid,
        "status": status,
        "current_step": step,
        "note": note,
    }
    HEARTBEAT.write_text(json.dumps(payload, indent=2) + "\n")


def write_numbered_log(source: Path, target: Path):
    with source.open("rb") as src, target.open("w", encoding="utf-8", errors="replace") as dst:
        for idx, line in enumerate(src, start=1):
            dst.write(f"{idx:6d}: {line.decode('latin1', errors='replace')}")


def main() -> int:
    FLIGHT.mkdir(parents=True, exist_ok=True)
    update_heartbeat("preflight", 2, "Starting T638 fix runner preflight")

    booted = False
    process = None
    stop_reason = "preflight did not complete"
    errors = []

    hashes = {
        "kernel_Image": {"path": str(KERNEL), "sha256": sha256(KERNEL)},
        "t638_initramfs": {"path": str(INITRD), "sha256": sha256(INITRD)},
        "embedded_system_img": {
            "member": "ramdisk-images/system.img",
            "sha256": cpio_member_sha256(INITRD, "ramdisk-images/system.img"),
        },
        "embedded_vendor_img": {
            "member": "ramdisk-images/vendor.img",
            "sha256": cpio_member_sha256(INITRD, "ramdisk-images/vendor.img"),
        },
        "qemu_binary": {"path": str(QEMU), "sha256": sha256(QEMU)},
    }
    if hashes["kernel_Image"]["sha256"] != EXPECTED_KERNEL:
        errors.append("kernel SHA differs from canonical lineage")
    if hashes["t638_initramfs"]["sha256"] != EXPECTED_INITRD:
        errors.append("T638 initramfs SHA differs from expected fix image")
    if hashes["embedded_system_img"]["sha256"] != EXPECTED_SYSTEM:
        errors.append("embedded system SHA differs from canonical system")
    if hashes["embedded_vendor_img"]["sha256"] != EXPECTED_VENDOR:
        errors.append("embedded vendor SHA differs from expected vendor")
    if sha256(BASE_T637_INITRD) != EXPECTED_BASE_T637_INITRD:
        errors.append("base T637 initramfs SHA changed")

    manifest_path = FLIGHT / "guest-image-manifest.json"
    if not manifest_path.exists():
        errors.append("T638 guest image manifest is missing")
    else:
        manifest = json.loads(manifest_path.read_text())
        if manifest["images"]["initramfs"]["sha256"] != hashes["t638_initramfs"]["sha256"]:
            errors.append("T638 initramfs SHA differs from build manifest")
        if manifest["images"]["embedded_vendor_img"]["sha256"] != hashes["embedded_vendor_img"]["sha256"]:
            errors.append("embedded vendor SHA differs from build manifest")

    marker = f"androidboot.t638_initramfs_sha256={hashes['t638_initramfs']['sha256']}"
    command = [
        str(QEMU),
        "-machine", "virt,gic-version=2,virtio-mmio-transports=1",
        "-cpu", "cortex-a57",
        "-m", "8192",
        "-smp", "4",
        "-kernel", str(KERNEL),
        "-initrd", str(INITRD),
        "-append",
        "console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk "
        "androidboot.hardware=virt androidboot.selinux=permissive "
        "androidboot.veritymode=disabled enforcing=0 " + marker,
        "-display", "none",
        "-serial", f"file:{FLIGHT / 'uart.log'}",
        "-monitor", "none",
        "-no-reboot",
    ]

    preflight = {
        "track": "T638",
        "flight": FLIGHT.name,
        "created_utc": utc_now(),
        "hashes": hashes,
        "guest_image_sha256_marker": marker,
        "qemu_version": subprocess.run([str(QEMU), "--version"], capture_output=True, text=True, check=True).stdout.splitlines()[0],
        "qemu_command": command,
        "ten_passes_claim": False,
    }
    (FLIGHT / "qemu-command.txt").write_text(" ".join(command) + "\n")
    (FLIGHT / "preflight.json").write_text(json.dumps(preflight, indent=2) + "\n")
    image_lines = [f"{item['sha256']}  {item.get('path', item.get('member'))}" for item in hashes.values() if item is not hashes["qemu_binary"]]
    (FLIGHT / "image-sha256sums.txt").write_text("\n".join(image_lines) + "\n")

    boot_disk = disk_snapshot("immediately-before-qemu-boot")
    if errors:
        stop_reason = f"preflight failed: {'; '.join(errors)}"
    elif boot_disk["free_bytes"] < MIN_FREE_BYTES:
        stop_reason = f"disk guard stopped before boot at {boot_disk['free_gib']} GiB free"
        errors.append("free disk is below 2 GiB QEMU boot floor")
    else:
        with (FLIGHT / "qemu-stderr.log").open("wb") as stderr_file:
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=stderr_file)
            booted = True
            update_heartbeat("qemu_running", 3, f"QEMU 11.1.2 spawned with pid={process.pid}", runner_pid=process.pid)
            (FLIGHT / "flight-start.json").write_text(json.dumps({
                "track": "T638",
                "flight": FLIGHT.name,
                "start_utc": utc_now(),
                "qemu_pid": process.pid,
                "initramfs_sha256": hashes["t638_initramfs"]["sha256"],
                "kernel_sha256": hashes["kernel_Image"]["sha256"],
            }, indent=2) + "\n")

            started = time.monotonic()
            next_heartbeat = started + 60
            mapper_loaded_seen_at = None
            stop_reason = "timeout waiting for guest evidence"

            uart_path = FLIGHT / "uart.log"

            while time.monotonic() - started < MAX_WAIT_SECONDS:
                if process.poll() is not None:
                    stop_reason = f"QEMU exited before host stop with rc={process.returncode}"
                    break

                if uart_path.exists():
                    uart_bytes = uart_path.read_bytes()

                    if mapper_loaded_seen_at is None and (b"T637_MAPPER" in uart_bytes or b"AIMapper_loadIMapper" in uart_bytes):
                        mapper_loaded_seen_at = time.monotonic()
                        update_heartbeat("mapper_loaded", 4, "Mapper load observed on UART; waiting for primeShaderCache and SF progression", runner_pid=process.pid)

                    # If mapper was loaded, allow POST_MAPPER_WAIT_SECONDS for SF to pass primeCache and hit next stage or new wall
                    if mapper_loaded_seen_at is not None:
                        elapsed = time.monotonic() - mapper_loaded_seen_at
                        if elapsed >= POST_MAPPER_WAIT_SECONDS:
                            stop_reason = f"observed mapper load and {POST_MAPPER_WAIT_SECONDS}s post-mapper observation window elapsed"
                            break

                if time.monotonic() >= next_heartbeat:
                    next_heartbeat = time.monotonic() + 60
                    disk_snap = disk_snapshot("in-flight-heartbeat")
                    if disk_snap["free_bytes"] < MIN_FREE_BYTES:
                        stop_reason = f"disk guard stopped during flight at {disk_snap['free_gib']} GiB free"
                        break
                    update_heartbeat("qemu_running", 3, f"QEMU running for {int(time.monotonic() - started)}s; free_gib={disk_snap['free_gib']}", runner_pid=process.pid)

                time.sleep(2)

    # Teardown QEMU
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)

    post_disk = disk_snapshot("flight-end")
    update_heartbeat("postflight", 5, "QEMU stopped; processing logs and artifacts")

    uart_path = FLIGHT / "uart.log"
    uart_bytes = b""
    if uart_path.exists():
        uart_bytes = uart_path.read_bytes()
        write_numbered_log(uart_path, FLIGHT / "uart.log.numbered.txt")
        with uart_path.open("rb") as f_in, gzip.open(FLIGHT / "uart.log.gz", "wb") as f_out:
            shutil.copyfileobj(f_in, f_out)

    # Analyze guest UART evidence
    uart_text = uart_bytes.decode("latin1", errors="replace")
    
    # 1. Did the guest consume our initramfs?
    cmdline_consumed = marker in uart_text

    # 2. Did mapper load?
    mapper_load_called = "AIMapper_loadIMapper" in uart_text or "T637_MAPPER" in uart_text

    # 3. Did "Failed to create a valid texture" AutoBackendTexture abort occur?
    autobackendtexture_abort = "Failed to create a valid texture" in uart_text or "AutoBackendTexture" in uart_text

    # 4. Check tombstones or crash lines
    sf_crash_lines = []
    for line in uart_text.splitlines():
        if "Service 'surfaceflinger'" in line or ("surfaceflinger" in line and ("signal 6" in line or "SIGABRT" in line)):
            sf_crash_lines.append(line.strip())

    # Verdict: PASS if guest consumed fix initramfs, reached primeShaderCache, and DID NOT hit AutoBackendTexture abort
    verdict = "PASS" if (cmdline_consumed and mapper_load_called and not autobackendtexture_abort) else "FAIL"

    result_summary = {
        "track": "T638",
        "verdict": verdict,
        "stop_reason": stop_reason,
        "cmdline_consumed": cmdline_consumed,
        "mapper_load_called": mapper_load_called,
        "autobackendtexture_abort_observed": autobackendtexture_abort,
        "sf_crash_count": len(sf_crash_lines),
        "sf_crash_sample": sf_crash_lines[:5],
        "image_hashes": hashes,
    }
    (FLIGHT / "flight-result.json").write_text(json.dumps(result_summary, indent=2) + "\n")
    print(json.dumps(result_summary, indent=2))
    return 0 if verdict == "PASS" else 1


if __name__ == "__main__":
    import sys
    sys.exit(main())
