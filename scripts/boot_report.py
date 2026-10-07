#!/usr/bin/env python3
"""Build/run the native ARM64 boot and emit human or machine-readable metrics."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

try:
    # Reuse the existing best-effort AArch64 disassembler from the spin tool.
    from spin_detector import disassemble
except ImportError:  # pragma: no cover - scripts/ is on sys.path when run directly
    disassemble = None


ROOT = Path(__file__).resolve().parents[1]
INIT_SUCCESS_MARKER = b"STATUS: PASS - USERSPACE INITIALIZATION COMPLETE"
ACCESS_RE = re.compile(
    r"^\[boot-report-access\] op=(read|write) pa=(0x[0-9a-fA-F]+) "
    r"width_bytes=(\d+) fault=(.*)$",
    re.MULTILINE,
)
MMU_ACCESS_RE = re.compile(
    r"^\[boot-report-access\] op=(read|write) va=(0x[0-9a-fA-F]+) fault=(.*)$",
    re.MULTILINE,
)


def run(
    command: list[str], *, env: dict[str, str], cwd: Path = ROOT
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, check=False)


def emit_failure(label: str, result: subprocess.CompletedProcess[str]) -> int:
    if result.stdout:
        print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
    if result.stderr:
        print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
    print(f"boot-report: {label} failed with exit code {result.returncode}", file=sys.stderr)
    return result.returncode or 1


def input_path(variable: str, default: str) -> Path:
    value = os.environ.get(variable, default)
    path = Path(value)
    if not path.is_absolute():
        path = ROOT / path
    if not path.is_file():
        raise FileNotFoundError(f"{variable} points to missing input: {path}")
    return path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def symbolize(pc: str) -> str:
    address = int(pc, 16)
    unknown = f"unknown+0x{address:x}"
    symbol_file = os.environ.get("BOOT_REPORT_SYMBOLS")
    if not symbol_file:
        return unknown
    path = Path(symbol_file)
    if not path.is_absolute():
        path = ROOT / path
    if not path.is_file():
        return unknown
    result = subprocess.run(["nm", "-n", str(path)], text=True, capture_output=True, check=False)
    if result.returncode:
        return unknown
    nearest: tuple[int, str] | None = None
    for line in result.stdout.splitlines():
        match = re.match(r"^\s*([0-9a-fA-F]+)\s+\S\s+(.+)$", line)
        if not match:
            continue
        start, name = int(match.group(1), 16), match.group(2).strip()
        if start <= address and (nearest is None or start > nearest[0]):
            nearest = (start, name)
    if nearest is None:
        return unknown
    return f"{nearest[1]}+0x{address - nearest[0]:x}"


def device_hint(pa: int) -> str:
    # QEMU virt GICv2 distributor window described by the guest DTB.
    if 0x08000000 <= pa < 0x08010000:
        return "gicd"
    if 0x08010000 <= pa < 0x08020000:
        return "gicc"
    return "unknown"


def parse_access(output: str) -> tuple[dict[str, Any] | None, str | None]:
    match = ACCESS_RE.search(output)
    if match:
        operation, raw_pa, raw_width, _fault = match.groups()
        pa = int(raw_pa, 16)
        hint = device_hint(pa)
        detail = {
            "pa": f"0x{pa:08x}",
            "width_bytes": int(raw_width),
            "is_write": operation == "write",
            "device_hint": hint,
        }
        reason = f"unmapped_mmio_{operation}" if hint != "unknown" else "data_abort"
        return detail, reason
    mmu_match = MMU_ACCESS_RE.search(output)
    if mmu_match:
        operation, raw_va, fault = mmu_match.groups()
        return {
            "va": f"0x{int(raw_va, 16):016x}",
            "is_write": operation == "write",
            "fault": fault,
        }, "data_abort"
    return None, None


def classification(reason: str, init_marker: bool) -> dict[str, str]:
    mapping = {
        "unmapped_mmio_read": ("unmapped_mmio", "high", "qemu_mmio_trace", "device"),
        "unmapped_mmio_write": ("unmapped_mmio", "high", "qemu_mmio_trace", "device"),
        "illegal_insn": ("unknown_insn", "high", "objdump_capstone", "decode"),
        "decode_trap": ("unknown_insn", "medium", "objdump_capstone", "decode"),
        "unhandled_sysreg": ("sysreg", "high", "qemu_sysreg_dump", "sysreg"),
        "data_abort": ("fault_divergence", "high", "qemu_divergence", "divergence"),
        "prefetch_abort": ("fault_divergence", "high", "qemu_divergence", "divergence"),
        "spin_detected": ("spin", "high", "symbolize_poll", "spin"),
    }
    if reason == "completed":
        # This is the userspace PASS marker, rather than the earlier cmdline echo.
        return {
            "category": "unknown",
            "confidence": "high" if init_marker else "low",
            "oracle": "uart.init_marker",
            "agent_kind": "human",
        }
    category, confidence, oracle, agent_kind = mapping.get(
        reason, ("unknown", "low", "none", "human")
    )
    return {
        "category": category,
        "confidence": confidence,
        "oracle": oracle,
        "agent_kind": agent_kind,
    }


def report_for_run(
    *,
    output: str,
    binary: Path,
    kernel: Path,
    initrd: Path,
    dtb: Path,
    uart: bytes,
    wall_s: float,
    snapshot: str | None,
) -> dict[str, Any]:
    revision_match = re.search(r"^BINARY_GIT_REV=(\S+)$", output, re.MULTILINE)
    done_match = re.search(r"^\[done\] Executed (\d+) steps\. PC=(0x[0-9a-fA-F]+)$", output, re.MULTILINE)
    halt_match = re.search(r"^\[halt\] Reason: (.*)$", output, re.MULTILINE)
    spin_match = re.search(
        r"^\[spin\] PC=(0x[0-9a-fA-F]+) iteration_count=(\d+)$", output, re.MULTILINE
    )
    if revision_match is None or done_match is None:
        raise ValueError("runner output is missing BINARY_GIT_REV or [done] summary")

    revision = revision_match.group(1)
    steps = int(done_match.group(1))
    pc = f"0x{int(done_match.group(2), 16):016x}"
    uart_text = uart.decode("utf-8", errors="replace")
    uart_lines = uart_text.splitlines()
    init_marker = INIT_SUCCESS_MARKER in uart

    access_detail, access_reason = parse_access(output)
    raw_halt = halt_match.group(1) if halt_match else ""
    if access_reason:
        reason = access_reason
    elif "IllegalInstruction" in raw_halt:
        reason = "illegal_insn"
    elif "FetchFault" in raw_halt:
        reason = "prefetch_abort"
    elif "sysreg" in raw_halt.lower() or "unhandled system register" in raw_halt.lower():
        reason = "unhandled_sysreg"
    elif "WasmTrap" in raw_halt and "decode" in raw_halt.lower():
        reason = "decode_trap"
    elif "spin_detected" in raw_halt:
        reason = "spin_detected"
    elif "ExitVm" in raw_halt:
        reason = "completed"
    elif "livelock" in raw_halt.lower():
        reason = "unknown"
    else:
        reason = "unknown"

    word_match = re.search(
        rf"^\s*{re.escape(pc)}: (0x[0-9a-fA-F]+)$", output, re.MULTILINE
    )
    insn = int(word_match.group(1), 16) if word_match else None
    if insn is None:
        insn_hex = "unknown"
        disasm = "unknown"
    else:
        insn_hex = f"0x{insn:08x}"
        disasm = disassemble(insn, int(pc, 16)) if disassemble else f".word {insn_hex}"
        disasm = re.sub(r", #0x0\]$", "]", disasm)

    if access_detail is None:
        detail: dict[str, Any] = {}
    else:
        detail = access_detail

    report = {
        "schema": "frontier-boot-report/1",
        "binary": {"git_rev": revision, "sha256": sha256(binary), "profile": "release"},
        "guest": {
            "image_sha256": sha256(kernel),
            "initramfs_sha256": sha256(initrd),
            "dtb_sha256": sha256(dtb),
        },
        "run": {
            "steps": steps,
            "halt_step": steps if halt_match or reason == "completed" else None,
            "wall_s": round(wall_s, 3),
            "resumed_from_snapshot": snapshot,
        },
        "halt": {
            "pc": pc,
            "symbol": symbolize(pc),
            "insn_hex": insn_hex,
            "disasm": disasm,
            "reason": reason,
            "detail": detail,
        },
        "uart": {
            "bytes": len(uart),
            "init_marker": init_marker,
            "tail": uart_lines[-5:],
        },
        "spin": (
            {
                "pc": f"0x{int(spin_match.group(1), 16):016x}",
                "iteration_count": int(spin_match.group(2)),
            }
            if spin_match
            else None
        ),
        "classification": classification(reason, init_marker),
    }
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--boot-report-json",
        metavar="PATH",
        help="write a frontier-boot-report/1 JSON document to PATH after the run",
    )
    args = parser.parse_args()

    env = os.environ.copy()
    revision_result = run(["git", "rev-parse", "HEAD"], env=env)
    if revision_result.returncode:
        return emit_failure("git rev-parse HEAD", revision_result)
    expected_revision = revision_result.stdout.strip()
    env["PATHN_BUILD_GIT_REV"] = expected_revision

    build = run(
        ["cargo", "build", "--release", "-p", "u12-orchestrator", "--bin", "run_kernel"],
        env=env,
    )
    if build.returncode:
        return emit_failure("emulator build", build)

    try:
        kernel = input_path("BOOT_REPORT_KERNEL", "guest-image/Image")
        initrd = input_path("BOOT_REPORT_INITRD", "guest-image/initramfs.cpio")
        dtb = input_path("BOOT_REPORT_DTB", "guest-image/minimal-virt.dtb")
    except OSError as error:
        print(f"boot-report: {error}", file=sys.stderr)
        return 2

    target_dir = Path(env.get("CARGO_TARGET_DIR", "target"))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    binary = target_dir / "release" / "run_kernel"
    if not binary.is_file():
        print(f"boot-report: emulator binary was not produced at {binary}", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory(prefix="pathn-boot-report-") as temporary:
        uart_path = Path(temporary) / "uart.bin"
        boot_args = [
            str(binary),
            "--kernel", str(kernel),
            "--dtb", str(dtb),
            "--initrd", str(initrd),
            "--console-output", str(uart_path),
        ]
        snapshot = env.get("BOOT_REPORT_SNAPSHOT")
        if snapshot:
            boot_args.extend(["--load-snapshot", snapshot])
        max_steps = env.get("BOOT_REPORT_MAX_STEPS")
        if max_steps:
            boot_args.extend(["--max-steps", max_steps])
        if args.boot_report_json:
            boot_args.extend(["--dump-around", "4", "--spin-detect"])
            env["PATHN_BOOT_REPORT_DIAGNOSTICS"] = "1"

        started = time.monotonic()
        boot = run(boot_args, env=env)
        wall_s = time.monotonic() - started
        if boot.returncode:
            return emit_failure("standard boot", boot)
        output = boot.stdout + boot.stderr
        revision_match = re.search(r"^BINARY_GIT_REV=(\S+)$", output, re.MULTILINE)
        if not revision_match:
            print("boot-report: emulator startup log did not include BINARY_GIT_REV", file=sys.stderr)
            return 1
        binary_revision = revision_match.group(1)
        if binary_revision != expected_revision:
            print(
                f"boot-report: built binary revision {binary_revision} does not match HEAD {expected_revision}",
                file=sys.stderr,
            )
            return 1
        uart = uart_path.read_bytes()
        uart_seed = env.get("BOOT_REPORT_UART_SEED")
        if uart_seed:
            seed_path = Path(uart_seed)
            if not seed_path.is_absolute():
                seed_path = ROOT / seed_path
            try:
                uart = seed_path.read_bytes() + uart
            except OSError as error:
                print(f"boot-report: cannot read BOOT_REPORT_UART_SEED: {error}", file=sys.stderr)
                return 2
        if args.boot_report_json:
            try:
                report = report_for_run(
                    output=output,
                    binary=binary,
                    kernel=kernel,
                    initrd=initrd,
                    dtb=dtb,
                    uart=uart,
                    wall_s=wall_s,
                    snapshot=snapshot,
                )
            except (OSError, ValueError) as error:
                print(f"boot-report: cannot assemble JSON report: {error}", file=sys.stderr)
                return 1
            report_path = Path(args.boot_report_json)
            if not report_path.is_absolute():
                report_path = ROOT / report_path
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    done_match = re.search(r"^\[done\].*?PC=(0x[0-9a-fA-F]+)$", boot.stdout, re.MULTILINE)
    halted = re.search(r"^\[halt\] Reason:", boot.stdout, re.MULTILINE) is not None
    halt_pc = done_match.group(1) if halted and done_match else "none"
    halt_fn = symbolize(halt_pc) if halt_pc != "none" else "unknown"

    print(f"BINARY_GIT_REV={binary_revision}")
    print(f"UART_BYTES={len(uart)}")
    print(f"INIT_MARKER={'haan' if INIT_SUCCESS_MARKER in uart else 'nahi'}")
    print(f"HALT_PC={halt_pc}")
    print(f"HALT_FN={halt_fn}")
    if args.boot_report_json:
        print(f"BOOT_REPORT_JSON={args.boot_report_json}")
    for line in output.splitlines():
        if line.startswith("SYSREG_MRS "):
            print(line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
