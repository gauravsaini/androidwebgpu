#!/usr/bin/env python3
"""Find repeated AArch64 instruction paths in a Path N per-step trace.

Trace rows use the format written by u12-orchestrator:
``step pc word x0 ... x30 sp nzcv``. The detector streams the file and keeps
only a bounded instruction window, so it can inspect large boot traces.
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import io
import re
import sys
from contextlib import redirect_stdout
from pathlib import Path
from typing import Iterable, Optional


MASK64 = (1 << 64) - 1
REG_NAMES = tuple(f"x{i}" for i in range(31))
CONDITIONS = (
    "eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc",
    "hi", "ls", "ge", "lt", "gt", "le", "al", "nv",
)


@dataclasses.dataclass(frozen=True)
class Instruction:
    index: int
    step: int
    pc: int
    word: int
    regs: tuple[int, ...]
    sp: int
    nzcv: int

    @property
    def state(self) -> tuple[int, ...]:
        return self.regs + (self.sp, self.nzcv)


@dataclasses.dataclass
class Loop:
    period: int
    pattern: tuple[Instruction, ...]
    start_index: int
    iterations: int
    headers: list[tuple[int, ...]]
    stable_transitions: int
    active: bool = True
    exit_step: Optional[int] = None
    exit_branch: Optional[int] = None
    taken: collections.Counter[int] = dataclasses.field(default_factory=collections.Counter)
    missed: collections.Counter[int] = dataclasses.field(default_factory=collections.Counter)

    @property
    def pcs(self) -> tuple[int, ...]:
        return tuple(dict.fromkeys(row.pc for row in self.pattern))

    @property
    def key(self) -> tuple[int, tuple[int, ...]]:
        return (self.period, canonical_rotation(tuple(row.pc for row in self.pattern)))


def sign_extend(value: int, width: int) -> int:
    sign = 1 << (width - 1)
    return (value ^ sign) - sign


def reg_name(number: int, width: int = 64, *, sp: bool = False) -> str:
    suffix = "" if width == 64 else "w"
    if number == 31:
        return "sp" if sp else "xzr" if width == 64 else "wzr"
    return f"{suffix}{number}" if suffix else f"x{number}"


def branch_info(word: int, pc: int, regs: Optional[tuple[int, ...]] = None) -> Optional[tuple[str, Optional[int]]]:
    """Return (mnemonic, static-or-resolved target) for common A64 branches."""
    if word & 0x7C000000 == 0x14000000:
        link = bool(word & 0x80000000)
        target = (pc + (sign_extend(word & 0x03FFFFFF, 26) << 2)) & MASK64
        return ("bl" if link else "b", target)
    if word & 0xFF000010 == 0x54000000:
        cond = word & 0xF
        target = (pc + (sign_extend((word >> 5) & 0x7FFFF, 19) << 2)) & MASK64
        return (f"b.{CONDITIONS[cond]}", target)
    if word & 0x7E000000 == 0x34000000:
        width = 64 if word >> 31 else 32
        operation = "cbnz" if (word >> 24) & 1 else "cbz"
        target = (pc + (sign_extend((word >> 5) & 0x7FFFF, 19) << 2)) & MASK64
        return (f"{operation} {reg_name(word & 0x1F, width)}, #0x{target:x}", target)
    if word & 0x7E000000 == 0x36000000:
        bit = (((word >> 31) & 1) << 5) | ((word >> 19) & 0x1F)
        operation = "tbnz" if (word >> 24) & 1 else "tbz"
        target = (pc + (sign_extend((word >> 5) & 0x3FFF, 14) << 2)) & MASK64
        return (f"{operation} {reg_name(word & 0x1F)}, #{bit}, #0x{target:x}", target)
    if word & 0xFFFFFC1F in (0xD61F0000, 0xD63F0000, 0xD65F0000):
        operation = {0xD61F0000: "br", 0xD63F0000: "blr", 0xD65F0000: "ret"}[word & 0xFFFFFC1F]
        rn = (word >> 5) & 0x1F
        target = regs[rn] if regs is not None and rn < 31 else None
        return (f"{operation} {reg_name(rn)}", target)
    return None


def disassemble(word: int, pc: int) -> str:
    """Readable built-in disassembly for common loop instructions."""
    try:
        from capstone import CS_ARCH_ARM64, CS_MODE_LITTLE_ENDIAN, Cs

        engine = Cs(CS_ARCH_ARM64, CS_MODE_LITTLE_ENDIAN)
        decoded = next(engine.disasm(word.to_bytes(4, "little"), pc), None)
        if decoded:
            return f"{decoded.mnemonic} {decoded.op_str}".rstrip()
    except (ImportError, AttributeError, StopIteration):
        pass

    branch = branch_info(word, pc)
    if branch:
        return branch[0]
    if word == 0xD503201F:
        return "nop"
    if word == 0xD503207F:
        return "wfi"
    if word == 0xD4000001:
        return "svc #0"

    # Logical shifted-register instructions, including ORN/MVN.
    if word & 0x1F000000 == 0x0A000000:
        width = 64 if word >> 31 else 32
        opc, invert = (word >> 29) & 0x3, (word >> 21) & 1
        names = ("and", "orr", "eor", "ands")
        name = names[opc]
        if invert:
            name = {"and": "bic", "orr": "orn", "eor": "eon", "ands": "bics"}[name]
        rd, rn, rm = word & 0x1F, (word >> 5) & 0x1F, (word >> 16) & 0x1F
        shift = (word >> 22) & 0x3
        amount = (word >> 10) & 0x3F
        shifts = ("lsl", "lsr", "asr", "ror")
        if name == "orn" and rn == 31 and shift == 0 and amount == 0:
            return f"mvn {reg_name(rd, width)}, {reg_name(rm, width)}"
        operands = f"{reg_name(rd, width)}, {reg_name(rn, width)}, {reg_name(rm, width)}"
        if shift or amount:
            operands += f", {shifts[shift]} #{amount}"
        return f"{name} {operands}"

    # ADD/SUB (immediate), including CMP/CMN aliases used by loop tests.
    if word & 0x1F000000 == 0x11000000:
        width = 64 if word >> 31 else 32
        subtract, flags = (word >> 30) & 1, (word >> 29) & 1
        rd, rn = word & 0x1F, (word >> 5) & 0x1F
        immediate = (word >> 10) & 0xFFF
        if (word >> 22) & 1:
            immediate <<= 12
        name = "sub" if subtract else "add"
        if flags:
            name += "s"
        if flags and rd == 31:
            name = "cmp" if subtract else "cmn"
            return f"{name} {reg_name(rn, width)}, #0x{immediate:x}"
        return f"{name} {reg_name(rd, width)}, {reg_name(rn, width)}, #0x{immediate:x}"

    # MOVN/MOVZ/MOVK.
    if word & 0x1F800000 == 0x12800000:
        width = 64 if word >> 31 else 32
        opc = (word >> 29) & 0x3
        name = {0: "movn", 2: "movz", 3: "movk"}.get(opc)
        if name:
            rd = word & 0x1F
            immediate = (word >> 5) & 0xFFFF
            shift = ((word >> 21) & 0x3) * 16
            return f"{name} {reg_name(rd, width)}, #0x{immediate:x}" + (f", lsl #{shift}" if shift else "")

    # ADR/ADRP.
    if word & 0x9F000000 == 0x10000000:
        page = (word >> 31) & 1
        immediate = sign_extend((((word >> 5) & 0x7FFFF) << 2) | ((word >> 29) & 0x3), 21)
        target = ((pc & ~0xFFF) + (immediate << 12)) if page else pc + immediate
        return f"{'adrp' if page else 'adr'} {reg_name(word & 0x1F)}, #0x{target:x}"

    # Common unsigned-immediate loads/stores.
    if word & 0x3B000000 == 0x39000000:
        width_code = (word >> 30) & 0x3
        size = (word >> 31) & 1
        byte_width = 1 << ((word >> 30) & 0x3)
        load = (word >> 22) & 1
        rt, rn = word & 0x1F, (word >> 5) & 0x1F
        offset = ((word >> 10) & 0xFFF) * byte_width
        suffix = {0: "b", 1: "h", 2: "", 3: ""}[width_code]
        data_width = 64 if size and width_code == 3 else 32 if width_code == 2 else 64
        operation = "ldr" if load else "str"
        return f"{operation}{suffix} {reg_name(rt, data_width)}, [{reg_name(rn, sp=True)}, #0x{offset:x}]"

    return f".word 0x{word:08x}"


def is_store(word: int) -> bool:
    if word & 0x3B000000 == 0x39000000:
        return ((word >> 22) & 1) == 0
    if word & 0x3A000000 == 0x28000000:
        return ((word >> 22) & 1) == 0
    # Exclusive/release stores and atomic read-modify-write instructions.
    if word & 0x3F000000 in (0x08000000, 0x38000000):
        return ((word >> 22) & 1) == 0
    return False


def parse_trace_row(line: str, index: int) -> Optional[Instruction]:
    fields = line.split()
    if len(fields) >= 36 and fields[0].isdigit() and fields[1].startswith("0x"):
        try:
            return Instruction(
                index=index,
                step=int(fields[0]),
                pc=int(fields[1], 0),
                word=int(fields[2], 0),
                regs=tuple(int(value, 0) for value in fields[3:34]),
                sp=int(fields[34], 0),
                nzcv=int(fields[35], 0),
            )
        except ValueError:
            return None

    # Also accept key/value rows from diagnostic trace tools.
    keyed = dict(re.findall(r"(?:^|\s)(step|pc|word|sp|nzcv|x(?:[0-9]|[12][0-9]|30))=([^\s]+)", line))
    if not {"step", "pc", "word", "sp", "nzcv"}.issubset(keyed):
        return None
    if not all(f"x{register}" in keyed for register in range(31)):
        return None
    try:
        return Instruction(
            index=index,
            step=int(keyed["step"], 0),
            pc=int(keyed["pc"], 0),
            word=int(keyed["word"], 0),
            regs=tuple(int(keyed[f"x{register}"], 0) for register in range(31)),
            sp=int(keyed["sp"], 0),
            nzcv=int(keyed["nzcv"], 0),
        )
    except ValueError:
        return None


def canonical_rotation(values: tuple[int, ...]) -> tuple[int, ...]:
    """Booth's linear-time lexicographically minimal rotation."""
    count = len(values)
    if count < 2:
        return values
    doubled = values + values
    left, right, matched = 0, 1, 0
    while left < count and right < count and matched < count:
        a, b = doubled[left + matched], doubled[right + matched]
        if a == b:
            matched += 1
        elif a > b:
            left += matched + 1
            if left == right:
                left += 1
            matched = 0
        else:
            right += matched + 1
            if left == right:
                right += 1
            matched = 0
    start = min(left, right)
    return doubled[start : start + count]


