#!/usr/bin/env python3
"""Capture a per-instruction QEMU trace with -singlestep for the oracle.

QEMU `-d cpu` without -singlestep emits one register dump per translation
block (TB), which is incommensurable with the emulator's per-instruction
trace. This script captures with `-singlestep -d cpu,nochain` so each record
is exactly one instruction, making trace_diff.py's lockstep diff valid.

Usage (on the box):
    python3 scripts/qemu_singlestep_capture.py \
        --qemu-bin /home/muse/qemu-root/usr/bin/qemu-system-aarch64 \
        --kernel /mnt/sdb1/aosp/Image \
        --dtb guest-image/minimal-virt.dtb \
        --out /mnt/sdb1/gb-loop/qemu-singlestep.log \
        --max-steps 50000

Then validate + diff:
    python3 scripts/trace_diff.py \
        --qemu /mnt/sdb1/gb-loop/qemu-singlestep.log \
        --pathn /tmp/emu-new.log \
        --validate-singlestep \
        --keep-pc-range 0x40080000:0x41000000
"""

import argparse
import os
import subprocess
import sys


def main():
    p = argparse.ArgumentParser(description="Capture per-instruction QEMU trace")
    p.add_argument("--qemu-bin", required=True, help="qemu-system-aarch64 binary")
    p.add_argument("--kernel", required=True, help="Kernel Image")
    p.add_argument("--dtb", required=True, help="Device tree blob")
    p.add_argument("--out", required=True, help="Output log path")
    p.add_argument("--max-steps", type=int, default=50000,
                   help="Instructions to capture (QEMU has no step limit; "
                        "we use timeout as a rough bound — see note)")
    p.add_argument("--timeout", type=int, default=600,
                   help="Kill QEMU after N seconds (default 600)")
    args = p.parse_args()

    # NOTE: QEMU -d cpu has no instruction-count limit. We bound by wall time.
    # Singlestep is ~100-1000x slower than normal TCG; 50K instr ~= 1-5 min.
    # Tune --timeout based on observed throughput; capture once, reuse.
    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = "/home/muse/qemu-root/usr/lib/x86_64-linux-gnu"
    env["QEMU_LD_PREFIX"] = "/home/muse/qemu-root/usr/share/qemu"

    cmd = [
        args.qemu_bin,
        "-cpu", "cortex-a53",
        "-machine", "virt",
        "-m", "1024",
        "-kernel", args.kernel,
        "-dtb", args.dtb,
        "-nographic",
        "-singlestep",
        "-d", "cpu,nochain",
        "-D", args.out,
    ]
    print(f"[capture] {' '.join(cmd)}")
    print(f"[capture] timeout={args.timeout}s -> {args.out}")
    try:
        subprocess.run(cmd, env=env, timeout=args.timeout,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        pass  # Expected: we kill QEMU via timeout.

    if not os.path.exists(args.out):
        print("[capture] ERROR: no output file produced")
        sys.exit(1)
    size = os.path.getsize(args.out)
    print(f"[capture] done: {args.out} ({size} bytes)")
    # Quick sanity: count PC= records.
    n = 0
    with open(args.out, errors="ignore") as f:
        for line in f:
            if line.startswith("PC="):
                n += 1
                if n >= args.max_steps:
                    break
    print(f"[capture] {n} instruction records (target ~{args.max_steps})")


if __name__ == "__main__":
    main()
