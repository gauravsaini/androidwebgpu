#!/usr/bin/env -S uv run --offline --no-project --python python3
"""Adapt the frontier driver's combined runner flags to T69/T70 interfaces."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

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


def run(command: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, check=False)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def repo_root(cwd: Path) -> Path:
    result = subprocess.run(
        ["git", "-C", str(cwd), "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"cannot find repository root from {cwd}: {result.stderr.strip()}")
    return Path(result.stdout.strip())


def resolve(root: Path, value: str) -> Path:
    path = Path(value).expanduser()
    return path if path.is_absolute() else root / path


def read_exact(stream: Any, size: int) -> bytes:
    data = stream.read(size)
    if len(data) != size:
        raise ValueError(f"truncated snapshot: expected {size} bytes, got {len(data)}")
    return data


def full_snapshot_metadata(path: Path, u11_length: int) -> tuple[int, bytes]:
    with path.open("rb") as stream:
        stream.seek(16 + u11_length)
        if read_exact(stream, 4) != b"PN12":
            raise ValueError("PNFS snapshot is missing its PN12 orchestrator section")
        transport = read_exact(stream, 15)
        queue_count = struct.unpack("<I", transport[11:15])[0]
        if queue_count > 1024:
            raise ValueError(f"invalid PNFS transport queue count: {queue_count}")
        stream.seek(queue_count * 31, os.SEEK_CUR)
        stream.seek(12, os.SEEK_CUR)  # GPU resource and fence counters.
        tx_length = struct.unpack("<Q", read_exact(stream, 8))[0]
        if tx_length > 64 * 1024 * 1024:
            raise ValueError(f"invalid PNFS UART length: {tx_length}")
        uart = read_exact(stream, tx_length)
        rx_length = struct.unpack("<I", read_exact(stream, 4))[0]
        if rx_length > 64 * 1024 * 1024:
            raise ValueError(f"invalid PNFS UART receive length: {rx_length}")
        stream.seek(rx_length + 8, os.SEEK_CUR)  # RX bytes and clock cycles.
        step = struct.unpack("<Q", read_exact(stream, 8))[0]
        return step, uart


def normalize_snapshot(path: Path, temporary: Path) -> tuple[Path, int, bytes]:
    """Return a snapshot readable by this checkout's T69 runner."""
    with path.open("rb") as stream:
        prefix = read_exact(stream, 4)
        if prefix != b"PNFS":
            stream.seek(0)
            step_header = read_exact(stream, 8)
            step = struct.unpack("<Q", step_header)[0]
            version = struct.unpack("<I", read_exact(stream, 4))[0]
            if version != 13:
                raise ValueError(
                    f"legacy snapshot has U11 version {version}; this runner requires version 13"
                )
            return path, step, b""

        full_version = struct.unpack("<I", read_exact(stream, 4))[0]
        if full_version != 1:
            raise ValueError(f"unsupported PNFS full-snapshot version: {full_version}")
        u11_length = struct.unpack("<Q", read_exact(stream, 8))[0]
        if u11_length > path.stat().st_size - 16:
            raise ValueError("PNFS U11 section extends beyond the snapshot file")
        stream.seek(16)
        u11_version, cpu_count = struct.unpack("<II", read_exact(stream, 8))
        cpu_sizes = {13: 167 * 8, 14: 169 * 8, 15: 170 * 8}
        if u11_version not in cpu_sizes:
            raise ValueError(f"unsupported PNFS U11 snapshot version: {u11_version}")
        if cpu_count == 0 or cpu_count > 64:
            raise ValueError(f"invalid PNFS CPU count: {cpu_count}")
        source_cpu_size = cpu_sizes[u11_version]
        if 8 + cpu_count * source_cpu_size > u11_length:
            raise ValueError("PNFS U11 CPU records exceed the declared section length")

    step, uart = full_snapshot_metadata(path, u11_length)
    legacy_path = temporary / f"legacy-{path.name}"
    remaining = u11_length - 8 - cpu_count * source_cpu_size
    with path.open("rb") as source, legacy_path.open("wb") as target:
        target.write(struct.pack("<Q", step))
        source.seek(24)  # Skip the PNFS envelope and U11 version/CPU-count header.
        target.write(struct.pack("<II", 13, cpu_count))
        for _ in range(cpu_count):
            cpu = read_exact(source, source_cpu_size)
            if u11_version == 15:
                # v15 added sp_el1 before PC and ELR_EL1/SPSR_EL1 at the end.
                target.write(cpu[:256])
                target.write(cpu[264:-16])
            elif u11_version == 14:
                # v14 added ELR_EL1/SPSR_EL1 at the end of each CPU record.
                target.write(cpu[:-16])
            else:
                target.write(cpu)
        while remaining:
            chunk = source.read(min(1024 * 1024, remaining))
            if not chunk:
                raise ValueError("truncated PNFS U11 machine section")
            target.write(chunk)
            remaining -= len(chunk)
    print(
        f"[adapter] converted PNFS/U11 v{u11_version} to T69/U11 v13 "
        f"at step {step}; discarded fields are not present in v13"
    )
    return legacy_path, step, uart


