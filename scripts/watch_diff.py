#!/usr/bin/env python3
import sys
import os
import time
from aligned_diff_50k import parse_line

def check_progress(qemu_path, rust_path, offset=6):
    qemu_steps = {}
    with open(qemu_path, "r") as f:
        for line in f:
            p = parse_line(line)
            if p and "step" in p:
                qemu_steps[int(p["step"])] = p

    if not os.path.exists(rust_path):
        print(f"Rust log {rust_path} does not exist yet.")
        return

    rust_steps = {}
    with open(rust_path, "r") as f:
        for line in f:
            p = parse_line(line)
            if p and "step" in p:
                rust_steps[int(p["step"])] = p

    common_qemu_steps = sorted([s for s in qemu_steps if (s - offset) in rust_steps])
    print(f"Checking {len(common_qemu_steps)} available aligned checkpoints (QEMU {min(common_qemu_steps) if common_qemu_steps else 'none'}..{max(common_qemu_steps) if common_qemu_steps else 'none'})...")

    checked_regs = [f"x{i}" for i in range(31)] + ["sp", "cpsr", "sctlr_el1", "ttbr0_el1", "tcr_el1", "ttbr1_el1", "mair_el1"]
    
    diff_found = False
    for q_step in common_qemu_steps:
        r_step = q_step - offset
        qs = qemu_steps[q_step]
        rs = rust_steps[r_step]
        mismatches = []
        if qs.get("insn_pc") != rs.get("insn_pc"):
            mismatches.append(f"insn_pc: QEMU={qs.get('insn_pc')} vs Rust={rs.get('insn_pc')}")
        if qs.get("word") != rs.get("word"):
            mismatches.append(f"word: QEMU={qs.get('word')} vs Rust={rs.get('word')}")
        for reg in checked_regs:
            qv = qs.get(reg)
            rv = rs.get(reg)
            if qv is not None and rv is not None:
                try:
                    if int(qv, 16) != int(rv, 16):
                        mismatches.append(f"{reg}: QEMU={qv} vs Rust={rv}")
                except ValueError:
                    if qv != rv:
                        mismatches.append(f"{reg}: QEMU={qv} vs Rust={rv}")
        if mismatches:
            print(f"*** DIVERGENCE at QEMU {q_step} / Rust {r_step} ***")
            for m in mismatches:
                print(f"  {m}")
            diff_found = True
            break
        else:
            print(f"  [MATCH 100%] QEMU step {q_step} == Rust step {r_step} (PC={rs.get('insn_pc')})")

    if not diff_found and common_qemu_steps:
        print(f"ALL {len(common_qemu_steps)} CHECKED STEPS MATCH 100% BIT-FOR-BIT.")

if __name__ == "__main__":
    q_path = sys.argv[1] if len(sys.argv) > 1 else "/Users/Shared/guest-inputs/t558-qemu-pinned-instruction-trace.log"
    r_path = sys.argv[2] if len(sys.argv) > 2 else "/Users/Shared/agyx-logs/track575-rust-25m-periodic.log"
    offset = int(sys.argv[3]) if len(sys.argv) > 3 else 6
    check_progress(q_path, r_path, offset)
