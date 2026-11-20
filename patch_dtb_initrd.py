#!/usr/bin/env python3
"""Patch a DTB's /chosen/linux,initrd-end to match a given initrd file size.
Usage: patch_dtb_initrd.py <in.dtb> <initrd-file> <out.dtb>
initrd-end = 0x110000000 + filesize(initrd). initrd-start is verified unchanged.
"""
import struct, sys, os, hashlib

START = 0x110000000

def find_initrd_end(d):
    magic, totalsize, off_struct, off_strings, off_mem, version, last_comp, boot_cpuid, size_strings, size_struct = struct.unpack(">10I", d[:40])
    assert magic == 0xd00dfeed, "bad DTB magic"
    pos = off_struct
    path = []
    FDT_BEGIN_NODE, FDT_END_NODE, FDT_PROP, FDT_NOP, FDT_END = 1, 2, 3, 4, 9
    while pos < off_struct + size_struct:
        tok = struct.unpack(">I", d[pos:pos+4])[0]
        pos += 4
        if tok == FDT_BEGIN_NODE:
            end = d.find(b"\x00", pos)
            name = d[pos:end].decode()
            pos += (len(name) + 1 + 3) & ~3
            path.append(name)
        elif tok == FDT_END_NODE:
            path.pop()
        elif tok == FDT_PROP:
            ln, nameoff = struct.unpack(">II", d[pos:pos+8])
            pos += 8
            val_off = pos
            val = d[pos:pos+ln]
            name_end = d.find(b"\x00", off_strings + nameoff)
            pname = d[off_strings+nameoff:name_end].decode()
            cur = "/".join(path)
            if cur == "/chosen" and pname in ("linux,initrd-start", "linux,initrd-end"):
                assert ln == 8, f"{pname} len {ln} != 8"
                yield pname, val_off, struct.unpack(">Q", val)[0]
            pos += (ln + 3) & ~3
        elif tok == FDT_NOP:
            pass
        elif tok == FDT_END:
            break
        else:
            raise ValueError(f"unknown token {tok} at {pos-4}")

def main():
    in_dtb, initrd, out_dtb = sys.argv[1], sys.argv[2], sys.argv[3]
    d = bytearray(open(in_dtb, "rb").read())
    orig = bytes(d)
    size = os.path.getsize(initrd)
    props = dict((p, (off, v)) for p, off, v in find_initrd_end(bytes(d)))
    start_off, start_val = props["linux,initrd-start"]
    end_off, end_val = props["linux,initrd-end"]
    assert start_val == START, f"initrd-start 0x{start_val:x} != 0x{START:x}"
    new_end = START + size
    print(f"initrd file: {initrd} ({size} B = 0x{size:x})")
    print(f"initrd-start: 0x{start_val:x} (unchanged)")
    print(f"initrd-end: 0x{end_val:x} -> 0x{new_end:x}")
    assert new_end != end_val or size == end_val - START
    d[end_off:end_off+8] = struct.pack(">Q", new_end)
    open(out_dtb, "wb").write(bytes(d))
    # verify
    d2 = open(out_dtb, "rb").read()
    props2 = dict((p, (off, v)) for p, off, v in find_initrd_end(d2))
    assert props2["linux,initrd-start"][1] == START
    assert props2["linux,initrd-end"][1] == new_end
    assert len(d2) == len(d), "DTB size changed"
    # every byte except the 8 patched bytes must be identical
    diff = [i for i in range(len(d)) if orig[i] != d2[i]]
    assert all(end_off <= i < end_off+8 for i in diff), f"unexpected diff bytes: {diff}"
    assert len(diff) >= 1, "no bytes changed (end already equal?)"
    print(f"wrote {out_dtb} ({len(d2)} B, sha {hashlib.sha256(d2).hexdigest()[:8]})")
    print(f"verify: start=0x{props2['linux,initrd-start'][1]:x} end=0x{props2['linux,initrd-end'][1]:x} (end-start={props2['linux,initrd-end'][1]-props2['linux,initrd-start'][1]} == filesize {size})")

if __name__ == "__main__":
    main()