def guest_input(root: Path, variable: str, default: str, override: str | None) -> Path:
    path = resolve(root, override or os.environ.get(variable, default))
    if not path.is_file():
        raise FileNotFoundError(f"{variable} points to a missing input: {path}")
    return path


def device_hint(pa: int) -> str:
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
        return (
            {
                "pa": f"0x{pa:08x}",
                "width_bytes": int(raw_width),
                "is_write": operation == "write",
                "device_hint": hint,
            },
            f"unmapped_mmio_{operation}" if hint != "unknown" else "data_abort",
        )
    match = MMU_ACCESS_RE.search(output)
    if match:
        operation, raw_va, fault = match.groups()
        return (
            {
                "va": f"0x{int(raw_va, 16):016x}",
                "is_write": operation == "write",
                "fault": fault,
            },
            "data_abort",
        )
    return None, None


def classify(reason: str, init_marker: bool) -> dict[str, str]:
    mapping = {
        "unmapped_mmio_read": ("unmapped_mmio", "high", "qemu_mmio_trace", "device"),
        "unmapped_mmio_write": ("unmapped_mmio", "high", "qemu_mmio_trace", "device"),
        "illegal_insn": ("unknown_insn", "high", "objdump_capstone", "decode"),
        "decode_trap": ("unknown_insn", "medium", "objdump_capstone", "decode"),
        "unhandled_sysreg": ("sysreg", "high", "qemu_sysreg_dump", "sysreg"),
        "data_abort": ("fault_divergence", "medium", "qemu_divergence", "divergence"),
        "prefetch_abort": ("fault_divergence", "medium", "qemu_divergence", "divergence"),
        "spin_detected": ("spin", "high", "symbolize_poll", "spin"),
    }
    if reason == "completed":
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
    done_match = re.search(
        r"^\[done\] Executed (\d+) steps\. PC=(0x[0-9a-fA-F]+)$", output, re.MULTILINE
    )
    halt_match = re.search(r"^\[halt\] Reason: (.*)$", output, re.MULTILINE)
    spin_match = re.search(
        r"^\[spin\] PC=(0x[0-9a-fA-F]+) iteration_count=(\d+)$", output, re.MULTILINE
    )
    if revision_match is None or done_match is None:
        raise ValueError("runner output is missing BINARY_GIT_REV or [done] summary")

    revision = revision_match.group(1)
    steps = int(done_match.group(1))
    pc = f"0x{int(done_match.group(2), 16):016x}"
    uart_lines = uart.decode("utf-8", errors="replace").splitlines()
    init_marker = INIT_SUCCESS_MARKER in uart
    access_detail, access_reason = parse_access(output)
    raw_halt = halt_match.group(1) if halt_match else ""
    lowered_halt = raw_halt.lower()
    word_match = re.search(rf"^\s*{re.escape(pc)}: (0x[0-9a-fA-F]+)$", output, re.MULTILINE)
    insn = int(word_match.group(1), 16) if word_match else None

    # T69's host fault text retains the out-of-range physical address even
    # when T70's opt-in access diagnostic lines are unavailable.
    if access_detail is None:
        mem_fault = re.search(
            r"mem_(load|store) fault: TranslationFault \{ va: (0x[0-9a-fA-F]+|\d+) \}",
            raw_halt,
        )
        if mem_fault:
            operation, raw_pa = mem_fault.groups()
            pa = int(raw_pa, 16) if raw_pa.lower().startswith("0x") else int(raw_pa)
            is_write = operation == "store"
            hint = device_hint(pa)
            access_reason = (
                f"unmapped_mmio_{'write' if is_write else 'read'}"
                if hint != "unknown"
                else "data_abort"
            )
            access_detail = {
                "pa": f"0x{pa:08x}",
                "is_write": is_write,
                "device_hint": hint,
            }
            if insn is not None and insn & 0x3B000000 == 0x39000000:
                access_detail["width_bytes"] = 1 << ((insn >> 30) & 0x3)

    if access_reason:
        reason = access_reason
    elif "illegalinstruction" in lowered_halt:
        reason = "illegal_insn"
    elif "fetchfault" in lowered_halt:
        reason = "prefetch_abort"
    elif "unhandled system register" in lowered_halt or "sysreg" in lowered_halt:
        reason = "unhandled_sysreg"
    elif "spin_detected" in lowered_halt:
        reason = "spin_detected"
    elif "exitvm" in lowered_halt:
        reason = "completed"
    elif "mem_load fault" in lowered_halt or "mem_store fault" in lowered_halt or "mmu_fault" in lowered_halt:
        reason = "data_abort"
    elif "decode" in lowered_halt and "wasmtrap" in lowered_halt:
        reason = "decode_trap"
    else:
        reason = "unknown"

    insn_hex = f"0x{insn:08x}" if insn is not None else "unknown"
    detail: dict[str, Any] = access_detail or {}
    if not detail and raw_halt:
        detail = {"raw_halt": raw_halt}

    return {
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
            "symbol": f"unknown+{pc}",
            "insn_hex": insn_hex,
            "disasm": f".word {insn_hex}" if insn is not None else "unknown",
            "reason": reason,
            "detail": detail,
        },
        "uart": {"bytes": len(uart), "init_marker": init_marker, "tail": uart_lines[-5:]},
        "spin": (
            {"pc": f"0x{int(spin_match.group(1), 16):016x}", "iteration_count": int(spin_match.group(2))}
            if spin_match
            else None
        ),
        "classification": classify(reason, init_marker),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--boot-report-json", metavar="PATH")
    parser.add_argument("--load-snapshot", metavar="PATH")
    parser.add_argument("--save-snapshot", metavar="PATH")
    parser.add_argument("--snapshot-at-step", type=int, metavar="STEP")
    parser.add_argument("--kernel")
    parser.add_argument("--initrd")
    parser.add_argument("--dtb")
    parser.add_argument("--max-steps", type=int)
    parser.add_argument("--trace")
    args, passthrough = parser.parse_known_args(argv)

    if args.snapshot_at_step is not None and not args.save_snapshot:
        parser.error("--snapshot-at-step requires --save-snapshot")
    if args.snapshot_at_step is not None and args.snapshot_at_step < 0:
        parser.error("--snapshot-at-step must be nonnegative")

    root = repo_root(Path.cwd())
    env = os.environ.copy()
    revision_result = run(["git", "rev-parse", "HEAD"], cwd=root, env=env)
    if revision_result.returncode:
        return_code = revision_result.returncode or 1
        sys.stderr.write(revision_result.stdout + revision_result.stderr)
        return return_code
    expected_revision = revision_result.stdout.strip()
    env["PATHN_BUILD_GIT_REV"] = expected_revision

    try:
        kernel = guest_input(root, "BOOT_REPORT_KERNEL", "guest-image/Image", args.kernel)
        initrd = guest_input(root, "BOOT_REPORT_INITRD", "guest-image/initramfs.cpio", args.initrd)
        dtb = guest_input(root, "BOOT_REPORT_DTB", "guest-image/minimal-virt.dtb", args.dtb)
    except OSError as error:
        print(f"run_kernel adapter: {error}", file=sys.stderr)
        return 2

    build = run(
        ["cargo", "build", "--release", "-p", "u12-orchestrator", "--bin", "run_kernel"],
        cwd=root,
        env=env,
    )
    if build.returncode:
        sys.stderr.write(build.stdout + build.stderr)
        print(f"run_kernel adapter: build failed with exit code {build.returncode}", file=sys.stderr)
        return build.returncode or 1

    target_dir = Path(env.get("CARGO_TARGET_DIR", "target"))
    if not target_dir.is_absolute():
        target_dir = root / target_dir
    binary = target_dir / "release" / "run_kernel"
    if not binary.is_file():
        print(f"run_kernel adapter: binary was not produced at {binary}", file=sys.stderr)
        return 1

    help_result = run([str(binary), "--help"], cwd=root, env=env)
    supported = help_result.stdout + help_result.stderr
    with tempfile.TemporaryDirectory(prefix="frontier-run-kernel-") as temporary:
        temp_dir = Path(temporary)
        uart_path = resolve(root, os.environ["FRONTIER_UART_OUTPUT"]) if os.environ.get("FRONTIER_UART_OUTPUT") else temp_dir / "uart.bin"
        command = [
            str(binary),
            "--kernel", str(kernel),
            "--dtb", str(dtb),
            "--initrd", str(initrd),
            "--console-output", str(uart_path),
        ]
        snapshot = args.load_snapshot or env.get("BOOT_REPORT_SNAPSHOT")
        loaded_step = None
        snapshot_uart = b""
        if snapshot:
            snapshot_path = resolve(root, snapshot)
            if not snapshot_path.is_file():
                print(f"run_kernel adapter: snapshot does not exist: {snapshot_path}", file=sys.stderr)
                return 2
            try:
                runner_snapshot, loaded_step, snapshot_uart = normalize_snapshot(snapshot_path, temp_dir)
            except (OSError, ValueError) as error:
                print(f"run_kernel adapter: cannot normalize snapshot {snapshot_path}: {error}", file=sys.stderr)
                return 2
            command.extend(["--load-snapshot", str(runner_snapshot)])
            snapshot = str(snapshot_path)
        effective_snapshot_step = args.snapshot_at_step
        if args.save_snapshot:
            save_path = resolve(root, args.save_snapshot)
            save_path.parent.mkdir(parents=True, exist_ok=True)
            command.extend(["--save-snapshot", str(save_path)])
            if effective_snapshot_step is not None:
                if loaded_step is not None and effective_snapshot_step < loaded_step:
                    effective_snapshot_step = loaded_step + 1
                    print(
                        "[adapter] snapshot target precedes loaded checkpoint; "
                        f"clamping save step to {effective_snapshot_step}",
                        file=sys.stderr,
                    )
                command.extend(["--save-at", str(effective_snapshot_step)])
        if args.max_steps is not None:
            command.extend(["--max-steps", str(args.max_steps)])
        elif env.get("BOOT_REPORT_MAX_STEPS"):
            command.extend(["--max-steps", env["BOOT_REPORT_MAX_STEPS"]])
        if args.trace:
            command.extend(["--trace", str(resolve(root, args.trace))])
        if args.boot_report_json:
            # The T69 runner parses --dump-around even though its help omits it.
            command.extend(["--dump-around", "4"])
            if "--spin-detect" in supported:
                command.append("--spin-detect")
                env["PATHN_BOOT_REPORT_DIAGNOSTICS"] = "1"
            else:
                env["PATHN_BOOT_REPORT_DIAGNOSTICS"] = "1"
        command.extend(passthrough)

        started = time.monotonic()
        boot = run(command, cwd=root, env=env)
        wall_s = time.monotonic() - started
        output = boot.stdout + boot.stderr
        if args.boot_report_json:
            report_path = resolve(root, args.boot_report_json)
            runner_log_path = report_path.with_suffix(".runner.log")
        elif args.save_snapshot:
            report_path = None
            runner_log_path = resolve(root, args.save_snapshot).with_suffix(".runner.log")
        else:
            report_path = None
            run_dir = Path(env.get("FRONTIER_RUN_DIR", root / "target" / "frontier-loop"))
            if not run_dir.is_absolute():
                run_dir = root / run_dir
            run_dir.mkdir(parents=True, exist_ok=True)
            runner_log_path = run_dir / f"run-kernel-{os.getpid()}.log"
        runner_log_path.parent.mkdir(parents=True, exist_ok=True)
        runner_log_path.write_text(output, encoding="utf-8")
        if boot.returncode:
            print(output, file=sys.stderr, end="" if output.endswith("\n") else "\n")
            print(f"run_kernel adapter: runner failed with exit code {boot.returncode}", file=sys.stderr)
            return boot.returncode or 1

        if args.save_snapshot:
            save_path = resolve(root, args.save_snapshot)
            if not save_path.is_file():
                print(f"run_kernel adapter: runner did not create snapshot {save_path}", file=sys.stderr)
                return 1
            with save_path.open("rb") as stream:
                snapshot_header = stream.read(8)
            if len(snapshot_header) < 8:
                print(f"run_kernel adapter: snapshot is missing its step-count header: {save_path}", file=sys.stderr)
                return 1
            saved_step = struct.unpack("<Q", snapshot_header)[0]
            print(f"SNAPSHOT_PATH={save_path}")
            print(f"SNAPSHOT_STEP={saved_step}")
            if effective_snapshot_step is not None and saved_step != effective_snapshot_step:
                print(
                    f"run_kernel adapter: requested effective snapshot step {effective_snapshot_step}, got {saved_step}",
                    file=sys.stderr,
                )
                return 1

        revision_match = re.search(r"^BINARY_GIT_REV=(\S+)$", output, re.MULTILINE)
        binary_revision = revision_match.group(1) if revision_match else ""
        if binary_revision != expected_revision:
            print(
                f"run_kernel adapter: binary revision {binary_revision or 'missing'} does not match HEAD {expected_revision}",
                file=sys.stderr,
            )
            return 1
        print(f"BINARY_GIT_REV={binary_revision}")
        print(f"RUN_KERNEL_LOG={runner_log_path}")

        if report_path is not None:
            try:
                uart = uart_path.read_bytes()
                uart_seed = env.get("BOOT_REPORT_UART_SEED")
                if uart_seed:
                    uart = resolve(root, uart_seed).read_bytes() + uart
                elif snapshot_uart:
                    uart = snapshot_uart + uart
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
                print(f"run_kernel adapter: cannot assemble boot report: {error}", file=sys.stderr)
                return 1
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print(f"BOOT_REPORT_JSON={report_path}")
            print(f"HALT_REASON={report['halt']['reason']}")
            print(f"HALT_STEP={report['run']['halt_step']}")
            print(f"HALT_PC={report['halt']['pc']}")
            print(f"INIT_MARKER={str(report['uart']['init_marker']).lower()}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