def minimal_period(values: tuple[int, ...]) -> int:
    for period in range(1, len(values) + 1):
        if len(values) % period == 0 and all(values[index] == values[index % period] for index in range(period, len(values))):
            return period
    return len(values)


def countdown_register(headers: list[tuple[int, ...]]) -> Optional[tuple[str, int]]:
    if len(headers) < 3:
        return None
    best: Optional[tuple[str, int, int]] = None
    for register in range(31):
        deltas = [after[register] - before[register] for before, after in zip(headers, headers[1:])]
        tail = deltas[-2:]
        if len(tail) == 2 and tail[0] == tail[1] and tail[0] < 0:
            candidate = (f"x{register}", tail[0], len(deltas))
            if best is None or candidate[2] > best[2]:
                best = candidate
    return (best[0], best[1]) if best else None


class Detector:
    def __init__(self, max_period: int):
        self.capacity = max_period * 2 + 1
        self.history: list[Optional[Instruction]] = [None] * self.capacity
        self.prefix_a = [0] * self.capacity
        self.prefix_b = [0] * self.capacity
        self.base_a = 0x9E3779B185EBCA87
        self.base_b = 0xC2B2AE3D27D4EB4F
        self.powers_a = [1] * (max_period + 1)
        self.powers_b = [1] * (max_period + 1)
        for size in range(1, max_period + 1):
            self.powers_a[size] = (self.powers_a[size - 1] * self.base_a) & MASK64
            self.powers_b[size] = (self.powers_b[size - 1] * self.base_b) & MASK64
        self.max_period = max_period
        self.occurrences: dict[int, collections.deque[int]] = collections.defaultdict(
            lambda: collections.deque(maxlen=4)
        )
        self.loops: dict[tuple[int, tuple[int, ...]], Loop] = {}
        self.covered: set[tuple[int, int]] = set()
        self.branch_outcomes: dict[int, collections.Counter[int]] = collections.defaultdict(collections.Counter)
        self.row_count = 0
        self.previous: Optional[Instruction] = None

    def _at(self, absolute_index: int) -> Optional[Instruction]:
        if absolute_index < 0 or absolute_index >= self.row_count:
            return None
        row = self.history[absolute_index % self.capacity]
        return row if row is not None and row.index == absolute_index else None

    def _slice_hash(self, start: int, size: int, which: int) -> int:
        prefix = self.prefix_a if which == 0 else self.prefix_b
        power = self.powers_a[size] if which == 0 else self.powers_b[size]
        end = start + size - 1
        before = 0 if start == 0 else prefix[(start - 1) % self.capacity]
        after = prefix[end % self.capacity]
        return (after - before * power) & MASK64

    def _matches(self, first: int, second: int, size: int) -> bool:
        if self._slice_hash(first, size, 0) != self._slice_hash(second, size, 0):
            return False
        if self._slice_hash(first, size, 1) != self._slice_hash(second, size, 1):
            return False
        for offset in range(size):
            left = self._at(first + offset)
            right = self._at(second + offset)
            if left is None or right is None or left.pc != right.pc:
                return False
        return True

    def _loop_exit(self, loop: Loop, row: Instruction) -> bool:
        previous = self._at(row.index - 1)
        if previous is None:
            return False
        offset = (previous.index - loop.start_index) % loop.period
        info = branch_info(previous.word, previous.pc, previous.regs)
        if info is None or info[1] is None:
            return False
        pc_positions: dict[int, list[int]] = collections.defaultdict(list)
        for position, instruction in enumerate(loop.pattern):
            pc_positions[instruction.pc].append(position)
        target_positions = pc_positions.get(info[1], [])
        return any(position <= offset for position in target_positions) and row.pc != info[1]

    def _update_loops(self, row: Instruction) -> None:
        for loop in tuple(self.loops.values()):
            if not loop.active:
                continue
            offset = (row.index - loop.start_index) % loop.period
            expected_pc = loop.pattern[offset].pc
            if row.pc != expected_pc:
                loop.active = False
                loop.exit_step = row.step
                if self._loop_exit(loop, row):
                    loop.exit_branch = self._at(row.index - 1).pc  # type: ignore[union-attr]
                    loop.iterations += 1
                continue
            if offset == 0:
                before = loop.headers[-1] if loop.headers else None
                loop.headers.append(row.state)
                loop.iterations += 1
                if before == row.state:
                    loop.stable_transitions += 1
                else:
                    loop.stable_transitions = 0

    def add(self, row: Instruction) -> None:
        if self.previous is not None:
            branch = branch_info(self.previous.word, self.previous.pc, self.previous.regs)
            if branch and branch[1] is not None:
                self.branch_outcomes[self.previous.pc][row.pc] += 1

        self._update_loops(row)
        slot = row.index % self.capacity
        self.history[slot] = row
        word_hash = (row.pc ^ (row.pc >> 32)) & MASK64
        previous_a = self.prefix_a[(row.index - 1) % self.capacity] if row.index else 0
        previous_b = self.prefix_b[(row.index - 1) % self.capacity] if row.index else 0
        self.prefix_a[slot] = ((previous_a * self.base_a) + word_hash) & MASK64
        self.prefix_b[slot] = ((previous_b * self.base_b) + (word_hash ^ 0x517CC1B727220A95)) & MASK64

        recent = self.occurrences[row.pc]
        if len(recent) >= 2:
            for old_index in tuple(recent)[-3:]:
                period = row.index - old_index
                if period <= 0 or period > self.max_period or (period, row.pc) in self.covered:
                    continue
                prior_index = old_index - period
                if prior_index < 0 or not self._matches(prior_index, old_index, period):
                    continue
                pattern_rows = tuple(self._at(prior_index + offset) for offset in range(period))
                if any(item is None for item in pattern_rows):
                    continue
                matched_pattern = tuple(item for item in pattern_rows if item is not None)
                path = tuple(item.pc for item in matched_pattern)
                actual_period = minimal_period(path)
                pattern = matched_pattern[:actual_period]
                key = (actual_period, canonical_rotation(tuple(item.pc for item in pattern)))
                if key in self.loops:
                    continue
                header_rows = [
                    row if prior_index + offset == row.index else self._at(prior_index + offset)
                    for offset in range(0, 2 * period + 1, actual_period)
                ]
                if any(header is None for header in header_rows):
                    continue
                headers = [header.state for header in header_rows if header is not None]
                stable = 0
                for before, after in zip(headers, headers[1:]):
                    if before == after:
                        stable += 1
                    else:
                        stable = 0
                loop = Loop(
                    period=actual_period,
                    pattern=pattern,
                    start_index=prior_index,
                    iterations=len(headers) - 1,
                    headers=headers,
                    stable_transitions=stable,
                )
                self.loops[key] = loop
                for pc in set(item.pc for item in pattern):
                    self.covered.add((actual_period, pc))
                break

        recent.append(row.index)
        self.row_count += 1
        self.previous = row

    def process(self, lines: Iterable[str]) -> None:
        for line in lines:
            row = parse_trace_row(line, self.row_count)
            if row is not None:
                self.add(row)


