#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T655:
system-server-startotherservices-networkmanagementservice-netd-watchdog (Chain A)

1. Pinned Root Cause:
   In SystemServer.startOtherServices() at line 1968:
   NetworkManagementService.create(context) calls:
   NetworkManagementService.connectNativeNetdService() ->
   NetdService.get() queries ServiceManager.getService("netd") in a loop.
   In T589 (prepare_t589_stub.py), /system/etc/init/netd.rc (inode 717, data offset 0x17c9c000)
   was replaced with a dummy shell sleep stub:
     service netd /system/bin/sh -c "echo '[NETD-STUB] netd stub active' > /dev/kmsg; while true; do sleep 3600; done"
   This shell loop never registers INetd with ServiceManager.
   Furthermore, Android init failed to mount cgroup2 on /sys/fs/cgroup because of the
   "memory_recursiveprot" option in cgroups.json. When /system/bin/netd starts, it checks
   access to /sys/fs/cgroup; if unmounted, it exits with status 1.

2. Pinned Fix:
   a) In system.img (offset 0x17c9c000, 515 bytes):
      Restore service definition to invoke /system/bin/netd directly in netd domain,
      omitting 'updatable' and 'onrestart restart zygote' to prevent cascading crashes:
        service netd /system/bin/netd
            class main
            capabilities CHOWN DAC_OVERRIDE DAC_READ_SEARCH FOWNER IPC_LOCK KILL NET_ADMIN NET_BIND_SERVICE NET_RAW SETUID SETGID
            user root
            group root net_admin
            socket dnsproxyd stream 0660 root inet
            socket mdns stream 0660 root system
            socket fwmarkd stream 0660 root inet
   b) In t636-diag.sh:
      Mount cgroup2 on /sys/fs/cgroup (mode 0775, uid/gid 1000) so netd's cgroup2 check succeeds.
      Explicitly ensure netd is started.

3. Outputs:
   guest-image/t655-system-server-startotherservices-networkmanagementservice-netd-watchdog-initramfs.cpio.gz
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track655-system-server-startotherservices-networkmanagementservice-netd-watchdog")
BASE_IMAGE = ROOT / "guest-inputs/t654-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t655-system-server-startotherservices-networkmanagementservice-netd-watchdog-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t655-manifest.json"

EXPECTED_BASE_SHA256 = "2461d8bbb9c8dc1ea928c20d86cc15c1d1b7b75fce2a530202bab4d8f6d9af44"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2059816960
NETD_RC_OFFSET = 0x17c9c000  # 399101952
NETD_RC_PATCH_LEN = 515

OLD_STUB_PREFIX = (
    b"service netd /system/bin/sh -c \"echo '[NETD-STUB] netd stub active' > /dev/kmsg; while true; do sleep 3600; done\"\n"
    b"    class main\n"
    b"    seclabel u:r:shell:s0\n"
    b"    capabilities CHOWN DAC_OVERRIDE DAC_READ_SEARCH FOWNER IPC_LOCK KILL NET_ADMIN NET_BIND_SERVICE NET_RAW SETUID SETGID\n"
    b"    user root\n"
    b"    group root net_admin\n"
    b"    socket dnsproxyd stream 0660 root inet\n"
    b"    socket mdns stream 0660 root system\n"
    b"    socket fwmarkd stream 0660 root inet\n"
    b"    # b/121354779: netd stubbed for headless virt\n"
    b"    # updatable disabled\n"
)
assert len(OLD_STUB_PREFIX) == NETD_RC_PATCH_LEN, f"OLD_STUB_PREFIX len {len(OLD_STUB_PREFIX)} != {NETD_RC_PATCH_LEN}"

NEW_NETD_BODY = (
    b"service netd /system/bin/netd\n"
    b"    class main\n"
    b"    capabilities CHOWN DAC_OVERRIDE DAC_READ_SEARCH FOWNER IPC_LOCK KILL NET_ADMIN NET_BIND_SERVICE NET_RAW SETUID SETGID\n"
    b"    user root\n"
    b"    group root net_admin\n"
    b"    socket dnsproxyd stream 0660 root inet\n"
    b"    socket mdns stream 0660 root system\n"
    b"    socket fwmarkd stream 0660 root inet\n"
    b"# b/121354779: netd restored for T655\n"
)
PAD_COUNT = NETD_RC_PATCH_LEN - len(NEW_NETD_BODY) - 1
NEW_NETD_PATCH = NEW_NETD_BODY + b"#" * PAD_COUNT + b"\n"
assert len(NEW_NETD_PATCH) == NETD_RC_PATCH_LEN, f"NEW_NETD_PATCH len {len(NEW_NETD_PATCH)} != {NETD_RC_PATCH_LEN}"

CHUNK = 8 * 1024 * 1024


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def pad4(n: int) -> int:
    return (-n) & 3


def format_cpio_entry(name: str, content: bytes, mode: int = 0o100755, ino: int = 89999) -> bytes:
    name_bytes = name.encode("ascii") + b"\0"
    namesize = len(name_bytes)
    filesize = len(content)
    hdr = (
        f"070701"
        f"{ino:08x}"
        f"{mode:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{1:08x}"
        f"{0:08x}"
        f"{filesize:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{namesize:08x}"
        f"{0:08x}"
    ).encode("ascii")
    out = bytearray()
    out.extend(hdr)
    out.extend(name_bytes)
    pad_hdr = pad4(len(out))
    out.extend(b"\0" * pad_hdr)
    out.extend(content)
    pad_data = pad4(filesize)
    out.extend(b"\0" * pad_data)
    return bytes(out)


