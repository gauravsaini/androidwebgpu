#!/usr/bin/env python3
import struct

IMG = "/Users/Shared/guest-inputs/system.img"

def read_inode_bytes(f, inode_num, s_inodes_per_group=464, s_inode_size=256):
    group = (inode_num - 1) // s_inodes_per_group
    index = (inode_num - 1) % s_inodes_per_group
    f.seek(4096 + group * 32, 0)
    bgd = f.read(32)
    inode_table_block = struct.unpack("<I", bgd[8:12])[0]
    ino_off = inode_table_block * 4096 + index * s_inode_size
    f.seek(ino_off, 0)
    ino_bytes = f.read(s_inode_size)
    mode, uid, size = struct.unpack("<HHI", ino_bytes[0:8])
    size_high = struct.unpack("<I", ino_bytes[108:112])[0]
    full_size = (size_high << 32) | size
    i_block = ino_bytes[40:100]
    
    # Check if fast symlink (i_blocks == 0 and size < 60)
    i_blocks = struct.unpack("<I", ino_bytes[28:32])[0]
    if (mode & 0o170000) == 0o120000 and i_blocks == 0:
        return mode, full_size, i_block[:full_size]
    
    magic, entries, max_entries, depth, gen = struct.unpack("<HHHHI", i_block[:12])
    data = bytearray()
    if depth == 0:
        for i in range(entries):
            block, elen, start_hi, start_lo = struct.unpack("<IHHI", i_block[12+i*12:12+(i+1)*12])
            start = (start_hi << 32) | start_lo
            f.seek(start * 4096, 0)
            data.extend(f.read(elen * 4096))
    elif depth == 1:
        for i in range(entries):
            ei_block, ei_leaf_lo, ei_leaf_hi, unused = struct.unpack("<IIHH", i_block[12+i*12:12+(i+1)*12])
            leaf_block = ((ei_leaf_hi << 32) | ei_leaf_lo)
            f.seek(leaf_block * 4096, 0)
            leaf_bytes = f.read(4096)
            lmagic, lentries, lmax, ldepth, lgen = struct.unpack("<HHHHI", leaf_bytes[:12])
            assert ldepth == 0, f"Expected leaf depth 0, got {ldepth}"
            for le in range(lentries):
                block, elen, start_hi, start_lo = struct.unpack("<IHHI", leaf_bytes[12+le*12:12+(le+1)*12])
                start = (start_hi << 32) | start_lo
                f.seek(start * 4096, 0)
                data.extend(f.read(elen * 4096))
    return mode, full_size, bytes(data[:full_size])

def parse_dir(ddata):
    entries = []
    off = 0
    while off < len(ddata):
        if off + 8 > len(ddata): break
        ino, rec_len, name_len, file_type = struct.unpack("<IHBB", ddata[off:off+8])
        if ino == 0 or rec_len == 0: break
        dname = ddata[off+8:off+8+name_len].decode('latin1', 'replace')
        entries.append((ino, file_type, dname))
        off += rec_len
    return entries

with open(IMG, "rb") as f:
    # Read inode 226 ('system/bin')
    bmode, bsize, bdata = read_inode_bytes(f, 226)
    bin_entries = parse_dir(bdata)
    print(f"Total entries in system/bin: {len(bin_entries)}")
    for ino, ftype, name in sorted(bin_entries, key=lambda x: x[2]):
        if name in ("netd", "sh", "toybox", "toolbox", "linker64", "servicemanager", "hwservicemanager"):
            mmode, msize, mdata = read_inode_bytes(f, ino)
            is_elf = mdata[:4] == b"\x7fELF"
            print(f"  {name:20s} ino={ino:6d} ftype={ftype} mode={mmode:#o} size={msize} is_elf={is_elf}")
