#!/usr/bin/env python3
"""Apply a T727PATCH1 block-delta patch to a cloned base image (Mac side).

Usage: apply_patch_t727.py <patch> <target-image>
Reads magic line + records [offset:u64][length:u64][data]; writes each chunk
in place. No other file is touched.
"""
import struct
import sys

PATCH, TARGET = sys.argv[1], sys.argv[2]
n = 0
with open(PATCH, "rb") as fp:
    magic = fp.readline()
    assert magic == b"T727PATCH1\n", magic
    with open(TARGET, "r+b") as ft:
        while True:
            hdr = fp.read(16)
            if not hdr:
                break
            off, ln = struct.unpack(">QQ", hdr)
            data = fp.read(ln)
            assert len(data) == ln, f"short chunk at {off}"
            ft.seek(off)
            ft.write(data)
            n += 1
print(f"applied chunks={n}")
