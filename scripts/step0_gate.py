#!/usr/bin/env python3
"""Step-0 gate: QEMU vs emulator reset-state diff.

The cheapest, highest-value check. Dumps the CPU reset state (PC, X0-X3,
SP, PSTATE) from QEMU (via -d cpu, first record) and from our emulator
(via --trace, step 0), then diffs them.

If this fails, do NOT run anything else. Fix the boot setup first.
A mismatch here means: wrong kernel load address, wrong DTB address,
wrong initial registers, or wrong CPU state.

Usage:
  python3 scripts/step0_gate.py --qemu-bin <path> --kernel <Image> --dtb <dtb>
                                --emu-bin <run_kernel> [--qemu-extra ...]

Exit 0: reset states match.
Exit 1: mismatch (prints the diff).
Exit 2: infrastructure failure (QEMU or emulator didn't produce output).
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile


def run_qemu_step0(qemu_bin, kernel, dtb, qemu_extra, out_path):
    """Run QEMU for a handful of steps, capture -d cpu to out_path."""
    cmd = [
        qemu_bin, "-L", "/home/muse/qemu-root/usr/share/qemu",
        "-cpu", "cortex-a53", "-machine", "virt", "-m", "1024",
        "-kernel", kernel, "-dtb", dtb,
        "-nographic", "-d", "cpu", "-D", out_path,
    ] + qemu_extra
    # LD_LIBRARY_PATH for the qemu-root install
    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = "/home/muse/qemu-root/usr/lib/x86_64-linux-gnu:" + env.get("LD_LIBRARY_PATH", "")
    # Run briefly; QEMU with -d cpu writes continuously. Kill after 10s.
    try:
        subprocess.run(cmd, env=env, timeout=10, capture_output=True)
    except subprocess.TimeoutExpired:
        pass  # Expected: we kill it after capturing step 0


def run_emu_step0(emu_bin, kernel, dtb, out_path):
    """Run emulator for a few steps with --trace."""
    cmd = [emu_bin, "--kernel", kernel, "--dtb", dtb,
           "--max-steps", "10", "--trace", out_path]
    subprocess.run(cmd, timeout=120, capture_output=True)


RE_QEMU_PC = re.compile(r"^\s*PC=([0-9a-fA-F]+)\s+X00=([0-9a-fA-F]+)\s+X01=([0-9a-fA-F]+)\s+X02=([0-9a-fA-F]+)\s+X03=([0-9a-fA-F]+)")
RE_QEMU_SP = re.compile(r"SP=([0-9a-fA-F]+)")
RE_QEMU_PSTATE = re.compile(r"PSTATE=([0-9a-fA-F]+)")


def parse_qemu_step0(path):
    """Extract kernel-entry CPU record from QEMU -d cpu log.
    QEMU starts at reset stub (0x40000000); we want the kernel entry
    at 0x40080000, which is the 2nd record (step 1)."""
    with open(path, errors="replace") as f:
        content = f.read(131072)
    # Find all PC records, pick the one at 0x40080000 (kernel entry)
    for m in RE_QEMU_PC.finditer(content):
        pc = int(m.group(1), 16)
        if pc == 0x40080000:
            pc_v, x0, x1, x2, x3 = (int(v, 16) for v in m.groups())
            # Get X4 and full regs from the surrounding block
            start = max(0, m.start() - 2000)
            block = content[start:m.start() + 4000]
            x4_m = re.search(r"X04=([0-9a-fA-F]+)", block)
            x4 = int(x4_m.group(1), 16) if x4_m else 0
            sp_m = RE_QEMU_SP.search(block)
            ps_m = RE_QEMU_PSTATE.search(block)
            sp = int(sp_m.group(1), 16) if sp_m else 0
            pstate = int(ps_m.group(1), 16) if ps_m else 0
            # NZCV Z flag is bit 30 of PSTATE
            z_set = bool(pstate & 0x40000000)
            return {"pc": pc_v, "x0": x0, "x1": x1, "x2": x2, "x3": x3,
                    "x4": x4, "sp": sp, "pstate": pstate, "z": z_set}
    return None


def parse_emu_step0(path):
    """Extract step 0 from emulator --trace file.
    Format: step pc word x0..x30 sp nzcv (whitespace-separated hex).
    """
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if parts[0] == "0":  # step 0
                # step pc word x0..x30 sp nzcv
                vals = [int(p, 16) for p in parts[1:]]
                return {
                    "pc": vals[0],
                    "x0": vals[2], "x1": vals[3],
                    "x2": vals[4], "x3": vals[5],
                    "x4": vals[6],
                    "sp": vals[2 + 31],  # after x0..x30
                    "pstate": 0,  # nzcv in next field; pstate not directly available
                    "nzcv": vals[2 + 32],
                }
    return None


def main():
    ap = argparse.ArgumentParser(description="Step-0 gate: QEMU vs emulator reset state")
    ap.add_argument("--qemu-bin", required=True)
    ap.add_argument("--kernel", required=True)
    ap.add_argument("--dtb", required=True)
    ap.add_argument("--emu-bin", required=True)
    ap.add_argument("--qemu-extra", nargs="*", default=[])
    args = ap.parse_args()

    with tempfile.TemporaryDirectory() as tmp:
        qemu_log = os.path.join(tmp, "qemu.log")
        emu_log = os.path.join(tmp, "emu.log")

        run_qemu_step0(args.qemu_bin, args.kernel, args.dtb,
                       args.qemu_extra, qemu_log)
        run_emu_step0(args.emu_bin, args.kernel, args.dtb, emu_log)

        if not os.path.exists(qemu_log) or os.path.getsize(qemu_log) == 0:
            print("STEP0-GATE: FAIL (infra) - QEMU produced no output", file=sys.stderr)
            return 2
        if not os.path.exists(emu_log) or os.path.getsize(emu_log) == 0:
            print("STEP0-GATE: FAIL (infra) - emulator produced no output", file=sys.stderr)
            return 2

        q = parse_qemu_step0(qemu_log)
        e = parse_emu_step0(emu_log)

        if q is None:
            print("STEP0-GATE: FAIL (infra) - could not parse QEMU step 0", file=sys.stderr)
            return 2
        if e is None:
            print("STEP0-GATE: FAIL (infra) - could not parse emulator step 0", file=sys.stderr)
            return 2

        mismatches = []
        for reg in ["pc", "x0", "x1", "x2", "x3", "x4", "sp"]:
            qv = q.get(reg, 0)
            ev = e.get(reg, 0)
            if qv != ev:
                mismatches.append(
                    f"  {reg.upper():4s}: QEMU=0x{qv:016x}  EMU=0x{ev:016x}")
        # Z flag check (bit 30 of PSTATE)
        qz = q.get("z", False)
        ez = bool(e.get("pstate", 0) & 0x40000000) or (e.get("nzcv", 0) & 0x4)
        # Note: emulator trace has nzcv, QEMU has pstate; check Z via respective fields
        if qz != ez:
            mismatches.append(f"  ZFLG: QEMU={'set' if qz else 'clear'}  EMU={'set' if ez else 'clear'}")

        if mismatches:
            print("STEP0-GATE: FAIL - reset state mismatch")
            print("\n".join(mismatches))
            print("Fix the boot setup (load address, DTB address, initial regs) before proceeding.")
            return 1

        print(f"STEP0-GATE: PASS (pc=0x{q['pc']:x} x0=0x{q['x0']:x})")
        return 0


if __name__ == "__main__":
    sys.exit(main())
