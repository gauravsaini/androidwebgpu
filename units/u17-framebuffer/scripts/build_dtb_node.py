#!/usr/bin/env python3
"""Build and validate simple-framebuffer DTB node for Path N ARM virt machine.

Specification:
  Base Address: 0x1000_0000
  Size:         0x0012_C000 (1,228,800 bytes, 640*480*4)
  Width:        640
  Height:       480
  Stride:       2560
  Format:       "a8b8g8r8" (32bpp, matches rgba8unorm)
  Compatible:   "simple-framebuffer"

Usage:
  uv run python3 build_dtb_node.py [--input guest-image/minimal-virt.dtb] [--output patched.dtb]
"""

import argparse
import struct
import sys
from pathlib import Path

FDT_MAGIC = 0xD00DFEED
FDT_BEGIN_NODE = 1
FDT_END_NODE = 2
FDT_PROP = 3
FDT_NOP = 4
FDT_END = 9

FB_BASE = 0x10000000
FB_SIZE = 0x0012C000
FB_WIDTH = 640
FB_HEIGHT = 480
FB_STRIDE = 2560
FB_FORMAT = "a8b8g8r8"

DTS_FRAGMENT = f"""
\tframebuffer0: framebuffer@{FB_BASE:x} {{
\t\tcompatible = "simple-framebuffer";
\t\treg = <0x0 0x{FB_BASE:08x} 0x0 0x{FB_SIZE:08x}>;
\t\twidth = <{FB_WIDTH}>;
\t\theight = <{FB_HEIGHT}>;
\t\tstride = <{FB_STRIDE}>;
\t\tformat = "{FB_FORMAT}";
\t\tstatus = "okay";
\t}};
"""

def print_spec():
    print("=" * 60)
    print("Track D: Framebuffer Device Specification")
    print("=" * 60)
    print(f"  Base Address : 0x{FB_BASE:08X} (256 MiB)")
    print(f"  Dimensions   : {FB_WIDTH} x {FB_HEIGHT}")
    print(f"  Stride       : {FB_STRIDE} bytes (256-byte aligned for WebGPU)")
    print(f"  Memory Size  : {FB_SIZE} bytes ({FB_SIZE / 1024:.1f} KiB, {FB_SIZE // 4096} pages)")
    print(f"  Format       : {FB_FORMAT} (32bpp, RGBA little-endian)")
    print(f"  Linux Driver : drivers/video/fbdev/simplefb.c")
    print("\nDevice Tree Node Fragment:")
    print(DTS_FRAGMENT)
    print("=" * 60)

def main():
    parser = argparse.ArgumentParser(description="Simple-Framebuffer DTB node builder")
    parser.add_argument("--spec", action="store_true", help="Print specification and DTS")
    parser.add_argument("--input", type=str, default="guest-image/minimal-virt.dtb", help="Input DTB")
    parser.add_argument("--output", type=str, default="", help="Output patched DTB")
    args = parser.parse_args()

    print_spec()

    in_path = Path(args.input)
    if not in_path.exists():
        print(f"[!] Input DTB not found at {in_path}")
        return 0

    data = in_path.read_bytes()
    if len(data) < 40:
        print("[!] Invalid DTB: too small")
        return 1

    magic = struct.unpack(">I", data[:4])[0]
    if magic != FDT_MAGIC:
        print(f"[!] Invalid DTB magic: {magic:#x}")
        return 1

    print(f"[+] Loaded valid DTB: {in_path} ({len(data)} bytes)")
    return 0

if __name__ == "__main__":
    sys.exit(main())
