#!/usr/bin/env python3
"""QEMU vs Path N trace normalizer and differ (P0-1 oracle).

Parses QEMU `-d cpu` register dumps and Path N emulator `--trace` files into
a canonical form (pc, x0..x30, sp, nzcv), aligns execution, and reports the
first divergence.
"""

import argparse
import re
import sys
from dataclasses import dataclass
from typing import List, Optional, Tuple


@dataclass
class CpuRecord:
    step: int
    pc: int
    word: Optional[int]
    regs: List[int]  # x0..x30 (31 elements)
    sp: int
    nzcv: int
    pstate: int


def parse_pathn_trace(path: str, max_records: Optional[int] = None) -> List[CpuRecord]:
    """Parse Path N per-step trace file.

    Format: step pc word x0..x30 sp nzcv
    """
    records = []
    with open(path, "r") as f:
        for line_num, line in enumerate(f):
            if max_records and len(records) >= max_records:
                break
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) < 35:
                continue
            step = int(parts[0])
            pc = int(parts[1], 16)
            word = int(parts[2], 16)
            regs = [int(p, 16) for p in parts[3:34]]
            sp = int(parts[34], 16)
            nzcv = int(parts[35], 16) if len(parts) > 35 else 0
            pstate = nzcv << 28
            records.append(
                CpuRecord(
                    step=step,
                    pc=pc,
                    word=word,
                    regs=regs,
                    sp=sp,
                    nzcv=nzcv,
                    pstate=pstate,
                )
            )
    return records


RE_QEMU_PC = re.compile(
    r"^\s*PC=([0-9a-fA-F]+)\s+X00=([0-9a-fA-F]+)\s+X01=([0-9a-fA-F]+)"
)
RE_QEMU_REGS = re.compile(r"X\d\d=([0-9a-fA-F]+)")
RE_QEMU_SP = re.compile(r"SP=([0-9a-fA-F]+)")
RE_QEMU_PSTATE = re.compile(r"PSTATE=([0-9a-fA-F]+)")


def parse_qemu_trace(path: str, max_records: Optional[int] = None) -> List[CpuRecord]:
    """Parse QEMU `-d cpu` register dumps.

    Each CPU dump occupies 12 consecutive lines.
    """
    records = []
    with open(path, "r") as f:
        lines = f.readlines()

    i = 0
    step = 0
    while i < len(lines):
        if max_records and len(records) >= max_records:
            break
        line = lines[i]
        m = RE_QEMU_PC.match(line)
        if m:
            pc = int(m.group(1), 16)
            regs = [int(m.group(2), 16), int(m.group(3), 16)]
            sp = 0
            pstate = 0

            # Next 10 lines have X02..X30 and SP
            for j in range(1, 11):
                if i + j >= len(lines):
                    break
                subline = lines[i + j]
                for rm in RE_QEMU_REGS.finditer(subline):
                    regs.append(int(rm.group(1), 16))
                sm = RE_QEMU_SP.search(subline)
                if sm:
                    sp = int(sm.group(1), 16)

            # 12th line has PSTATE
            if i + 11 < len(lines):
                pm = RE_QEMU_PSTATE.search(lines[i + 11])
                if pm:
                    pstate = int(pm.group(1), 16)

            nzcv = (pstate >> 28) & 0xF
            # Ensure regs has 31 entries
            while len(regs) < 31:
                regs.append(0)
            regs = regs[:31]

            records.append(
                CpuRecord(
                    step=step,
                    pc=pc,
                    word=None,
                    regs=regs,
                    sp=sp,
                    nzcv=nzcv,
                    pstate=pstate,
                )
            )
            step += 1
            i += 12
        else:
            i += 1

    return records


def format_nzcv(val: int) -> str:
    n = "N" if (val & 8) else "-"
    z = "Z" if (val & 4) else "-"
    c = "C" if (val & 2) else "-"
    v = "V" if (val & 1) else "-"
    return f"{n}{z}{c}{v} (0x{val:x})"


