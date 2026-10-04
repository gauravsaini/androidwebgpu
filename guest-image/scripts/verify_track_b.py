#!/usr/bin/env python3
"""Validation harness for Track B: initramfs + DTB boot under QEMU ARM64.

Verifies:
  1. Boot of AOSP ARM64 kernel Image + minimal-virt.dtb + initramfs.cpio.
  2. Linux mounts initramfs rootfs and executes /init (PID 1).
  3. UART markers are observed on ttyAMA0 console.
  4. RAM markers are executed by static init without panic.

Usage: python3 guest-image/scripts/verify_track_b.py
"""
import os
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

KERNEL_CANDIDATES = [
    "/Users/Shared/androidwebgpu/androidwebgpu/guest-image/Image",
    os.path.join(REPO_ROOT, "guest-image/Image"),
    "/mnt/sdb1/aosp/Image",
]

INITRD_CANDIDATES = [
    "/Users/Shared/initramfs.cpio",
    os.path.join(REPO_ROOT, "guest-image/initramfs.cpio"),
    "/mnt/sdb1/initramfs.cpio",
]

DTB_PATH = os.path.join(REPO_ROOT, "guest-image/minimal-virt.dtb")

EXPECTED_MARKERS = [
    "[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)",
    "[PATHN-TRACK-B] UART MARKER: PL011 TTYAMA0 DRIVER AT 0x09000000 OK",
    "[PATHN-TRACK-B] RAM MARKER: MAGIC 0x504154484E5F5241 0x4D5F4D41524B4552 OK",
    "[PATHN-TRACK-B] STATUS: PASS - USERSPACE INITIALIZATION COMPLETE",
]


def find_file(candidates):
    for c in candidates:
        if os.path.exists(c):
            return c
    return None


def main():
    kernel = find_file(KERNEL_CANDIDATES)
    if not kernel:
        print("ERROR: Kernel Image not found in candidates:", KERNEL_CANDIDATES)
        sys.exit(1)

    initrd = find_file(INITRD_CANDIDATES)
    if not initrd:
        print("ERROR: initramfs.cpio not found in candidates:", INITRD_CANDIDATES)
        sys.exit(1)

    if not os.path.exists(DTB_PATH):
        print("ERROR: DTB not found at", DTB_PATH)
        sys.exit(1)

    # Locate qemu-system-aarch64
    qemu_bin = None
    for p in ["/opt/homebrew/bin/qemu-system-aarch64", "/usr/local/bin/qemu-system-aarch64", "qemu-system-aarch64"]:
        if os.path.exists(p) or subprocess.run(["which", p], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            qemu_bin = p
            break

    if not qemu_bin:
        print("ERROR: qemu-system-aarch64 not found in system")
        sys.exit(1)

    cmd = [
        qemu_bin,
        "-M", "virt",
        "-cpu", "cortex-a53",
        "-m", "1024",
        "-kernel", kernel,
        "-initrd", initrd,
        "-dtb", DTB_PATH,
        "-nographic",
    ]

    print("[track-b-verify] Running QEMU:")
    print(" ", " ".join(cmd))

    try:
        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=20)
        output = proc.stdout.decode("utf-8", errors="ignore")
    except subprocess.TimeoutExpired as e:
        output = e.stdout.decode("utf-8", errors="ignore") if e.stdout else ""

    # Verify markers
    missing = []
    for m in EXPECTED_MARKERS:
        if m not in output:
            missing.append(m)

    if missing:
        print("[track-b-verify] FAILED - Missing expected UART markers:")
        for m in missing:
            print("  MISSING:", m)
        print("\nLast 40 lines of output:")
        print("\n".join(output.splitlines()[-40:]))
        sys.exit(1)

    print("\n[track-b-verify] SUCCESS - All UART markers observed in QEMU boot output:")
    for m in EXPECTED_MARKERS:
        print("  [x]", m)
    return 0


if __name__ == "__main__":
    sys.exit(main())
