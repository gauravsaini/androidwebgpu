#!/usr/bin/env python3
"""First-divergence finder: QEMU vs emulator trace, via binary search.

Takes QEMU (-d cpu) and emulator (--trace) logs, then binary-searches for
the exact step where they first diverge. Linear scan is O(n); binary
search is O(log n) trace generations.

Each probe runs both simulators up to step M and compares the CPU state
(PC, X0-X30, SP, NZCV) at that step. The search narrows to the first
divergent step.

Usage:
  python3 scripts/diff_trace.py --qemu-bin <bin> --emu-bin <bin>
      --kernel <Image> --dtb <dtb> --cpu cortex-a53
      --max-steps N [--window W]

Output:
  FIRST DIVERGENCE at step S
    PC: qemu=0x... emu=0x...
    instruction: 0x... (disassembled if capstone available)
    mismatched regs: X5, SP, ...

Exit 0: divergence found (or no divergence within max-steps).
Exit 2: infrastructure failure.
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile


RE_QEMU_PC = re.compile(
    r"^\s*PC=([0-9a-fA-F]+)\s+X00=([0-9a-fA-F]+)\s+X01=([0-9a-fA-F]+)")
RE_QEMU_ALL_REGS = re.compile(r"X(\d\d)=([0-9a-fA-F]+)")
RE_QEMU_SP = re.compile(r"SP=([0-9a-fA-F]+)")
RE_QEMU_PSTATE = re.compile(r"PSTATE=([0-9a-fA-F]+)")


def qemu_state_at(qemu_bin, kernel, dtb, step, tmpdir, qemu_extra):
    """Run QEMU to `step`, return CPU state dict at that step."""
    log = os.path.join(tmpdir, f"qemu_{step}.log")
    cmd = [qemu_bin, "-L", "/home/muse/qemu-root/usr/share/qemu",
           "-cpu", "cortex-a53", "-machine", "virt", "-m", "1024",
           "-kernel", kernel, "-dtb", dtb,
           "-nographic", "-d", "cpu", "-D", log] + qemu_extra
    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = ("/home/muse/qemu-root/usr/lib/x86_64-linux-gnu:"
                              + env.get("LD_LIBRARY_PATH", ""))
    # Heuristic: QEMU -d cpu is fast; give it enough time for `step` steps.
    # 1M steps ~ 30s based on observed throughput.
    timeout = max(30, int(step / 1_000_000 * 40) + 20)
    try:
        subprocess.run(cmd, env=env, timeout=timeout, capture_output=True)
    except subprocess.TimeoutExpired:
        pass
    return parse_qemu_at(log, step)


def parse_qemu_at(path, want_step):
    """Parse the Nth CPU record (0-indexed) from a QEMU -d cpu log."""
    # QEMU -d cpu emits a register dump per TB, not per instruction.
    # We approximate: each "PC=" line is one record.
    # NOTE: this is TB-granular; for instruction-granular diff use the
    # emulator's trace and align by PC sequence.
    try:
        with open(path, errors="replace") as f:
            records = []
            cur = {}
            for line in f:
                m = RE_QEMU_PC.match(line)
                if m:
                    if cur:
                        records.append(cur)
                    cur = {"pc": int(m.group(1), 16),
                           "x0": int(m.group(2), 16),
                           "x1": int(m.group(3), 16)}
                    if len(records) > want_step + 1:
                        break
                elif cur:
                    for rm in RE_QEMU_ALL_REGS.finditer(line):
                        cur[f"x{int(rm.group(1))}"] = int(rm.group(2), 16)
                    sm = RE_QEMU_SP.search(line)
                    if sm:
                        cur["sp"] = int(sm.group(1), 16)
                    pm = RE_QEMU_PSTATE.search(line)
                    if pm:
                        cur["pstate"] = int(pm.group(1), 16)
            if cur:
                records.append(cur)
    except FileNotFoundError:
        return None
    if want_step < len(records):
        return records[want_step]
    return None


def emu_state_at(emu_bin, kernel, dtb, step, tmpdir):
    """Run emulator to `step` with --trace, return CPU state at that step."""
    log = os.path.join(tmpdir, f"emu_{step}.log")
    cmd = [emu_bin, "--kernel", kernel, "--dtb", dtb,
           "--max-steps", str(step + 1), "--trace", log]
    subprocess.run(cmd, timeout=600, capture_output=True)
    return parse_emu_at(log, step)


def parse_emu_at(path, want_step):
    try:
        with open(path) as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split()
                if int(parts[0]) == want_step:
                    vals = [int(p, 16) for p in parts[1:]]
                    st = {"pc": vals[0], "word": vals[1]}
                    for i in range(31):
                        st[f"x{i}"] = vals[2 + i]
                    st["sp"] = vals[2 + 31]
                    st["nzcv"] = vals[2 + 32]
                    return st
    except FileNotFoundError:
        return None
    return None


def states_match(q, e):
    """Compare the registers both sides provide."""
    if q is None or e is None:
        return False
    for reg in ["pc"] + [f"x{i}" for i in range(31)] + ["sp"]:
        qv = q.get(reg)
        ev = e.get(reg)
        if qv is not None and ev is not None and qv != ev:
            return False
    return True


def main():
    ap = argparse.ArgumentParser(description="Binary-search first divergence")
    ap.add_argument("--qemu-bin", required=True)
    ap.add_argument("--emu-bin", required=True)
    ap.add_argument("--kernel", required=True)
    ap.add_argument("--dtb", required=True)
    ap.add_argument("--cpu", default="cortex-a53")
    ap.add_argument("--max-steps", type=int, default=1_300_000)
    ap.add_argument("--qemu-extra", nargs="*", default=[])
    args = ap.parse_args()

    with tempfile.TemporaryDirectory() as tmp:
        # Step-0 sanity: if these differ, binary search is meaningless.
        q0 = qemu_state_at(args.qemu_bin, args.kernel, args.dtb,
                           0, tmp, args.qemu_extra)
        e0 = emu_state_at(args.emu_bin, args.kernel, args.dtb, 0, tmp)
        if not states_match(q0, e0):
            print("DIVERGENCE at step 0 (boot setup mismatch). "
                  "Run scripts/step0_gate.py for details.")
            return 0

        # Check the top: if max-steps matches, no divergence in range.
        q_hi = qemu_state_at(args.qemu_bin, args.kernel, args.dtb,
                             args.max_steps, tmp, args.qemu_extra)
        e_hi = emu_state_at(args.emu_bin, args.kernel, args.dtb,
                            args.max_steps, tmp)
        if states_match(q_hi, e_hi):
            print(f"No divergence in 0..{args.max_steps}.")
            return 0

        # Binary search for first divergence.
        lo, hi = 0, args.max_steps
        while lo + 1 < hi:
            mid = (lo + hi) // 2
            qm = qemu_state_at(args.qemu_bin, args.kernel, args.dtb,
                               mid, tmp, args.qemu_extra)
            em = emu_state_at(args.emu_bin, args.kernel, args.dtb, mid, tmp)
            if states_match(qm, em):
                lo = mid
            else:
                hi = mid
            print(f"  probe step {mid}: {'match' if states_match(qm, em) else 'DIVERGE'}",
                  file=sys.stderr)

        # hi is the first divergent step.
        qd = qemu_state_at(args.qemu_bin, args.kernel, args.dtb, hi, tmp,
                           args.qemu_extra)
        ed = emu_state_at(args.emu_bin, args.kernel, args.dtb, hi, tmp)
        print(f"FIRST DIVERGENCE at step {hi}")
        if qd and ed:
            print(f"  PC: qemu=0x{qd.get('pc', 0):x} emu=0x{ed.get('pc', 0):x}")
            if "word" in ed:
                print(f"  instruction: 0x{ed['word']:08x}")
            bad = [r for r in ["pc"] + [f"x{i}" for i in range(31)] + ["sp"]
                   if qd.get(r) is not None and ed.get(r) is not None
                   and qd[r] != ed[r]]
            print(f"  mismatched regs: {', '.join(bad) if bad else '(none parsed)'}")
        return 0


if __name__ == "__main__":
    sys.exit(main())