def diff_records(
    pathn_records: List[CpuRecord],
    qemu_records: List[CpuRecord],
    pc_offset: int = 0,
    qemu_start_idx: int = 0,
    pathn_start_idx: int = 0,
    ignore_boot_regs: bool = False,
    image_words: Optional[List[int]] = None,
    image_base: int = 0x40000000,
) -> Tuple[bool, Optional[str]]:
    """Compare traces starting from given alignment indices."""
    pn_idx = pathn_start_idx
    qm_idx = qemu_start_idx

    diffs_found = 0
    report_lines = []

    while pn_idx < len(pathn_records) and qm_idx < len(qemu_records):
        pn = pathn_records[pn_idx]
        qm = qemu_records[qm_idx]

        expected_qemu_pc = pn.pc + pc_offset

        # Check PC alignment
        if expected_qemu_pc != qm.pc:
            report_lines.append("=" * 60)
            report_lines.append(f"FIRST DIVERGENCE: PC Mismatch at Step")
            report_lines.append(f"  Path N Step: {pn.step} (PC={pn.pc:#018x})")
            report_lines.append(f"  QEMU   Step: {qm.step} (PC={qm.pc:#018x})")
            report_lines.append(f"  Expected QEMU PC: {expected_qemu_pc:#018x} (offset {pc_offset:#x})")
            return False, "\n".join(report_lines)

        # Check fetched word against the kernel image (catches a PC match
        # where the two sides fetched different opcodes).
        mismatches = []
        if image_words is not None and pn.word is not None:
            file_off = pn.pc - image_base
            if 0 <= file_off < len(image_words) * 4 and file_off % 4 == 0:
                expected = image_words[file_off // 4]
                if pn.word != expected:
                    mismatches.append(
                        f"  WORD: Path N fetched {pn.word:#010x} != image word "
                        f"{expected:#010x} at PC={pn.pc:#018x}"
                    )

        # Check Registers
        for r_idx in range(31):
            if ignore_boot_regs and r_idx in (0, 4, 21):
                # X0 (DTB pointer), X4 (QEMU stub sets kernel entry 0x40080000,
                # Path N sets 0), and X21 (preserved boot args) differ by boot contract
                continue
            pn_val = pn.regs[r_idx]
            qm_val = qm.regs[r_idx]
            # Kernel addresses differ by exactly pc_offset between the two load
            # bases (Path N 0x40000000 vs QEMU 0x40080000); values differing by
            # exactly the offset are a match, not a divergence.
            addrs_aligned = pc_offset != 0 and abs(qm_val - pn_val) == pc_offset
            if pn_val != qm_val and not addrs_aligned:
                mismatches.append(
                    f"  X{r_idx:02d}: Path N = {pn_val:#018x} | QEMU = {qm_val:#018x}"
                )

        if not ignore_boot_regs and pn.sp != qm.sp:
            mismatches.append(
                f"  SP : Path N = {pn.sp:#018x} | QEMU = {qm.sp:#018x}"
            )

        if not ignore_boot_regs and pn.nzcv != qm.nzcv:
            mismatches.append(
                f"  NZCV: Path N = {format_nzcv(pn.nzcv)} | QEMU = {format_nzcv(qm.nzcv)}"
            )

        if mismatches:
            report_lines.append("=" * 60)
            report_lines.append(f"FIRST DIVERGENCE: Register Mismatch at Step {pn.step}")
            report_lines.append(f"  Path N: Step {pn.step}, PC = {pn.pc:#018x}")
            report_lines.append(f"  QEMU:   Step {qm.step}, PC = {qm.pc:#018x}")
            report_lines.append("Mismatched Registers:")
            for m in mismatches:
                report_lines.append(m)
            return False, "\n".join(report_lines)

        pn_idx += 1
        qm_idx += 1

    steps_compared = pn_idx - pathn_start_idx
    return True, f"No divergence found in {steps_compared} steps."


def _validate_singlestep(qemu_records: List[CpuRecord]) -> None:
    """Fail loudly if the QEMU trace looks TB-granular instead of singlestep.

    In -singlestep mode each record is one instruction, so straight-line code
    shows a PC+4 chain. In TB mode, consecutive records jump by whole blocks.
    We check the first 500 records: count PC+4 transitions vs larger jumps.
    A TB-granular trace shows mostly large jumps -> reject.
    """
    n = min(500, len(qemu_records) - 1)
    if n <= 0:
        print("[oracle] Warning: too few records for singlestep validation")
        return
    plus4 = 0
    jumps = 0
    for i in range(n):
        d = qemu_records[i + 1].pc - qemu_records[i].pc
        if d == 4:
            plus4 += 1
        elif d > 4:
            jumps += 1
        # d <= 0 (branches/backwards) are neutral — not counted either way.
    # In singlestep kernel boot, straight-line runs dominate: expect mostly +4.
    # In TB mode, nearly every transition is a multi-instruction jump.
    total = plus4 + jumps
    if total == 0:
        print("[oracle] Warning: no forward PC transitions in validation window")
        return
    frac = plus4 / total
    print(f"[oracle] Singlestep check: {plus4}/{total} forward transitions are PC+4 ({frac:.0%})")
    if frac < 0.5:
        print("[oracle] ERROR: trace looks TB-granular, not -singlestep.")
        print("[oracle] Re-capture QEMU with: -singlestep -d cpu,nochain")
        sys.exit(1)
    print("[oracle] Singlestep cadence OK")


def main():
    parser = argparse.ArgumentParser(
        description="Compare Path N execution trace against QEMU -d cpu oracle"
    )
    parser.add_argument("--qemu", required=True, help="Path to QEMU -d cpu log file")
    parser.add_argument("--pathn", required=True, help="Path to Path N emulator trace file")
    parser.add_argument(
        "--pc-offset",
        type=lambda x: int(x, 0),
        default=None,
        help="PC offset between Path N and QEMU (default: auto-detect 0x80000 kernel load offset)",
    )
    parser.add_argument(
        "--ignore-boot-regs",
        action="store_true",
        help="Ignore known boot-contract mismatches (X0=DTB, X4=kernel entry, X21=saved DTB, SP=RAM_TOP, initial PSTATE)",
    )
    parser.add_argument(
        "--image",
        default=None,
        help="Path to kernel Image; when given, each Path N fetched word is "
        "checked against the image (catches a PC match with different opcodes)",
    )
    parser.add_argument(
        "--pathn-image-base",
        type=lambda x: int(x, 0),
        default=0x40000000,
        help="Guest-physical base where Path N loaded the image (default 0x40000000)",
    )
    parser.add_argument(
        "--max-steps",
        type=int,
        default=None,
        help="Maximum steps to compare",
    )
    parser.add_argument(
        "--keep-pc-range",
        default="0x40080000:0x41000000",
        help="Keep only QEMU records with PC in [start:end) (kernel range). "
        "Filters out reset stub (0x40000000) and EL3 firmware. "
        "Format: 0xSTART:0xEND. Set to empty string to disable.",
    )
    parser.add_argument(
        "--validate-singlestep",
        action="store_true",
        help="Validate QEMU trace looks like -singlestep output (PC+4 chain check "
        "on first 500 records) before diffing. Fails loudly if TB-granular.",
    )

    args = parser.parse_args()

    print(f"[oracle] Loading Path N trace: {args.pathn}")
    pathn_records = parse_pathn_trace(args.pathn, args.max_steps)
    print(f"[oracle] Loaded {len(pathn_records)} Path N steps")

    print(f"[oracle] Loading QEMU trace: {args.qemu}")
    qemu_records = parse_qemu_trace(args.qemu, args.max_steps)
    print(f"[oracle] Loaded {len(qemu_records)} QEMU steps")

    if not pathn_records or not qemu_records:
        print("[oracle] Error: one or both traces are empty")
        sys.exit(1)

    # Singlestep validation: QEMU -d cpu without -singlestep emits TB-granular
    # records, which are incommensurable with per-instruction emulator traces.
    # Check PC+4 chain on straight-line code before trusting the diff.
    if args.validate_singlestep:
        _validate_singlestep(qemu_records)

    # Firmware-aware filtering: keep only kernel-range PCs.
    # Drops the reset stub (0x40000000) and EL3 firmware (e.g. 0x41440000).
    if args.keep_pc_range:
        try:
            start_s, end_s = args.keep_pc_range.split(":")
            kstart, kend = int(start_s, 0), int(end_s, 0)
        except ValueError:
            print(f"[oracle] Error: bad --keep-pc-range '{args.keep_pc_range}', want 0xSTART:0xEND")
            sys.exit(1)
        before = len(qemu_records)
        qemu_records = [r for r in qemu_records if kstart <= r.pc < kend]
        # Re-number steps after filtering so ordinals stay dense.
        for i, r in enumerate(qemu_records):
            r.step = i
        print(f"[oracle] PC-range filter [{kstart:#x}:{kend:#x}): {before} -> {len(qemu_records)} records")
        if not qemu_records:
            print("[oracle] Error: no QEMU records in kernel range — is this a -singlestep trace?")
            sys.exit(1)

    # Optional kernel image for fetched-word verification.
    image_words = None
    if args.image:
        with open(args.image, "rb") as f:
            data = f.read()
        image_words = [
            int.from_bytes(data[i : i + 4], "little")
            for i in range(0, len(data) - 3, 4)
        ]
        print(f"[oracle] Loaded {len(image_words)} image words from {args.image}")

    # Alignment: after PC-range filtering, QEMU record 0 should be the
    # kernel entry (0x40080000). Path N step 0 is also the kernel entry.
    # No stub-skip needed anymore — the filter already dropped 0x40000000.
    pc_offset = args.pc_offset
    qemu_start = 0

    if pc_offset is None:
        pc_offset = 0
        if qemu_records and qemu_records[0].pc == 0x40080000:
            print(f"[oracle] Aligned: Path N step 0 <-> QEMU step 0 (PC=0x40080000)")
        else:
            first_pc = qemu_records[0].pc if qemu_records else 0
            print(f"[oracle] Warning: first filtered QEMU PC is {first_pc:#x}, expected 0x40080000")

    ok, msg = diff_records(
        pathn_records,
        qemu_records,
        pc_offset=pc_offset,
        qemu_start_idx=qemu_start,
        pathn_start_idx=0,
        ignore_boot_regs=args.ignore_boot_regs,
        image_words=image_words,
        image_base=args.pathn_image_base,
    )

    print(msg)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
