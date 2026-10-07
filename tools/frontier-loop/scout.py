#!/usr/bin/env python3
"""Look past one emulator halt at a time, recording every forced skip as imprecise."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
SKIP_MANIFEST = ROOT / "tools/frontier-loop/scout_skip/Cargo.toml"
DEFAULT_MMIO_TRACE = Path("/Users/Shared/codex-logs/track66-qemu-mmio.trace")

sys.path.insert(0, str(ROOT))


def say(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


def run(command: list[str], *, cwd: Path = ROOT, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, check=False)


def build_runner(binary_override: str | None) -> Path:
    if binary_override:
        binary = Path(binary_override).expanduser().resolve()
        if not binary.is_file():
            raise RuntimeError(f"--run-kernel points to missing file: {binary}")
        return binary

    binary = ROOT / "target/release/run_kernel"
    env = os.environ.copy()
    revision = run(["git", "rev-parse", "HEAD"], env=env)
    if revision.returncode:
        raise RuntimeError(f"git rev-parse HEAD failed: {revision.stderr.strip()}")
    env["PATHN_BUILD_GIT_REV"] = revision.stdout.strip()
    build = run(
        ["cargo", "build", "--offline", "--release", "-p", "u12-orchestrator", "--bin", "run_kernel"],
        env=env,
    )
    if build.returncode:
        raise RuntimeError(f"run_kernel build failed:\n{build.stdout}{build.stderr}")
    if not binary.is_file():
        raise RuntimeError(f"cargo build did not produce {binary}")
    return binary


def build_skipper(target_dir: Path) -> Path:
    binary = target_dir / "release/scout-skip"
    result = run(
        [
            "cargo",
            "build",
            "--offline",
            "--release",
            "--manifest-path",
            str(SKIP_MANIFEST),
            "--target-dir",
            str(target_dir),
        ]
    )
    if result.returncode:
        raise RuntimeError(f"snapshot skip helper build failed:\n{result.stdout}{result.stderr}")
    return binary


def parse_help(binary: Path) -> str:
    result = run([str(binary), "--help"])
    if result.returncode:
        raise RuntimeError(f"run_kernel --help failed:\n{result.stdout}{result.stderr}")
    return result.stdout + result.stderr


def number(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        match = re.search(r"0x[0-9a-fA-F]+|\b\d+\b", value)
        if match:
            token = match.group(0)
            try:
                return int(token, 16 if token.lower().startswith("0x") else 10)
            except ValueError:
                return None
    return None


def find_values(value: Any, keys: set[str]) -> list[Any]:
    found: list[Any] = []
    if isinstance(value, dict):
        for key, child in value.items():
            normalized = str(key).lower().replace("-", "_")
            if normalized in keys or normalized.rsplit(".", 1)[-1] in keys:
                found.append(child)
            found.extend(find_values(child, keys))
    elif isinstance(value, list):
        for child in value:
            found.extend(find_values(child, keys))
    return found


def first_value(value: Any, keys: set[str]) -> Any:
    values = find_values(value, keys)
    return values[0] if values else None


def parse_trace_row(path: Path) -> dict[str, Any] | None:
    last: dict[str, Any] | None = None
    if not path.is_file():
        return None
    with path.open("r", encoding="utf-8", errors="replace") as trace:
        for line in trace:
            fields = line.split()
            if len(fields) < 36 or not fields[0].isdigit():
                continue
            try:
                last = {
                    "step": int(fields[0]),
                    "pc": int(fields[1], 0),
                    "word": int(fields[2], 0),
                    "regs": [int(field, 0) for field in fields[3:34]],
                    "sp": int(fields[34], 0),
                    "nzcv": int(fields[35], 0),
                }
            except ValueError:
                continue
    return last


def trace_row_from_snapshot(helper: Path, snapshot: Path, step_hint: int | None = None) -> dict[str, Any] | None:
    state = inspect_machine(helper, snapshot)
    pc = number(state.get("live_pc"))
    word = number(state.get("word"))
    step = number(state.get("snapshot_step")) or step_hint
    if pc is None or word is None or step is None:
        return None
    return {"step": step, "pc": pc, "word": word, "regs": [], "sp": 0, "nzcv": 0}


def parse_legacy_output(stdout: str, trace_row: dict[str, Any] | None) -> dict[str, Any]:
    done = re.search(r"^\[done\] Executed (\d+) steps\. PC=(0x[0-9a-fA-F]+)", stdout, re.MULTILINE)
    halt = re.search(r"^\[halt\] Reason:\s*(.+)$", stdout, re.MULTILINE)
    uart_bytes = 0
    uart_match = re.search(r"^UART_BYTES=(\d+)$", stdout, re.MULTILINE)
    if uart_match:
        uart_bytes = int(uart_match.group(1))
    symbol = re.search(r"^HALT_FN=(.+)$", stdout, re.MULTILINE)
    step = int(done.group(1)) if done else (trace_row["step"] if trace_row else None)
    pc = int(done.group(2), 16) if done else (trace_row["pc"] if trace_row else None)
    return {
        "halt_step": step,
        "pc": pc,
        "reason": halt.group(1).strip() if halt else None,
        "symbol": symbol.group(1).strip() if symbol else None,
        "uart_bytes": uart_bytes,
        "source": "legacy run_kernel stdout + trace",
    }


def load_report(report_path: Path, stdout: str, trace_row: dict[str, Any] | None) -> dict[str, Any]:
    payload: Any = None
    if report_path.is_file():
        try:
            payload = json.loads(report_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            payload = None
    if payload is None:
        for line in reversed(stdout.splitlines()):
            candidate = line.strip()
            if candidate.startswith("{") and candidate.endswith("}"):
                try:
                    payload = json.loads(candidate)
                    break
                except json.JSONDecodeError:
                    pass
    if payload is None:
        payload = parse_legacy_output(stdout, trace_row)
    else:
        if isinstance(payload, dict):
            source = payload
        else:
            source = {"payload": payload}
        legacy = parse_legacy_output(stdout, trace_row)
        step = first_value(source, {"halt_step", "step"})
        pc = first_value(source, {"halt_pc", "pc"})
        reason = first_value(source, {"halt_reason", "reason"})
        symbol = first_value(source, {"symbol", "halt_fn", "function"})
        if isinstance(reason, dict):
            reason = reason.get("kind") or reason.get("type") or json.dumps(reason, sort_keys=True)
        payload = {
            **source,
            "halt_step": number(step) if step is not None else legacy["halt_step"],
            "pc": number(pc) if pc is not None else legacy["pc"],
            "reason": reason if reason is not None else legacy["reason"],
            "symbol": str(symbol) if symbol is not None else legacy["symbol"],
            "source": "frontier-boot-report/1",
        }
    if not isinstance(payload, dict):
        payload = {"payload": payload}
    if trace_row:
        payload.setdefault("trace_pc", trace_row["pc"])
        payload.setdefault("trace_word", trace_row["word"])
        payload.setdefault("trace_step", trace_row["step"])
    marker = first_value(payload, {"init_marker"})
    if marker is None:
        uart = payload.get("uart")
        if isinstance(uart, dict):
            marker = uart.get("init_marker")
    payload["uart_init_marker"] = marker if isinstance(marker, bool) else None
    return payload


def symbol_for_pc(pc: int | None, symbol_map: Path | None) -> str:
    if pc is None or not symbol_map or not symbol_map.is_file():
        return "unknown"
    nm = run(["nm", "-n", str(symbol_map)])
    lines = nm.stdout.splitlines() if nm.returncode == 0 else []
    try:
        # Also accept Linux System.map or /proc/kallsyms text files directly.
        lines.extend(symbol_map.read_text(encoding="utf-8", errors="replace").splitlines())
    except OSError:
        pass
    nearest: tuple[int, str] | None = None
    for line in lines:
        match = re.match(r"^\s*([0-9a-fA-F]+)\s+(?:\S\s+)?(.+)$", line)
        if not match:
            continue
        address, name = int(match.group(1), 16), match.group(2).strip()
        if address <= pc and (nearest is None or address > nearest[0]):
            nearest = (address, name)
    if not nearest:
        return "unknown"
    return f"{nearest[1]}+0x{pc - nearest[0]:x}"


def get_field(report: dict[str, Any], keys: set[str]) -> Any:
    return first_value(report, keys)


def normalize_reason(report: dict[str, Any], trace_row: dict[str, Any] | None, inspected: dict[str, Any] | None) -> str:
    explicit = str(get_field(report, {"category", "halt_category"}) or "").lower()
    known = {
        "unmapped_mmio_read",
        "unmapped_mmio_write",
        "illegal_insn",
        "decode_trap",
        "unhandled_sysreg",
        "spin_detected",
        "exception_return",
        "step_limit",
    }
    if explicit in known:
        return explicit
    reason = str(report.get("reason") or get_field(report, {"halt_reason"}) or "").lower()
    text = f"{explicit} {reason}"
    if "steplimit" in text.replace("_", "").replace(" ", "") or "step limit" in text:
        return "step_limit"
    if "eret" in text and "exception return" in text:
        return "exception_return"
    if "spin_detected" in text or "infinite spin" in text or "true infinite spin" in text:
        return "spin_detected"
    if "unhandled_sysreg" in text or "unhandled sysreg" in text or "unknown sysreg" in text:
        return "unhandled_sysreg"
    if "illegalinstruction" in text or "illegal_insn" in text:
        return "illegal_insn"
    if "decode_trap" in text or "decode trap" in text:
        return "decode_trap"
    if "mem_load fault" in text and inspected and inspected.get("is_load") is True:
        pa = number(inspected.get("physical_address"))
        if pa is not None and is_mmio_address(pa):
            return "unmapped_mmio_read"
    if "mem_store fault" in text and inspected and inspected.get("is_load") is False:
        pa = number(inspected.get("physical_address"))
        if pa is not None and is_mmio_address(pa):
            return "unmapped_mmio_write"
    # Older native builds collapse host-side MMIO failures into a generic
    # WASM trap. The captured faulting instruction and translated PA provide
    # the read/write distinction without changing the emulator.
    if "wasmtrap" in text and inspected:
        pa = number(inspected.get("physical_address"))
        if pa is not None and is_mmio_address(pa):
            return "unmapped_mmio_read" if inspected.get("is_load") else "unmapped_mmio_write"
    if "unsupported" in text and trace_row and trace_row.get("word") is not None:
        return "decode_trap"
    return "unknown"


def is_mmio_address(pa: int) -> bool:
    return 0x0800_0000 <= pa < 0x1000_0000


def decode_sysreg(word: int) -> str | None:
    if (word & 0xFFE0_0000) != 0xD520_0000:
        return None
    op0 = (word >> 19) & 0x3
    op1 = (word >> 16) & 0x7
    crn = (word >> 12) & 0xF
    crm = (word >> 8) & 0xF
    op2 = (word >> 5) & 0x7
    return f"S{op0}_{op1}_C{crn}_C{crm}_{op2}"


def instruction_class(word: int | None) -> str:
    if word is None:
        return "unknown"
    if word & 0x7C00_0000 == 0x1400_0000 or word & 0xFF00_0010 == 0x5400_0000:
        return "branch"
    if word & 0x7E00_0000 in (0x3400_0000, 0x3600_0000):
        return "branch"
    if decode_sysreg(word):
        return "sysreg"
    if word & 0x1F00_0000 == 0x0A00_0000:
        return "dp_reg"
    if word & 0x1F00_0000 in (0x1100_0000, 0x1200_0000):
        return "dp_imm"
    if word & 0x3B00_0000 in (0x3900_0000, 0x3800_0000):
        return "load_store"
    if word & 0x3F00_0000 in (0x0800_0000, 0x1800_0000):
        return "atomic_or_literal_load"
    if word & 0x1E00_0000 == 0x0E00_0000:
        return "simd_fp"
    return "other"


def mmio_identity(pa: int | None) -> tuple[str, str]:
    if pa is None:
        return "mmio:unknown", "register:unknown"
    regions = (
        (0x0800_0000, 0x0801_0000, "gic_dist"),
        (0x0801_0000, 0x0802_0000, "gic_cpu"),
        (0x080A_0000, 0x080C_0000, "gic_redist"),
        (0x0900_0000, 0x0901_0000, "uart"),
        (0x0A00_0000, 0x0B00_0000, "virtio"),
    )
    for base, end, family in regions:
        if base <= pa < end:
            return f"mmio:{family}", f"register:{pa - base:#x}"
    return f"mmio:0x{pa & ~0xffff:x}", f"register:{pa & 0xffff:#x}"


def signature_for(category: str, word: int | None, report: dict[str, Any], inspected: dict[str, Any] | None) -> tuple[str, str]:
    if category == "exception_return":
        return "insn:eret", "instruction:ERET"
    if category.startswith("unmapped_mmio"):
        pa = number(inspected.get("physical_address")) if inspected else None
        family, register = mmio_identity(pa)
        return f"{family}:{register}", register
    if category == "unhandled_sysreg":
        selector = decode_sysreg(word) if word is not None else None
        return (f"sysreg:{selector or 'unknown'}", selector or "selector:unknown")
    if category in {"illegal_insn", "decode_trap"}:
        cls = instruction_class(word)
        return (f"insn:{cls}", f"class:{cls}")
    if category == "spin_detected":
        symbol = str(report.get("symbol") or "unknown")
        return (f"spin:{symbol.split('+', 1)[0]}", f"symbol:{symbol}")
    return (f"category:{category}", f"reason:{category}")


def independence(signature: str, category: str, prior: list[dict[str, Any]]) -> tuple[bool, str]:
    for earlier in prior:
        if earlier["signature"] == signature:
            return False, f"same {earlier['identity']} as earlier halt at {earlier['pc']}"
        if category.startswith("unmapped_mmio") and earlier["category"].startswith("unmapped_mmio"):
            if signature.startswith("mmio:") and earlier["signature"].startswith("mmio:"):
                if signature.split(":")[1] == earlier["signature"].split(":")[1]:
                    return False, f"same MMIO device family ({signature.split(':')[1]})"
    return True, "different device family, sysreg selector, or instruction class from prior halts"


def parse_qemu_events(path: Path) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    if not path.is_file():
        raise RuntimeError(f"QEMU MMIO oracle trace is missing: {path}")
    pattern = re.compile(
        r"\b(gic_(dist|cpu|redist|its)_(read|write))\b.*?\bat\s+(0x[0-9a-fA-F]+)(?:\s+size\s+(\d+))?\s*:\s*(0x[0-9a-fA-F]+)"
    )
    for line_number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        match = pattern.search(line)
        if not match:
            continue
        events.append(
            {
                "line": line_number,
                "device": match.group(2),
                "operation": match.group(3),
                "offset": int(match.group(4), 16),
                "size": int(match.group(5) or 4),
                "value": int(match.group(6), 16),
                "raw": line.strip(),
            }
        )
    if not events:
        raise RuntimeError(f"no GIC MMIO events parsed from {path}")
    return events


def oracle_offset(pa: int) -> tuple[str | None, int]:
    regions = (
        (0x0800_0000, 0x0801_0000, "dist"),
        (0x0801_0000, 0x0802_0000, "cpu"),
        (0x080A_0000, 0x080C_0000, "redist"),
        (0x080C_0000, 0x0810_0000, "its"),
    )
    for base, end, device in regions:
        if base <= pa < end:
            return device, pa - base
    return None, pa & 0xffff


def find_oracle_event(
    events: list[dict[str, Any]], cursor: int, pa: int, size: int, operation: str
) -> tuple[dict[str, Any] | None, int, int | None]:
    device, offset = oracle_offset(pa)
    for index in range(cursor, len(events)):
        event = events[index]
        if event["operation"] != operation or event["offset"] != offset or event["size"] != size:
            continue
        if device is not None and event["device"] != device:
            continue
        return event, index + 1, index
    return None, cursor, None


def inspect_snapshot(helper: Path, snapshot: Path, pc: int, word: int) -> dict[str, Any]:
    result = run([str(helper), "inspect", str(snapshot), "--pc", hex(pc), "--word", hex(word)])
    if result.returncode:
        raise RuntimeError(f"snapshot inspection failed: {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"snapshot inspector returned invalid JSON: {result.stdout!r}") from error


def inspect_machine(helper: Path, snapshot: Path) -> dict[str, Any]:
    result = run([str(helper), "inspect", str(snapshot)])
    if result.returncode:
        raise RuntimeError(f"machine snapshot inspection failed: {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"snapshot inspector returned invalid JSON: {result.stdout!r}") from error


def apply_skip(
    helper: Path,
    source: Path,
    destination: Path,
    method: str,
    pc: int,
    word: int,
    value: int | None = None,
    patch_pc: int | None = None,
) -> dict[str, Any]:
    command = [str(helper), "apply", str(source), "--output", str(destination), "--method", method, "--pc", hex(pc), "--word", hex(word)]
    if value is not None:
        command.extend(["--value", hex(value)])
    if patch_pc is not None:
        command.extend(["--patch-pc", hex(patch_pc)])
    result = run(command)
    if result.returncode:
        raise RuntimeError(f"skip helper failed ({method}): {result.stderr.strip()}")
    return json.loads(result.stdout)


def apply_irq_entry_skip(
    helper: Path,
    source: Path,
    destination: Path,
    state: dict[str, Any],
    intid: int,
    skip_log: Path,
    label: str,
) -> dict[str, Any]:
    pending = state.get("pending_irqs")
    pstate = number(state.get("pstate"))
    pc = number(state.get("live_pc"))
    word = number(state.get("word"))
    if not isinstance(pending, list) or intid not in pending:
        raise RuntimeError(f"IRQ pause reported without pending INTID {intid}: {pending!r}")
    if pstate is None or pstate & 0x80:
        raise RuntimeError(f"IRQ pause reported while IRQ is masked: PSTATE={state.get('pstate')!r}")
    if pc is None or word is None:
        raise RuntimeError("IRQ pause snapshot is missing current PC or instruction word")
    mutation = apply_skip(helper, source, destination, "irq-vector", pc, word)
    vector = number(mutation.get("vector_pc"))
    vector_text = f"0x{vector:016x}" if vector is not None else "unknown"
    event = {
        "source": label,
        "halt_step": number(state.get("snapshot_step")),
        "pc": f"0x{pc:016x}",
        "reason": f"pending unmasked timer IRQ INTID {intid}; exception entry is not modeled",
        "category": "irq_exception_entry_gap",
        "skip_method": f"manual_VBAR_EL1_plus_0x280:{vector_text}",
        "imprecise": True,
        "details": {
            "pending_irqs": pending,
            "pstate_before": state.get("pstate"),
            "vbar_el1": state.get("vbar_el1"),
            "mutation": mutation,
        },
    }
    append_skip(skip_log, event)
    say(f"IMPRECISE SKIP [{label}]: timer INTID {intid} -> EL1h IRQ vector {event['skip_method']}")
    return mutation


def spin_backedge(trace_path: Path) -> tuple[int | None, str | None, str | None]:
    try:
        from scripts.spin_detector import Detector, branch_info, loop_status
    except ImportError:
        return None, None, "spin detector unavailable"
    detector = Detector(max_period=256)
    try:
        with trace_path.open("r", encoding="utf-8", errors="replace") as trace:
            detector.process(trace)
    except OSError as error:
        return None, None, str(error)
    candidates = []
    for loop in detector.loops.values():
        status, _ = loop_status(loop)
        if "TRUE INFINITE SPIN" not in status:
            continue
        loop_pcs = set(loop.pcs)
        branches = []
        for instruction in loop.pattern:
            result = branch_info(instruction.word, instruction.pc, instruction.regs)
            if result and result[1] in loop_pcs and result[1] <= instruction.pc:
                branches.append(instruction.pc)
        if branches:
            candidates.append((loop.iterations, max(branches), loop))
    if not candidates:
        return None, None, "no confirmed backward loop branch in trace window"
    _, branch_pc, loop = max(candidates, key=lambda entry: entry[0])
    symbol = f"loop@0x{min(loop.pcs):016x}"
    status, _ = loop_status(loop)
    return branch_pc, symbol, status


def run_iteration(
    binary: Path,
    current_snapshot: Path,
    run_dir: Path,
    report_dir: Path,
    label: str,
    help_text: str,
    max_steps: int,
    helper: Path | None = None,
    skip_log: Path | None = None,
    irq_intid: int | None = None,
    keep_traces: bool = False,
) -> tuple[dict[str, Any], Path, Path, Path]:
    run_dir.mkdir(parents=True, exist_ok=True)
    json_supported = "--boot-report-json" in help_text
    remaining = max_steps
    segment = 0
    if irq_intid is not None:
        if helper is None or skip_log is None:
            raise RuntimeError("forced IRQ entry requires the snapshot helper and skip audit log")
        initial = inspect_machine(helper, current_snapshot)
        last_step = number(initial.get("snapshot_step"))
        if last_step is None:
            raise RuntimeError("starting snapshot has no step count")
    else:
        last_step = None

    while True:
        segment_label = label if segment == 0 else f"{label}.irq-{segment:02d}"
        halted_snapshot = run_dir / f"{segment_label}.halt.snapshot"
        report_path = report_dir / f"{segment_label}.boot-report.json"
        trace_path = run_dir / f"{segment_label}.trace"
        trace_output = trace_path if keep_traces else Path(os.devnull)
        uart_path = run_dir / f"{segment_label}.uart"
        report_path.unlink(missing_ok=True)
        command = [
            str(binary),
            "--load-snapshot",
            str(current_snapshot),
            "--max-steps",
            str(remaining),
            "--trace",
            str(trace_output),
            "--save-snapshot",
            str(halted_snapshot),
            "--console-output",
            str(uart_path),
            "--pause-on-init-marker",
            "--skip-ram-search",
        ]
        if irq_intid is not None:
            command.extend(["--pause-on-pending-irq", str(irq_intid)])
        if json_supported:
            command.extend(["--boot-report-json", str(report_path)])
        result = run(command)
        if result.returncode:
            raise RuntimeError(f"{segment_label} run failed:\n{result.stdout}\n{result.stderr}")
        trace_row = parse_trace_row(trace_output)
        report = load_report(report_path, result.stdout, trace_row)
        report.setdefault("_scout_boot_report_supported", json_supported)
        report["_scout_runner_stdout"] = result.stdout
        if uart_path.is_file():
            uart = uart_path.read_bytes()
            report["uart_bytes"] = len(uart)
            # Use the exact execution line, not rdinit=/init in the command echo.
            report["uart_init_marker"] = b"Run /init as init process" in uart
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if not halted_snapshot.is_file():
            raise RuntimeError(f"{segment_label} runner did not save a halt snapshot")

        if irq_intid is not None and "[irq-pause]" in result.stdout:
            state = inspect_machine(helper, halted_snapshot)
            step = number(state.get("snapshot_step"))
            if step is None or last_step is None:
                raise RuntimeError("IRQ pause snapshot is missing its step counter")
            remaining -= max(0, step - last_step)
            next_snapshot = run_dir / f"{segment_label}.irq-entry.snapshot"
            apply_irq_entry_skip(helper, halted_snapshot, next_snapshot, state, irq_intid, skip_log, segment_label)
            previous_snapshot = current_snapshot
            current_snapshot = next_snapshot
            # A completed skip has produced the replacement full-state snapshot.
            # Drop its source and the runner's pause snapshot to keep the 1.1 GB
            # PNFS snapshots from accumulating at every IRQ boundary.
            if previous_snapshot.is_relative_to(run_dir):
                previous_snapshot.unlink(missing_ok=True)
            halted_snapshot.unlink(missing_ok=True)
            last_step = step
            segment += 1
            if keep_traces is False:
                trace_path.unlink(missing_ok=True)
                uart_path.unlink(missing_ok=True)
            if remaining <= 0:
                report["reason"] = None
                report["halt_step"] = step
                report["pc"] = number(state.get("live_pc"))
                report["_scout_runner_stdout"] += "\n[limit] step budget exhausted after imprecise IRQ entry"
                return report, next_snapshot, trace_output, report_path
            continue
        if current_snapshot.is_relative_to(run_dir):
            current_snapshot.unlink(missing_ok=True)
        return report, halted_snapshot, trace_output, report_path


def is_init_marker(report: dict[str, Any]) -> bool:
    marker = report.get("uart_init_marker")
    if isinstance(marker, bool):
        return marker
    uart = report.get("uart")
    if isinstance(uart, dict) and isinstance(uart.get("init_marker"), bool):
        return uart["init_marker"]
    return False


def make_record(
    report: dict[str, Any], trace_row: dict[str, Any] | None, category: str, inspected: dict[str, Any] | None,
    skip_method: str, prior: list[dict[str, Any]], fallback_symbol: str,
) -> tuple[dict[str, Any], dict[str, Any]]:
    step = number(report.get("halt_step"))
    if step is None:
        step = number(report.get("trace_step"))
    if step is None and trace_row:
        step = trace_row["step"]
    pc = number(report.get("pc"))
    if pc is None:
        pc = number(report.get("trace_pc"))
    if pc is None and trace_row:
        pc = trace_row["pc"]
    word = number(report.get("trace_word"))
    if word is None and trace_row:
        word = trace_row["word"]
    symbol = str(report.get("symbol") or fallback_symbol or "unknown")
    agent_kind = str(report.get("agent_kind") or {
        "unmapped_mmio_read": "mmio",
        "unmapped_mmio_write": "mmio",
        "illegal_insn": "instruction-decoder",
        "decode_trap": "instruction-decoder",
        "unhandled_sysreg": "sysreg",
        "exception_return": "cpu-exception",
        "spin_detected": "liveness",
    }.get(category, "triage"))
    signature, identity = signature_for(category, word, report, inspected)
    is_independent, explanation = independence(signature, category, prior)
    original_reason = str(report.get("reason") or category)
    record = {
        "halt_step": step,
        "pc": f"0x{pc:016x}" if pc is not None else "unknown",
        "symbol": symbol,
        "reason": original_reason,
        "category": category,
        "agent_kind": agent_kind,
        "skip_method": skip_method,
        "imprecise": True,
        "independent_of_earlier": is_independent,
        "independence_reason": explanation,
    }
    identity_record = {
        "signature": signature,
        "identity": identity,
        "category": category,
        "pc": record["pc"],
    }
    return record, identity_record


def write_queue(path: Path, queue: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(queue, indent=2) + "\n", encoding="utf-8")


def append_skip(path: Path, event: dict[str, Any]) -> None:
    with path.open("a", encoding="utf-8") as log:
        log.write(json.dumps(event, sort_keys=True) + "\n")


def prepare_start_snapshot(args: argparse.Namespace, binary: Path, help_text: str, work_dir: Path) -> Path:
    if args.snapshot:
        snapshot = Path(args.snapshot).expanduser().resolve()
        if not snapshot.is_file():
            raise RuntimeError(f"input snapshot does not exist: {snapshot}")
        return snapshot
    if args.snapshot_at_step is None:
        raise RuntimeError("provide --snapshot or --snapshot-at-step")
    if "--save-snapshot" not in help_text:
        raise RuntimeError("runner does not support --save-snapshot; cannot make a reset-based fallback snapshot")
    snapshot = work_dir / "frontier-start.snapshot"
    command = [str(binary)]
    kernel = args.kernel or os.environ.get("BOOT_REPORT_KERNEL") or "/Users/Shared/guest-inputs/Image"
    initrd = args.initrd or os.environ.get("BOOT_REPORT_INITRD") or "/Users/Shared/guest-inputs/initramfs.cpio"
    dtb = args.dtb or os.environ.get("BOOT_REPORT_DTB") or "/Users/Shared/guest-inputs/minimal-virt-fixed.dtb"
    for option, value in (("--kernel", kernel), ("--initrd", initrd), ("--dtb", dtb)):
        if value and Path(value).is_file():
            command.extend([option, str(Path(value).resolve())])
        elif value:
            raise RuntimeError(f"reset fallback input is missing for {option}: {value}")
    command.extend(["--max-steps", str(args.snapshot_at_step + 1), "--save-snapshot", str(snapshot)])
    snapshot_flag = "--snapshot-at-step" if "--snapshot-at-step" in help_text else "--save-at"
    command.extend([snapshot_flag, str(args.snapshot_at_step)])
    say(f"[scout] creating fallback snapshot from reset at step {args.snapshot_at_step} (using {snapshot_flag})")
    result = run(command)
    if result.returncode:
        raise RuntimeError(f"reset-based snapshot run failed:\n{result.stdout}\n{result.stderr}")
    if not snapshot.is_file():
        raise RuntimeError(f"reset-based run did not produce {snapshot}")
    saved_at = re.search(r"\[snapshot\] Saved \d+ bytes to .+ at step (\d+),", result.stdout)
    if not saved_at:
        raise RuntimeError("reset-based run did not report the full-state snapshot step")
    saved_step = int(saved_at.group(1))
    if saved_step != args.snapshot_at_step:
        raise RuntimeError(
            f"reset-based run stopped at step {saved_step}, before requested snapshot step {args.snapshot_at_step}"
        )
    return snapshot


def plan_and_apply_skip(
    category: str,
    report: dict[str, Any],
    trace_row: dict[str, Any] | None,
    inspected: dict[str, Any] | None,
    helper: Path,
    source: Path,
    destination: Path,
    trace_path: Path,
    oracle_events: list[dict[str, Any]],
    oracle_cursor: int,
) -> tuple[str, dict[str, Any], int, dict[str, Any]]:
    if trace_row is None:
        raise RuntimeError("cannot select a skip without a faulting instruction trace row")
    pc, word = trace_row["pc"], trace_row["word"]
    details: dict[str, Any] = {"pc": f"0x{pc:016x}", "word": f"0x{word:08x}"}
    if category == "unmapped_mmio_read":
        if not inspected or inspected.get("is_load") is not True:
            raise RuntimeError("MMIO read halt did not decode to a load")
        pa = number(inspected.get("physical_address"))
        size = number(inspected.get("access_size"))
        if pa is None or size is None:
            raise RuntimeError("MMIO read address/size is unavailable from snapshot state")
        event, new_cursor, event_index = find_oracle_event(oracle_events, oracle_cursor, pa, size, "read")
        if event is None:
            raise RuntimeError(f"no QEMU read oracle match for PA {pa:#x} size {size} after event {oracle_cursor}")
        mutation = apply_skip(helper, source, destination, "mmio-read", pc, word, value=event["value"])
        method = f"return_qemu_value:{event['value']:#x}@{event['device']}+{event['offset']:#x}"
        details.update({"physical_address": f"0x{pa:016x}", "size": size, "oracle": event, "oracle_index": event_index, "substituted_value": f"0x{event['value']:x}"})
        return method, mutation, new_cursor, details
    if category == "unmapped_mmio_write":
        if not inspected or inspected.get("is_load") is not False:
            raise RuntimeError("MMIO write halt did not decode to a store")
        pa = number(inspected.get("physical_address"))
        size = number(inspected.get("access_size"))
        if pa is None or size is None:
            raise RuntimeError("MMIO write address/size is unavailable from snapshot state")
        event, new_cursor, event_index = find_oracle_event(oracle_events, oracle_cursor, pa, size, "write")
        mutation = apply_skip(helper, source, destination, "advance", pc, word)
        method = "drop_write_and_advance_pc"
        details.update({"physical_address": f"0x{pa:016x}", "size": size, "oracle_event": event, "oracle_index": event_index})
        if event is None:
            # A write is intentionally dropped even when the reference trace does not contain it;
            # retain the cursor so later reads can still seek a matching QEMU event.
            new_cursor = oracle_cursor
        return method, mutation, new_cursor, details
    if category in {"illegal_insn", "decode_trap"}:
        mutation = apply_skip(helper, source, destination, "nop", pc, word)
        method = "replace_with_nop"
        details["nop_word"] = "0xd503201f"
        return method, mutation, oracle_cursor, details
    if category == "unhandled_sysreg":
        selector = decode_sysreg(word)
        if selector is None:
            raise RuntimeError(f"unhandled sysreg halt does not carry an MRS instruction (word={word:#010x})")
        mutation = apply_skip(helper, source, destination, "sysreg-zero", pc, word)
        method = f"return_zero:{selector}"
        details["sysreg"] = selector
        return method, mutation, oracle_cursor, details
    if category == "exception_return":
        mutation = apply_skip(helper, source, destination, "eret-return", pc, word)
        method = "restore_saved_ELR_EL1_SPSR_EL1_and_return"
        details.update(
            {
                "restored_pc": mutation.get("restored_pc"),
                "restored_pstate": mutation.get("restored_pstate"),
            }
        )
        return method, mutation, oracle_cursor, details
    if category == "spin_detected":
        patch_pc, spin_symbol, status = spin_backedge(trace_path)
        if patch_pc is None:
            raise RuntimeError(f"cannot break detected spin: {status}")
        mutation = apply_skip(helper, source, destination, "spin-break", pc, word, patch_pc=patch_pc)
        method = f"nop_backedge_once:{spin_symbol}@0x{patch_pc:016x}"
        details.update({"spin_symbol": spin_symbol, "spin_status": status, "patched_branch_pc": f"0x{patch_pc:016x}", "nop_word": "0xd503201f"})
        say(f"[scout] spin loop break once: {spin_symbol}; patched back-edge at 0x{patch_pc:016x}")
        return method, mutation, oracle_cursor, details
    raise RuntimeError(f"no authorized imprecise skip rule for category {category!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", help="T69 snapshot at or just before the current frontier halt")
    parser.add_argument("--snapshot-at-step", type=int, help="reset-based fallback: create a snapshot at this step")
    parser.add_argument("--iterations", type=int, default=20, help="subsequent halts to queue (default: 20)")
    parser.add_argument("--output", default="scout-queue.json", help="ordered queue JSON output path")
    parser.add_argument("--mmio-trace", default=str(DEFAULT_MMIO_TRACE), help="QEMU GIC MMIO oracle trace")
    parser.add_argument("--run-kernel", help="use an existing run_kernel binary instead of building this HEAD")
    parser.add_argument("--kernel", help="kernel path for reset-based snapshot creation")
    parser.add_argument("--initrd", help="initramfs path for reset-based snapshot creation")
    parser.add_argument("--dtb", help="DTB path for reset-based snapshot creation")
    parser.add_argument("--max-steps", type=int, default=200_000, help="step budget per halt search")
    parser.add_argument("--step-limit-continuations", type=int, default=3, help="extra step-budget segments before stopping without a halt")
    parser.add_argument("--force-irq-intid", type=int, help="discovery-only manual vector entry for an unmasked pending INTID")
    parser.add_argument("--symbol-map", help="ELF/System.map used to symbolize fallback PCs")
    parser.add_argument("--keep-traces", action="store_true", help="retain each per-iteration instruction trace")
    parser.add_argument("--frontier-snapshot", help="persist the latest step-limit checkpoint for a later scout run")
    args = parser.parse_args()

    if args.iterations < 1 or args.max_steps < 1 or args.step_limit_continuations < 0:
        parser.error("--iterations and --max-steps must be positive; --step-limit-continuations cannot be negative")
    if args.force_irq_intid is not None and args.force_irq_intid != 27:
        parser.error("this EL1h vector skip currently supports only timer INTID 27")
    output = Path(args.output).expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    frontier_snapshot = (
        Path(args.frontier_snapshot).expanduser().resolve()
        if args.frontier_snapshot
        else output.parent / f"{output.stem}-frontier.snapshot"
    )
    report_dir = output.parent / f"{output.stem}-reports"
    report_dir.mkdir(parents=True, exist_ok=True)
    skip_log = output.parent / f"{output.stem}-skip-log.jsonl"
    skip_log.write_text("", encoding="utf-8")
    queue: list[dict[str, Any]] = []
    write_queue(output, queue)

    try:
        binary = build_runner(args.run_kernel)
        help_text = parse_help(binary)
        with tempfile.TemporaryDirectory(prefix="frontier-scout-") as temporary:
            work_dir = Path(temporary)
            helper = build_skipper(ROOT / "target")
            start_snapshot = prepare_start_snapshot(args, binary, help_text, work_dir)
            oracle_events = parse_qemu_events(Path(args.mmio_trace).expanduser().resolve())
            oracle_cursor = 0
            current_snapshot = start_snapshot
            # Only temporary snapshots created by this invocation may be deleted.
            owned_snapshots: set[Path] = set()
            prior: list[dict[str, Any]] = []
            symbol_map = Path(args.symbol_map or os.environ.get("BOOT_REPORT_SYMBOLS", "")).expanduser() if (args.symbol_map or os.environ.get("BOOT_REPORT_SYMBOLS")) else None

            seed_continuations = 0
            seed_label = "seed"
            while True:
                seed_report, seed_halted, seed_trace, _seed_report_path = run_iteration(
                    binary, current_snapshot, work_dir, report_dir, seed_label, help_text, args.max_steps,
                    helper, skip_log, args.force_irq_intid, args.keep_traces,
                )
                seed_row = parse_trace_row(seed_trace)
                if seed_row is None:
                    seed_row = trace_row_from_snapshot(helper, seed_halted, number(seed_report.get("halt_step")))
                seed_inspected = inspect_snapshot(helper, seed_halted, seed_row["pc"], seed_row["word"]) if seed_row else None
                seed_category = normalize_reason(seed_report, seed_row, seed_inspected)
                if seed_category == "unknown" and not seed_report.get("reason") and "[limit]" in str(seed_report.get("_scout_runner_stdout", "")):
                    seed_category = "step_limit"
                if seed_category != "step_limit" or is_init_marker(seed_report):
                    break

                frontier_snapshot.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(seed_halted, frontier_snapshot)
                if seed_continuations >= args.step_limit_continuations:
                    terminal_pc = number(seed_report.get("pc")) or (seed_row["pc"] if seed_row else None)
                    say(
                        f"[scout] seed reached the runner step limit after {seed_continuations} continuations; "
                        f"checkpoint={frontier_snapshot}; no subsequent halts queued"
                    )
                    write_queue(output, queue)
                    return 0

                state = inspect_machine(helper, seed_halted)
                pc, word = number(state.get("live_pc")), number(state.get("word"))
                if pc is None or word is None:
                    raise RuntimeError(f"{seed_label} step-limit snapshot is missing PC or instruction word")
                resumed_snapshot = work_dir / f"seed.continuation-{seed_continuations + 1:02d}.snapshot"
                mutation = apply_skip(helper, seed_halted, resumed_snapshot, "resume-step-limit", pc, word)
                append_skip(
                    skip_log,
                    {
                        "source": seed_label,
                        "halt_step": number(state.get("snapshot_step")),
                        "pc": f"0x{pc:016x}",
                        "reason": "runner instruction budget exhausted; resuming the same machine state",
                        "category": "step_budget_continuation",
                        "skip_method": "clear_runner_step_limit_latch_and_continue",
                        "imprecise": True,
                        "details": {"mutation": mutation},
                    },
                )
                say(f"IMPRECISE SKIP [{seed_label}]: clear runner step-limit latch and continue")
                seed_halted.unlink(missing_ok=True)
                if current_snapshot != start_snapshot and current_snapshot in owned_snapshots:
                    current_snapshot.unlink(missing_ok=True)
                owned_snapshots.add(resumed_snapshot)
                current_snapshot = resumed_snapshot
                seed_continuations += 1
                seed_label = f"seed.continuation-{seed_continuations:02d}"

            seed_pc = number(seed_report.get("pc")) or (seed_row["pc"] if seed_row else None)
            if seed_category == "unknown" and not seed_report.get("reason"):
                seed_category = "step_limit"
            seed_symbol = str(seed_report.get("symbol") or symbol_for_pc(seed_pc, symbol_map))
            seed_record, seed_identity = make_record(
                seed_report,
                seed_row,
                seed_category,
                seed_inspected,
                "seed_frontier_skip",
                [],
                seed_symbol,
            )
            say(
                f"[scout] seed frontier: step={seed_record['halt_step']} pc={seed_record['pc']} "
                f"reason={seed_category} symbol={seed_symbol} source={seed_report.get('source', 'boot-report')}"
            )
            if is_init_marker(seed_report):
                say("*** UART INIT MARKER TRUE at the starting frontier; no subsequent halts queued. ***")
                write_queue(output, queue)
                return 0
            if seed_category in {"unknown", "step_limit"}:
                raise RuntimeError(f"could not classify the starting frontier halt: {seed_report.get('reason')}")

            seed_next = work_dir / "seed.next.snapshot"
            seed_method, seed_mutation, oracle_cursor, seed_details = plan_and_apply_skip(
                seed_category,
                seed_report,
                seed_row,
                seed_inspected,
                helper,
                seed_halted,
                seed_next,
                seed_trace,
                oracle_events,
                oracle_cursor,
            )
            seed_identity["pc"] = seed_record["pc"]
            prior.append(seed_identity)
            seed_skip = {
                "source": "frontier_seed",
                "halt_step": seed_record["halt_step"],
                "pc": seed_record["pc"],
                "reason": seed_record["reason"],
                "category": seed_category,
                "skip_method": seed_method,
                "imprecise": True,
                "details": seed_details,
                "mutation": seed_mutation,
            }
            append_skip(skip_log, seed_skip)
            say(f"IMPRECISE SKIP [seed]: {seed_method}")
            seed_is_queued = args.force_irq_intid is not None
            if seed_is_queued:
                seed_record["skip_method"] = seed_method
                queue.append(seed_record)
                write_queue(output, queue)
                say(
                    f"[scout] queued 1/{args.iterations}: step={seed_record['halt_step']} "
                    f"pc={seed_record['pc']} reason={seed_category} symbol={seed_symbol} "
                    f"independent={seed_record['independent_of_earlier']}"
                )
            seed_halted.unlink(missing_ok=True)
            owned_snapshots.add(seed_halted)
            owned_snapshots.add(seed_next)
            if current_snapshot != start_snapshot and current_snapshot in owned_snapshots:
                current_snapshot.unlink(missing_ok=True)
            current_snapshot = seed_next
            if not args.keep_traces and seed_trace != Path(os.devnull):
                seed_trace.unlink(missing_ok=True)

            for index in range(1 + int(seed_is_queued), args.iterations + 1):
                label = f"halt-{index:02d}"
                # QEMU shows repeated timer PPI assertion/deassertion cycles.
                # Pause and manually vector each unmasked INTID 27 boundary;
                # every such imprecise entry is recorded in the skip audit.
                continuations = 0
                run_label = label
                while True:
                    report, halted_snapshot, trace_path, _report_path = run_iteration(
                        binary, current_snapshot, work_dir, report_dir, run_label, help_text, args.max_steps,
                        helper, skip_log, args.force_irq_intid, args.keep_traces,
                    )
                    trace_row = parse_trace_row(trace_path)
                    if trace_row is None:
                        trace_row = trace_row_from_snapshot(helper, halted_snapshot, number(report.get("halt_step")))
                    checkpoint_category = normalize_reason(report, trace_row, None)
                    if (
                        checkpoint_category == "unknown"
                        and report.get("reason") is None
                        and "[limit]" in str(report.get("_scout_runner_stdout", ""))
                    ):
                        checkpoint_category = "step_limit"
                    if is_init_marker(report) or checkpoint_category != "step_limit":
                        break
                    frontier_snapshot.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(halted_snapshot, frontier_snapshot)
                    if continuations >= args.step_limit_continuations:
                        break
                    state = inspect_machine(helper, halted_snapshot)
                    pc = number(state.get("live_pc"))
                    word = number(state.get("word"))
                    if pc is None or word is None:
                        raise RuntimeError(f"{run_label} step-limit snapshot is missing PC or instruction word")
                    resumed_snapshot = work_dir / f"{label}.continuation-{continuations + 1:02d}.snapshot"
                    mutation = apply_skip(helper, halted_snapshot, resumed_snapshot, "resume-step-limit", pc, word)
                    append_skip(
                        skip_log,
                        {
                            "source": run_label,
                            "halt_step": number(state.get("snapshot_step")),
                            "pc": f"0x{pc:016x}",
                            "reason": "runner instruction budget exhausted; resuming the same machine state",
                            "category": "step_budget_continuation",
                            "skip_method": "clear_runner_step_limit_latch_and_continue",
                            "imprecise": True,
                            "details": {"mutation": mutation},
                        },
                    )
                    say(f"IMPRECISE SKIP [{run_label}]: clear runner step-limit latch and continue")
                    halted_snapshot.unlink(missing_ok=True)
                    current_snapshot = resumed_snapshot
                    continuations += 1
                    run_label = f"{label}.continuation-{continuations:02d}"
                if report.get("reason") is None and "[limit]" in str(report.get("_scout_runner_stdout", "")):
                    branch_pc, spin_symbol, spin_status = spin_backedge(trace_path)
                    if branch_pc is not None:
                        report["reason"] = "spin_detected: " + (spin_status or "confirmed loop")
                        report["category"] = "spin_detected"
                        report["symbol"] = spin_symbol
                        report["pc"] = trace_row["pc"] if trace_row else None
                halt_pc = number(report.get("pc")) or (trace_row["pc"] if trace_row else None)
                inspected = inspect_snapshot(helper, halted_snapshot, trace_row["pc"], trace_row["word"]) if trace_row else None
                category = normalize_reason(report, trace_row, inspected)
                if category == "unknown" and report.get("reason") is None:
                    category = "step_limit"
                if not report.get("symbol"):
                    report["symbol"] = symbol_for_pc(halt_pc, symbol_map)

                if is_init_marker(report):
                    say(f"*** UART INIT MARKER TRUE after {index - 1} queued halts; stopped at {report.get('symbol', 'unknown')} ({report.get('pc', 'unknown')}). ***")
                    append_skip(
                        skip_log,
                        {
                            "source": label,
                            "uart_init_marker": True,
                            "halt_step": report.get("halt_step"),
                            "pc": report.get("pc"),
                            "reason": report.get("reason"),
                            "skip_method": "none_init_marker_stop",
                            "imprecise": True,
                        },
                    )
                    break

                if category == "step_limit":
                    write_queue(output, queue)
                    terminal_pc = f"0x{halt_pc:016x}" if halt_pc is not None else "unknown"
                    say(f"[scout] reached the runner step limit after {continuations} continuations at {terminal_pc}; checkpoint={frontier_snapshot}; {len(queue)} halts queued")
                    break
                if category == "unknown":
                    write_queue(output, queue)
                    raise RuntimeError(f"{label} has unclassified halt reason {report.get('reason')!r}")
                if trace_row is None:
                    raise RuntimeError(f"{label} did not produce a faulting instruction trace row")

                # Find the skip plan before adding the row; the queue identifies exactly how
                # that halt was forced past for the next frontier.
                next_snapshot = work_dir / f"{label}.next.snapshot"
                method, mutation, new_cursor, details = plan_and_apply_skip(
                    category,
                    report,
                    trace_row,
                    inspected,
                    helper,
                    halted_snapshot,
                    next_snapshot,
                    trace_path,
                    oracle_events,
                    oracle_cursor,
                )
                fallback_symbol = symbol_for_pc(halt_pc, symbol_map)
                record, identity = make_record(
                    report,
                    trace_row,
                    category,
                    inspected,
                    method,
                    prior,
                    fallback_symbol,
                )
                queue.append(record)
                write_queue(output, queue)
                identity["pc"] = record["pc"]
                prior.append(identity)
                skip_event = {
                    "source": label,
                    "halt_step": record["halt_step"],
                    "pc": record["pc"],
                    "reason": record["reason"],
                    "category": category,
                    "skip_method": method,
                    "imprecise": True,
                    "details": details,
                    "mutation": mutation,
                }
                append_skip(skip_log, skip_event)
                say(
                    f"[scout] queued {index}/{args.iterations}: step={record['halt_step']} "
                    f"pc={record['pc']} reason={category} symbol={record['symbol']} "
                    f"independent={record['independent_of_earlier']}"
                )
                say(f"IMPRECISE SKIP [{label}]: {method}")
                halted_snapshot.unlink(missing_ok=True)
                owned_snapshots.add(halted_snapshot)
                owned_snapshots.add(next_snapshot)
                if current_snapshot in owned_snapshots:
                    current_snapshot.unlink(missing_ok=True)
                current_snapshot = next_snapshot
                oracle_cursor = new_cursor
                if args.keep_traces is False and trace_path != Path(os.devnull):
                    trace_path.unlink(missing_ok=True)

            write_queue(output, queue)
            say(f"[scout] wrote {len(queue)} subsequent halts to {output}")
            say(f"[scout] skip audit: {skip_log}; boot reports: {report_dir}")
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        write_queue(output, queue)
        say(f"[scout] stopped: {error}")
        say(f"[scout] partial queue has {len(queue)} entries at {output}")
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
