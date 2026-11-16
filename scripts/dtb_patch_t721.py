#!/usr/bin/env python3
"""T720 DTB re-align (T699/T716/T717 procedure; no dtc/fdtget on the box).

Minimal FDT parser: walks the struct block to find the /chosen node's
linux,initrd-start and linux,initrd-end properties, asserts the old end
value equals start + old CPIO size, writes the new end (start + new CPIO
size) as a big-endian u64, and proves byte-diff confinement (only bytes
inside the 8-byte end value differ; total size unchanged).

Usage: dtb_patch_t720.py <base.dtb> <old-gz-size> <new-gz-size> <out.dtb>
"""
import hashlib
import struct
import sys

FDT_BEGIN_NODE = 1
FDT_END_NODE = 2
FDT_PROP = 3
FDT_NOP = 4
FDT_END = 9


def u32(data: bytes, off: int) -> int:
    return struct.unpack(">I", data[off:off + 4])[0]


def u64(data: bytes, off: int) -> int:
    return struct.unpack(">Q", data[off:off + 8])[0]


def main() -> None:
    base_dtb, old_size_s, new_size_s, out_dtb = sys.argv[1:5]
    old_size = int(old_size_s)
    new_size = int(new_size_s)
    data = open(base_dtb, "rb").read()
    assert u32(data, 0) == 0xD00DFEED, "bad FDT magic"
    totalsize = u32(data, 4)
    assert totalsize == len(data), f"FDT totalsize {totalsize} != file size {len(data)}"
    off_struct = u32(data, 8)
    off_strings = u32(data, 12)

    # Walk the struct block tracking the node path.
    path = []
    off = off_struct
    props = {}  # (node path, prop name) -> (value_off, length)
    while True:
        token = u32(data, off)
        off += 4
        if token == FDT_BEGIN_NODE:
            end = data.index(b"\x00", off)
            name = data[off:end].decode("ascii")
            off = end + 1
            off += (-off) % 4
            path.append(name)
        elif token == FDT_END_NODE:
            path.pop()
        elif token == FDT_PROP:
            length = u32(data, off)
            nameoff = u32(data, off + 4)
            off += 8
            end = data.index(b"\x00", off_strings + nameoff)
            pname = data[off_strings + nameoff:end].decode("ascii")
            props[("/".join(path), pname)] = (off, length)
            off += length
            off += (-off) % 4
        elif token == FDT_NOP:
            pass
        elif token == FDT_END:
            break
        else:
            raise AssertionError(f"unknown FDT token {token} at offset {off - 4}")

    start_key = ("/chosen", "linux,initrd-start")
    end_key = ("/chosen", "linux,initrd-end")
    assert start_key in props, "linux,initrd-start not found under /chosen"
    assert end_key in props, "linux,initrd-end not found under /chosen"
    start_off, start_len = props[start_key]
    end_off, end_len = props[end_key]
    assert start_len == 8 and end_len == 8, f"initrd props must be u64 (got {start_len}/{end_len})"
    start_val = u64(data, start_off)
    old_end_val = u64(data, end_off)
    print(f"initrd-start = {start_val:#x} (value offset {start_off})")
    print(f"initrd-end   = {old_end_val:#x} (value offset {end_off})")
    assert old_end_val == start_val + old_size, (
        f"old end {old_end_val:#x} != start + old size {start_val + old_size:#x}")
    new_end_val = start_val + new_size
    print(f"new initrd-end = {new_end_val:#x} (start + {new_size})")

    out = bytearray(data)
    out[end_off:end_off + 8] = struct.pack(">Q", new_end_val)
    diffs = [i for i in range(len(data)) if out[i] != data[i]]
    for i in diffs:
        assert end_off <= i < end_off + 8, f"diff outside initrd-end value at offset {i}"
    print(f"diff bytes: {len(diffs)} (all inside the 8-byte initrd-end value)")
    open(out_dtb, "wb").write(bytes(out))
    print(f"wrote {out_dtb} ({len(out)} bytes) sha256={hashlib.sha256(bytes(out)).hexdigest()}")


if __name__ == "__main__":
    main()
