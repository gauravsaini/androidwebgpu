#!/usr/bin/env python3
import gzip
import struct

CPIO_PATH = "guest-inputs/t654-base-image.cpio.gz"

def read_inode_data(fin, inode_num, s_inodes_per_group=464, s_inode_size=256):
    group = (inode_num - 1) // s_inodes_per_group
    index = (inode_num - 1) % s_inodes_per_group
    # Each group desc is 64 bytes (or 32 bytes). Earlier s_desc_size=0 means 32 bytes (standard ext4 32-bit)
    # Let's read bg_inode_table from group desc
    # Seek to group desc at 4096 + group * 32
    fin.seek(4096 + group * 32, 0)
    bgd = fin.read(32)
    inode_table_block = struct.unpack("<I", bgd[8:12])[0]
    # Inode offset
    ino_off = inode_table_block * 4096 + index * s_inode_size
    fin.seek(ino_off, 0)
    ino_bytes = fin.read(s_inode_size)
    mode, uid, size = struct.unpack("<HHI", ino_bytes[0:8])
    size_high = struct.unpack("<I", ino_bytes[108:112])[0]
    full_size = (size_high << 32) | size
    i_block = ino_bytes[40:100]
    
    # Read extents
    magic, entries, max_entries, depth, gen = struct.unpack("<HHHHI", i_block[:12])
    data = bytearray()
    if depth == 0:
        for i in range(entries):
            block, elen, start_hi, start_lo = struct.unpack("<IHHI", i_block[12+i*12:12+(i+1)*12])
            start = (start_hi << 32) | start_lo
            fin.seek(start * 4096, 0)
            data.extend(fin.read(elen * 4096))
    return mode, full_size, bytes(data[:full_size])

# Let's extract system.img into a temp buffer or read directly if we seek
# Wait, gzip doesn't support random seek across gigabytes quickly unless decompressed or read in chunks