def loop_status(loop: Loop) -> tuple[str, Optional[tuple[str, int]]]:
    countdown = countdown_register(loop.headers)
    if loop.active:
        if countdown:
            return (f"COUNTDOWN LOOP (not an infinite spin; {countdown[0]} decreases by {abs(countdown[1])} per iteration)", countdown)
        has_store = any(is_store(item.word) for item in loop.pattern)
        if loop.stable_transitions >= 2 and not has_store:
            return ("TRUE INFINITE SPIN (repeating PC path and full CPU state; no stores in the cycle)", None)
        if loop.stable_transitions >= 2:
            return ("REPEATING STATE LOOP (store in cycle prevents an infinite-spin proof)", None)
        return ("REPEATING LOOP (spin not proven; state still changes or trace ended early)", None)
    if countdown:
        return (f"FINITE METADATA SWEEP ({countdown[0]} decreases by {abs(countdown[1])} per iteration; loop exited)", countdown)
    return ("FINITE LOOP (the repeated PC path exited)", None)


def branch_lines(loop: Loop, outcomes: dict[int, collections.Counter[int]]) -> list[str]:
    pc_positions: dict[int, list[int]] = collections.defaultdict(list)
    for position, instruction in enumerate(loop.pattern):
        pc_positions[instruction.pc].append(position)
    lines: list[str] = []
    for position, instruction in enumerate(loop.pattern):
        info = branch_info(instruction.word, instruction.pc, instruction.regs)
        if info is None:
            continue
        mnemonic, target = info
        if target not in pc_positions:
            continue
        back_edge = any(target_position <= position for target_position in pc_positions[target])
        if not back_edge:
            continue
        counts = outcomes.get(instruction.pc, collections.Counter())
        taken = counts.get(target, 0)
        not_taken = sum(count for next_pc, count in counts.items() if next_pc != target)
        lines.append(
            f"  branch {instruction.pc:#018x}: {mnemonic} -> {target:#018x}"
            f" (taken {taken}, not-taken {not_taken})"
        )
    return lines


