#!/usr/bin/env python3
import sys

def parse_line(line):
    parts = line.strip().split()
    if not parts or (parts[0] != "I" and parts[0] != "[sample]"):
        return None
    d = {}
    for p in parts[1:]:
        if "=" in p:
            k, v = p.split("=", 1)
            d[k] = v.lower()
    return d

def main():
    qemu_path = sys.argv[1]
    rust_path = sys.argv[2]
    offset = int(sys.argv[3]) if len(sys.argv) > 3 else 6 # QEMU_step - Rust_step

    qemu_steps = {}
    with open(qemu_path, "r") as f:
        for line in f:
            parsed = parse_line(line)
            if parsed and "step" in parsed:
                step = int(parsed["step"])
                qemu_steps[step] = parsed

    rust_steps = {}
    with open(rust_path, "r") as f:
        for line in f:
            parsed = parse_line(line)
            if parsed and "step" in parsed:
                step = int(parsed["step"])
                rust_steps[step] = parsed

    print(f"Loaded {len(qemu_steps)} QEMU steps, {len(rust_steps)} Rust steps. Offset={offset}")

    common_qemu_steps = sorted([s for s in qemu_steps if (s - offset) in rust_steps])
    print(f"Comparing {len(common_qemu_steps)} aligned steps...")

    diffs = 0
    first_diff = None

    # Compare GPRs x0..x30, SP, CPSR, and sysregs.
    # Note: QEMU's TB-internal "pc" register is TB-base; insn_pc is the true instruction address.
    checked_regs = [f"x{i}" for i in range(31)] + ["sp", "cpsr", "sctlr_el1", "ttbr0_el1", "tcr_el1", "ttbr1_el1", "mair_el1"]

    for q_step in common_qemu_steps:
        r_step = q_step - offset
        q_state = qemu_steps[q_step]
        r_state = rust_steps[r_step]

        mismatches = []
        # Check PC and word
        if q_state.get("insn_pc") != r_state.get("insn_pc"):
            mismatches.append(f"insn_pc: QEMU={q_state.get('insn_pc')} vs Rust={r_state.get('insn_pc')}")
        if q_state.get("word") != r_state.get("word"):
            mismatches.append(f"word: QEMU={q_state.get('word')} vs Rust={r_state.get('word')}")

        for reg in checked_regs:
            qv = q_state.get(reg)
            rv = r_state.get(reg)
            if qv is not None and rv is not None:
                try:
                    q_int = int(qv, 16)
                    r_int = int(rv, 16)
                    if q_int != r_int:
                        mismatches.append(f"{reg}: QEMU={qv} vs Rust={rv}")
                except ValueError:
                    if qv != rv:
                        mismatches.append(f"{reg}: QEMU={qv} vs Rust={rv}")

        if mismatches:
            diffs += 1
            if first_diff is None:
                first_diff = (q_step, r_step, q_state, r_state, mismatches)
                print(f"\n*** FIRST DIVERGENCE at QEMU step {q_step} / Rust step {r_step} ***")
                print(f"QEMU: insn_pc={q_state.get('insn_pc')} word={q_state.get('word')}")
                print(f"Rust: insn_pc={r_state.get('insn_pc')} word={r_state.get('word')}")
                print("Mismatches:")
                for m in mismatches:
                    print(f"  {m}")
                # print previous step state for context
                prev_q = q_step - 1
                prev_r = r_step - 1
                if prev_q in qemu_steps and prev_r in rust_steps:
                    pq = qemu_steps[prev_q]
                    pr = rust_steps[prev_r]
                    print(f"\nPrevious step (QEMU {prev_q} / Rust {prev_r}):")
                    print(f"QEMU: insn_pc={pq.get('insn_pc')} word={pq.get('word')}")
                    print(f"Rust: insn_pc={pr.get('insn_pc')} word={pr.get('word')}")
                    # show any differences in previous step
                    for reg in checked_regs:
                        if pq.get(reg) != pr.get(reg):
                            print(f"  prev {reg}: QEMU={pq.get(reg)} vs Rust={pr.get(reg)}")

            if diffs <= 10:
                print(f"\nStep QEMU {q_step} / Rust {r_step}: {len(mismatches)} mismatches: {mismatches[:4]}")
            elif diffs == 11:
                print("... further diffs truncated ...")

    if diffs == 0:
        print(f"ALL {len(common_qemu_steps)} ALIGNED STEPS MATCH 100% BIT-FOR-BIT!")
    else:
        print(f"\nTotal differing steps: {diffs} out of {len(common_qemu_steps)}")

if __name__ == "__main__":
    main()
