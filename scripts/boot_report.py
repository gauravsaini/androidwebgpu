#!/usr/bin/env python3
"""Build and run the standard native ARM64 boot, then print its five metrics."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], *, env: dict[str, str], cwd: Path = ROOT) -> subprocess.CompletedProcess[str]:
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


def symbolize(pc: str) -> str:
    symbol_file = os.environ.get("BOOT_REPORT_SYMBOLS")
    if not symbol_file:
        return "unknown"
    path = Path(symbol_file)
    if not path.is_absolute():
        path = ROOT / path
    if not path.is_file():
        return "unknown"
    result = subprocess.run(["nm", "-n", str(path)], text=True, capture_output=True, check=False)
    if result.returncode:
        return "unknown"
    address = int(pc, 16)
    nearest: tuple[int, str] | None = None
    for line in result.stdout.splitlines():
        match = re.match(r"^\s*([0-9a-fA-F]+)\s+\S\s+(.+)$", line)
        if not match:
            continue
        start, name = int(match.group(1), 16), match.group(2).strip()
        if start <= address and (nearest is None or start > nearest[0]):
            nearest = (start, name)
    if nearest is None:
        return "unknown"
    return f"{nearest[1]}+0x{address - nearest[0]:x}"


def main() -> int:
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
        boot = run(
            [
                str(binary),
                "--kernel", str(kernel),
                "--dtb", str(dtb),
                "--initrd", str(initrd),
                "--console-output", str(uart_path),
            ],
            env=env,
        )
        if boot.returncode:
            return emit_failure("standard boot", boot)
        revision_match = re.search(r"^BINARY_GIT_REV=(\S+)$", boot.stdout, re.MULTILINE)
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
        done_match = re.search(r"^\[done\].*?PC=(0x[0-9a-fA-F]+)$", boot.stdout, re.MULTILINE)
        halted = re.search(r"^\[halt\] Reason:", boot.stdout, re.MULTILINE) is not None
        halt_pc = done_match.group(1) if halted and done_match else "none"
        halt_fn = symbolize(halt_pc) if halt_pc != "none" else "unknown"

    print(f"BINARY_GIT_REV={binary_revision}")
    print(f"UART_BYTES={len(uart)}")
    print(f"INIT_MARKER={'haan' if b'/init' in uart else 'nahi'}")
    print(f"HALT_PC={halt_pc}")
    print(f"HALT_FN={halt_fn}")
    for line in boot.stdout.splitlines():
        if line.startswith("SYSREG_MRS "):
            print(line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