def print_report(detector: Detector, limit: int) -> int:
    loops = sorted(detector.loops.values(), key=lambda item: (-item.iterations, item.period, item.pattern[0].pc))
    if not loops:
        print("No repeated PC paths found within the configured period.")
        return 0
    shown = loops[:limit]
    for number, loop in enumerate(shown, 1):
        status, countdown = loop_status(loop)
        first, last = loop.pattern[0], loop.pattern[-1]
        print(
            f"Loop {number}: {loop.period}-instruction cycle, {len(loop.pcs)} looping PCs, "
            f"{loop.iterations} iterations ({status})"
        )
        print(f"  observed steps: {first.step}..{last.step} (cycle anchor {first.pc:#018x})")
        if countdown:
            print(f"  progress register: {countdown[0]} delta={countdown[1]}")
        branches = branch_lines(loop, detector.branch_outcomes)
        if branches:
            print("  loop branch:")
            for line in branches:
                print(line)
        else:
            print("  loop branch: not present in this trace window")
        print("  looping PCs:")
        emitted: set[int] = set()
        for instruction in loop.pattern:
            if instruction.pc in emitted:
                continue
            emitted.add(instruction.pc)
            print(
                f"    {instruction.pc:#018x}: {instruction.word:#010x}  "
                f"{disassemble(instruction.word, instruction.pc)}"
            )
    if len(loops) > limit:
        print(f"... {len(loops) - limit} additional loop(s) omitted; raise --max-report-loops to show them.")
    return 0


