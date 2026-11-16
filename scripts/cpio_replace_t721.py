#!/usr/bin/env python3
"""T721 CPIO member replacement (T699/T716/T717/T718/T720 procedure).

Replaces the `ramdisk-images/vendor.img` member of the T720 CPIO with the
T721 vendor image (patched composer + debugfs mount trigger). The guest
loop-mounts this embedded member as /vendor (early-ramdisk line 14); the
supervisor's vendor file is verified but never passed to QEMU, so THIS
member is the consumed vendor image. The replacement is the same size
(134217728 B); the archive is re-walked and member-by-member comparison
proves ONLY the vendor member differs, then re-gzipped deterministically
(gzip -9 -n). Unchanged members are emitted byte-verbatim.

Usage: cpio_replace_t721.py <in.cpio> <new-vendor.img> <out.cpio> <out.cpio.gz>
"""
import hashlib
import subprocess
import sys

MEMBER_VENDOR = "ramdisk-images/vendor.img"
EXPECTED_OLD_VENDOR_SHA = "bac3cdc112dc6c3257c016fb4f600c46f43b9372f3d2f2ebb9b7d79fd05daeca"
EXPECTED_NEW_VENDOR_SHA = "6bf3e2e0ebbc7dc06031ad64bfaae1acb7fe207e1453e297af6639bbb215148b"
EXPECTED_VENDOR_SIZE = 134217728


def parse_newc(data: bytes):
    """Walk a newc archive. Returns list of dicts with span info."""
    members = []
    off = 0
    n = len(data)
    while off + 110 <= n:
        magic = data[off:off + 6]
        assert magic == b"070701", f"bad newc magic at offset {off}: {magic!r}"
        fields = data[off + 6:off + 110].decode("ascii")
        filesize = int(fields[48:56], 16)
        namesize = int(fields[88:96], 16)
        name_off = off + 110
        name = data[name_off:name_off + namesize - 1].decode("utf-8", "surrogateescape")
        data_off = name_off + namesize
        data_off += (-data_off) % 4
        end = data_off + filesize
        end += (-end) % 4
        members.append({"name": name, "header_off": off, "data_off": data_off,
                        "size": filesize, "end": end, "namesize": namesize,
                        "fields": fields})
        if name == "TRAILER!!!":
            break
        off = end
    return members


def pad4(n: int) -> int:
    return (-n) % 4


def main() -> None:
    in_cpio, new_vendor, out_cpio, out_gz = sys.argv[1:5]
    data = open(in_cpio, "rb").read()
    print(f"input archive: {len(data)} bytes")
    members = parse_newc(data)
    real = [m for m in members if m["name"] != "TRAILER!!!"]
    print(f"members: {len(real)} real + trailer")
    assert members[-1]["name"] == "TRAILER!!!", "archive has no TRAILER!!! member"
    trailer_end = members[-1]["end"]
    trailing = data[trailer_end:]
    print(f"trailing bytes after trailer span: {len(trailing)}")

    hits = [m for m in members if m["name"] == MEMBER_VENDOR]
    assert len(hits) == 1, f"expected exactly 1 {MEMBER_VENDOR}, found {len(hits)}"
    vendor_m = hits[0]
    old_vendor = data[vendor_m["data_off"]:vendor_m["data_off"] + vendor_m["size"]]
    old_sha = hashlib.sha256(old_vendor).hexdigest()
    print(f"old {MEMBER_VENDOR}: size={vendor_m['size']} sha256={old_sha}")
    assert vendor_m["size"] == EXPECTED_VENDOR_SIZE, "old vendor size mismatch"
    assert old_sha == EXPECTED_OLD_VENDOR_SHA, "old vendor SHA mismatch (not the T718 image?)"

    new_bytes = open(new_vendor, "rb").read()
    new_sha = hashlib.sha256(new_bytes).hexdigest()
    print(f"new {MEMBER_VENDOR}: size={len(new_bytes)} sha256={new_sha}")
    assert len(new_bytes) == EXPECTED_VENDOR_SIZE, "new vendor size mismatch"
    assert new_sha == EXPECTED_NEW_VENDOR_SHA, "new vendor SHA mismatch"
    assert new_bytes != old_vendor, "new vendor identical to old"

    # Rebuild the stream: only the vendor member's DATA changes (same size,
    # so the header — including filesize — stays byte-identical).
    out = bytearray()
    for m in members:
        if m["name"] == MEMBER_VENDOR:
            out += data[m["header_off"]:m["data_off"]]
            out += new_bytes
            out += b"\x00" * pad4(len(out))
            # data_off..end span: header+name+pad already emitted; data+pad
            # must land exactly on m["end"] (same size => same span).
            assert len(out) == m["end"], f"vendor span misaligned: {len(out)} != {m['end']}"
        else:
            out += data[m["header_off"]:m["end"]]
    out += trailing
    print(f"output archive: {len(out)} bytes (delta {len(out) - len(data):+d})")
    assert len(out) == len(data), "archive size changed (vendor sizes must match)"

    # Re-walk + member-by-member comparison.
    new_members = parse_newc(bytes(out))
    assert [m["name"] for m in new_members] == [m["name"] for m in members], "member list changed"
    assert len(new_members) == len(members)
    for m_old in members:
        m_new = [m for m in new_members if m["name"] == m_old["name"]][0]
        old_seg = data[m_old["data_off"]:m_old["data_off"] + m_old["size"]]
        new_seg = bytes(out[m_new["data_off"]:m_new["data_off"] + m_new["size"]])
        oh = data[m_old["header_off"]:m_old["header_off"] + 110]
        nh = bytes(out[m_new["header_off"]:m_new["header_off"] + 110])
        assert oh == nh, f"header changed unexpectedly: {m_old['name']}"
        if m_old["name"] == MEMBER_VENDOR:
            assert hashlib.sha256(new_seg).hexdigest() == new_sha
        else:
            assert old_seg == new_seg, f"member changed unexpectedly: {m_old['name']}"
    print("member-by-member comparison PASS: ONLY ramdisk-images/vendor.img differs")

    open(out_cpio, "wb").write(bytes(out))
    print(f"wrote {out_cpio} ({len(out)} bytes)")
    with open(out_gz, "wb") as f:
        subprocess.run(["gzip", "-9", "-n", "-c", out_cpio], stdout=f, check=True)
    gz_bytes = open(out_gz, "rb").read()
    print(f"wrote {out_gz} ({len(gz_bytes)} bytes) sha256={hashlib.sha256(gz_bytes).hexdigest()}")


if __name__ == "__main__":
    main()
