#!/usr/bin/env python3
from find_bin_netd import IMG, read_inode_bytes, parse_dir

with open(IMG, "rb") as f:
    emode, esize, edata = read_inode_bytes(f, 631) # system/etc
    for ino, ftype, name in parse_dir(edata):
        if "cgroup" in name:
            mode, size, data = read_inode_bytes(f, ino)
            print(f"{name}: ino={ino}, size={size}")
            print(data.decode('latin1', 'replace')[:500])