def build_diag_script(base_script: str) -> bytes:
    script = base_script

    anchor = '    mark "T654_FIX_APPLIED"\n'
    t655_check = """    # T655: Mount cgroup2 on /sys/fs/cgroup for netd and ensure netd is running
    mark "T655_PREP_CGROUP2"
    "$TOY" mkdir -p /sys/fs/cgroup 2>/dev/null || :
    "$TOY" mount -t cgroup2 none /sys/fs/cgroup 2>/dev/null || :
    "$TOY" chmod 0775 /sys/fs/cgroup 2>/dev/null || :
    "$TOY" chown 1000:1000 /sys/fs/cgroup 2>/dev/null || :
    /system/bin/start netd 2>/dev/null || :
    "$TOY" sleep 0.2 2>/dev/null || :
    mark "T655_FIX_APPLIED"
"""
    if anchor not in script:
        raise ValueError("Could not find anchor 'mark \"T654_FIX_APPLIED\"' in base diag script")
    script = script.replace(anchor, anchor + "\n" + t655_check, 1)

    boot_anchor = 'sensors_hal=$(/system/bin/getprop init.svc.vendor.sensors-hal-multihal 2>/dev/null)\n        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)'
    boot_replace = ('sensors_hal=$(/system/bin/getprop init.svc.vendor.sensors-hal-multihal 2>/dev/null)\n'
                    '        netd_svc=$(/system/bin/getprop init.svc.netd 2>/dev/null)\n'
                    '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)')
    if boot_anchor in script:
        script = script.replace(boot_anchor, boot_replace, 1)
        script = script.replace('sensors_hal=$sensors_hal boot_completed=$completed"', 'sensors_hal=$sensors_hal netd=$netd_svc boot_completed=$completed"', 1)

    return script.encode("utf-8")


def main():
    t0 = time.monotonic()
    print("[T655 BUILDER] Starting T655 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T655 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T655 BUILDER] Verified T654 base image SHA: {base_sha}")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T655 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
    netd_rc_patched = False
    diag_patched = False

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if hdr[:6] != b"070701":
                raise ValueError(f"Invalid CPIO header magic: {hdr[:6]}")

            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "ramdisk-images/system.img":
                print(f"[T655 BUILDER] Found system.img ({filesize} bytes), streaming and patching netd.rc at 0x{NETD_RC_OFFSET:x}...")
                if filesize != SYSTEM_IMG_SIZE:
                    raise ValueError(f"system.img size mismatch: {filesize} != {SYSTEM_IMG_SIZE}")

                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                cur_offset = 0
                rem = filesize
                while rem > 0:
                    read_len = min(rem, CHUNK)
                    chunk = bytearray(fin.read(read_len))
                    chunk_start = cur_offset
                    chunk_end = cur_offset + len(chunk)

                    if chunk_start <= NETD_RC_OFFSET < chunk_end:
                        local_offset = NETD_RC_OFFSET - chunk_start
                        old_bytes = bytes(chunk[local_offset:local_offset + NETD_RC_PATCH_LEN])
                        if old_bytes != OLD_STUB_PREFIX:
                            raise ValueError(f"Mismatch at 0x{NETD_RC_OFFSET:x} in system.img: expected {OLD_STUB_PREFIX[:50]}, got {old_bytes[:50]}")
                        print(f"[T655 BUILDER] Found old stub at offset 0x{NETD_RC_OFFSET:x}, applying new netd.rc ({NETD_RC_PATCH_LEN} bytes)...")
                        chunk[local_offset:local_offset + NETD_RC_PATCH_LEN] = NEW_NETD_PATCH
                        netd_rc_patched = True

                    fout.write(chunk)
                    cur_offset += len(chunk)
                    rem -= len(chunk)

                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

            elif name_str == "t636-diag.sh":
                print(f"[T655 BUILDER] Reading and updating t636-diag.sh ({filesize} bytes)...")
                base_diag_bytes = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_diag_bytes = build_diag_script(base_diag_bytes.decode("latin1"))
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_bytes)
                fout.write(new_entry)
                diag_patched = True
                print(f"[T655 BUILDER] Replaced t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)")

            else:
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

            if name_str == "TRAILER!!!":
                break

    if not netd_rc_patched:
        raise RuntimeError("Failed to patch netd.rc in system.img!")
    if not diag_patched:
        raise RuntimeError("Failed to patch t636-diag.sh!")

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha = sha256_file(OUTPUT_IMAGE)
    output_bytes = os.path.getsize(OUTPUT_IMAGE)
    output_inode = OUTPUT_IMAGE.stat().st_ino
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T655",
        "description": "Fix NetworkManagementService netd Watchdog timeout: restore /system/bin/netd in netd.rc (offset 0x17c9c000) and mount cgroup2 on /sys/fs/cgroup in t636-diag.sh",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "base_image_bytes": BASE_IMAGE.stat().st_size,
        "kernel_image": str(kernel_path),
        "kernel_image_sha256": kernel_sha,
        "fix_image": str(OUTPUT_IMAGE),
        "fix_image_sha256": output_sha,
        "fix_image_bytes": output_bytes,
        "fix_image_inode": output_inode,
        "elapsed_seconds": round(elapsed, 2),
    }

    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")

    print(f"[T655 BUILDER] Successfully built {OUTPUT_IMAGE} in {elapsed:.1f}s")
    print(f"[T655 BUILDER] SHA-256: {output_sha} ({output_bytes} bytes, inode {output_inode})")


if __name__ == "__main__":
    main()
