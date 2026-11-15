#!/usr/bin/env python3
"""Step 1 / pre-flight input verification for Track T716.

Verifies every staged guest input by SHA-256, proves the staged DTB's
linux,initrd-end equals initrd-start + the staged CPIO size byte-exactly,
and proves the staged CPIO's embedded ramdisk-images/vendor.img member
hashes to the patched vendor.img SHA (streaming parse — the 725 MB
uncompressed stream is never fully materialised).

The system.img SHA equality to the T715 certified value IS the installd
trigger verification: system.img is byte-identical to the certified
T715 image, so installd.rc is SHA 3ea3466b by construction.
"""
import gzip
import hashlib
import struct
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
GI = WORKTREE / "guest-inputs"

EXPECTED = {
    "t705-Image": ("53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859", 20126208),
    "canonical-Image": ("05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04", 23073280),
    "t716-modified.dtb": ("cbb25094fffbb9f1bd010fde21868a8e2938a10d214a60051181ab123fa10e97", 2049),
    "t716-fix.cpio.gz": ("08eb665bb5860a5cd56ca92e0cfdddb0d4ee351f4265e6dcf755eb0bfb4414e3", 53380254),
    "t715-system.img": ("1283cbf279038f692ae42adc7bab571e89ef4ffd97efb0d982ee46e8050bfe3c", 2059816960),
    "t716-vendor.img": ("9a2519b17ff8145f835ec6320e3c1b8f11e711b0c3ac70c868fef0c91e22a6af", 134217728),
}
VENDOR_MEMBER = "ramdisk-images/vendor.img"
INITRD_START = 0x110000000


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def dtb_chosen(data: bytes):
    assert data[0:4] == b"\xd0\x0d\xfe\xed", "bad FDT magic"
    (totalsize, off_struct, off_strings) = struct.unpack(">3I", data[4:16])
    strings = data[off_strings:totalsize]
    structs = data[off_struct:totalsize]
    pos, path, out = 0, [], {}
    while pos + 4 <= len(structs):
        (tag,) = struct.unpack(">I", structs[pos:pos + 4])
        pos += 4
        if tag == 1:  # BEGIN_NODE
            end = structs.index(b"\x00", pos)
            path.append(structs[pos:end].decode())
            pos = (end + 1 + 3) & ~3
        elif tag == 2:  # END_NODE
            path.pop()
        elif tag == 3:  # PROP
            (plen, nameoff) = struct.unpack(">2I", structs[pos:pos + 8])
            pos += 8
            value = bytes(structs[pos:pos + plen])
            pos = (pos + plen + 3) & ~3
            name_end = strings.index(b"\x00", nameoff)
            name = strings[nameoff:name_end].decode()
            if "/".join(path) in ("chosen", "/chosen"):
                out[name] = value
        elif tag == 4:  # NOP
            pass
        elif tag == 9:  # END
            break
        else:
            raise ValueError(f"unknown FDT tag {tag}")
    return out


def cpio_vendor_member_sha(gz_path: Path) -> tuple[str, int, int]:
    """Stream the gzipped newc archive; return (vendor member SHA, size, member count)."""
    member_sha, member_size, count = None, None, 0
    with gzip.open(gz_path, "rb") as f:
        while True:
            header = f.read(110)
            if len(header) < 110:
                break
            assert header[0:6] == b"070701", f"bad newc magic at member {count}"
            namesize = int(header[94:102], 16)
            filesize = int(header[54:62], 16)
            name = f.read(namesize)[:-1].decode()
            # pad to 4 after name (header 110 + namesize)
            pad = (4 - ((110 + namesize) % 4)) % 4
            if pad:
                f.read(pad)
            count += 1
            if name == VENDOR_MEMBER:
                h = hashlib.sha256()
                remaining = filesize
                while remaining > 0:
                    chunk = f.read(min(8 * 1024 * 1024, remaining))
                    if not chunk:
                        raise ValueError("short read in vendor member")
                    h.update(chunk)
                    remaining -= len(chunk)
                member_sha, member_size = h.hexdigest(), filesize
            else:
                # skip data in chunks (stream may not be seekable)
                remaining = filesize
                while remaining > 0:
                    chunk = f.read(min(8 * 1024 * 1024, remaining))
                    if not chunk:
                        raise ValueError(f"short read in member {name}")
                    remaining -= len(chunk)
            pad = (4 - (filesize % 4)) % 4
            if pad:
                f.read(pad)
            if name == "TRAILER!!!":
                break
    assert member_sha is not None, f"{VENDOR_MEMBER} member not found"
    return member_sha, member_size, count


def main() -> None:
    print("=== T716 INPUT VERIFICATION ===")
    for name, (sha, size) in EXPECTED.items():
        p = GI / name
        assert p.exists(), f"MISSING: {p}"
        got_sha = sha256_file(p)
        got_size = p.stat().st_size
        assert got_sha == sha, f"SHA MISMATCH {name}: {got_sha}"
        assert got_size == size, f"SIZE MISMATCH {name}: {got_size}"
        print(f"PASS: {name} {got_size} B sha={got_sha[:16]}...")

    qemu = Path("/opt/homebrew/bin/qemu-system-aarch64")
    print(f"QEMU: {qemu} sha={sha256_file(qemu)[:16]}... bytes={qemu.stat().st_size} (record-only)")

    # DTB initrd-end vs staged CPIO size
    chosen = dtb_chosen((GI / "t716-modified.dtb").read_bytes())
    start = int.from_bytes(chosen["linux,initrd-start"], "big")
    end = int.from_bytes(chosen["linux,initrd-end"], "big")
    cpio_size = (GI / "t716-fix.cpio.gz").stat().st_size
    assert start == INITRD_START, f"initrd-start 0x{start:x} != 0x{INITRD_START:x}"
    assert end - start == cpio_size, f"initrd-end delta {end - start} != CPIO size {cpio_size}"
    print(f"PASS: DTB initrd-start=0x{start:x} initrd-end=0x{end:x} delta={end - start} == CPIO size")

    # Embedded vendor member
    member_sha, member_size, count = cpio_vendor_member_sha(GI / "t716-fix.cpio.gz")
    assert member_sha == EXPECTED["t716-vendor.img"][0], f"embedded vendor SHA {member_sha}"
    assert member_size == 134217728, f"embedded vendor size {member_size}"
    print(f"PASS: embedded {VENDOR_MEMBER} sha={member_sha[:16]}... size={member_size} ({count} members)")
    print("=== ALL T716 INPUTS VERIFIED ===")


if __name__ == "__main__":
    main()
