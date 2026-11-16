#!/usr/bin/env python3
"""Hash the ramdisk-images/vendor.img member of a gzipped newc CPIO.
Usage: cpio_member_hash.py <cpio.gz> <expected-sha256>
Exit 0 + PASS line on match, exit 1 otherwise."""
import gzip
import hashlib
import sys

path, expected = sys.argv[1], sys.argv[2]
data = gzip.decompress(open(path, "rb").read())
off = 0
found = None
while off + 110 <= len(data):
    assert data[off:off + 6] == b"070701", f"bad newc magic at {off}"
    fields = data[off + 6:off + 110].decode("ascii")
    filesize = int(fields[48:56], 16)
    namesize = int(fields[88:96], 16)
    name = data[off + 110:off + 110 + namesize - 1].decode()
    doff = off + 110 + namesize
    doff += (-doff) % 4
    if name == "ramdisk-images/vendor.img":
        found = hashlib.sha256(data[doff:doff + filesize]).hexdigest()
        print(f"member ramdisk-images/vendor.img: size={filesize} sha256={found}")
    if name == "TRAILER!!!":
        break
    end = doff + filesize
    end += (-end) % 4
    off = end
assert found is not None, "vendor.img member not found"
assert found == expected, f"EMBEDDED-VENDOR MISMATCH: {found} != {expected}"
print("EMBEDDED-VENDOR CHECK PASS")