def build_sample_trace(bitmap_spin: bool) -> str:
    """Small executable fixture modeled on Track 20's x24/MVN bitmap spin."""
    loop_head = 0xFFFFFF80085C0D68
    mvn_pc = loop_head - 4
    body = [
        (mvn_pc, 0xAA3803F8),  # mvn x24, x24; the known emulator bug leaves x24 == 3
        (loop_head, 0xD503201F),
        (loop_head + 4, 0x17FFFFFE),  # b mvn_pc
    ]
    rows: list[str] = []
    regs = [0] * 31
    regs[24] = 3
    pc = mvn_pc
    words = dict(body)
    limit = 3 + (4 if bitmap_spin else 1) * 8
    for step in range(limit):
        word = words.get(pc, 0xD503201F)
        row = [str(step), f"0x{pc:016x}", f"0x{word:08x}"]
        row.extend(f"0x{value:016x}" for value in regs)
        row.extend(("0x0000000000000000", "0x8"))
        rows.append(" ".join(row))
        if pc == loop_head + 4:
            pc += 4
            pc = mvn_pc
        else:
            pc += 4
        if not bitmap_spin and step == 5:
            break
    return "\n".join(rows) + "\n"


def self_test() -> int:
    # Track 20 (codex-track20-bitmap.md): MVN x24,x24 at ...d64, followed by
    # a repeated bitmap search at ...d68 with x24 stuck at 3.
    spin = Detector(max_period=32)
    spin.process(build_sample_trace(bitmap_spin=True).splitlines())
    spin_loop = max(spin.loops.values(), key=lambda item: item.iterations)
    status, _ = loop_status(spin_loop)
    assert "TRUE INFINITE SPIN" in status, status
    assert spin_loop.period == 3, spin_loop.period
    assert any(item.pc == 0xFFFFFF80085C0D64 for item in spin_loop.pattern)
    assert spin_loop.stable_transitions >= 2
    assert all(header[24] == 3 for header in spin_loop.headers)
    assert disassemble(0xAA3803F8, 0xFFFFFF80085C0D64) == "mvn x24, x24"
    spin_output = io.StringIO()
    with redirect_stdout(spin_output):
        print_report(spin, 10)
    assert "TRUE INFINITE SPIN" in spin_output.getvalue()
    assert "mvn x24, x24" in spin_output.getvalue()
    assert "loop branch" in spin_output.getvalue()

    # A finite x14 table walk repeats a two-PC path while its limit falls.
    sweep = Detector(max_period=16)
    rows: list[str] = []
    start = 0x2000
    subs_x14_1 = 0xF10005CE  # subs x14, x14, #1
    cbnz_x14_back = 0xB5FFFFEE  # cbnz x14, 0x2000
    values = [3, 2, 1]
    step = 0
    for value in values:
        regs = [0] * 31
        regs[14] = value
        for pc, word, current in ((start, subs_x14_1, value), (start + 4, cbnz_x14_back, value - 1)):
            regs[14] = current
            fields = [str(step), f"0x{pc:016x}", f"0x{word:08x}"]
            fields.extend(f"0x{register:016x}" for register in regs)
            fields.extend(("0x0000000000000000", "0x0"))
            rows.append(" ".join(fields))
            step += 1
    regs = [0] * 31
    rows.append(" ".join([str(step), f"0x{start + 8:016x}", "0xd503201f"] + [f"0x{value:016x}" for value in regs] + ["0x0", "0x0"]))
    sweep.process(rows)
    sweep_loop = next(loop for loop in sweep.loops.values() if loop.period == 2)
    sweep_status, countdown = loop_status(sweep_loop)
    assert "FINITE METADATA SWEEP" in sweep_status, sweep_status
    assert countdown == ("x14", -1), countdown
    sweep_output = io.StringIO()
    with redirect_stdout(sweep_output):
        print_report(sweep, 10)
    assert "FINITE METADATA SWEEP" in sweep_output.getvalue()
    assert "progress register: x14 delta=-1" in sweep_output.getvalue()
    print("spin-detector self-test: PASS (Track 20 bitmap spin; finite x14 metadata sweep)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", nargs="?", help="Path N per-step trace file")
    parser.add_argument("--max-period", type=int, default=256, help="Largest loop body to inspect (default: 256 PCs)")
    parser.add_argument("--max-report-loops", type=int, default=10, help="Maximum loops to print (default: 10)")
    parser.add_argument("--self-test", action="store_true", help="Run the Track 20 bitmap and x14 countdown smoke tests")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.trace:
        parser.error("trace is required unless --self-test is used")
    if args.max_period < 1 or args.max_report_loops < 1:
        parser.error("--max-period and --max-report-loops must be positive")
    path = Path(args.trace)
    detector = Detector(args.max_period)
    try:
        with path.open("r", encoding="utf-8", errors="replace") as trace:
            detector.process(trace)
    except OSError as error:
        print(f"spin-detector: {error}", file=sys.stderr)
        return 2
    return print_report(detector, args.max_report_loops)


if __name__ == "__main__":
    raise SystemExit(main())
