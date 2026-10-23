#!/usr/bin/env python3
"""T626 cold-boot supervisor with hash preflight and heartbeat."""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time


WORKTREE = Path("/Users/Shared/wt-track626-m3-runner-fix")
RUNNER = WORKTREE / "target-t626/release/run_kernel"
KERNEL = Path("/Users/Shared/guest-inputs/Image")
DTB = Path("/Users/Shared/guest-inputs/minimal-virt-fixed.dtb")
INITRD = Path("/Users/Shared/guest-inputs/initramfs.cpio")
FLIGHT_DIR = Path("/Users/Shared/t626-flight")
UART_RAW = FLIGHT_DIR / "guest-uart.log"
UART_GZ = FLIGHT_DIR / "guest-uart.log.gz"
RUNNER_LOG = FLIGHT_DIR / "runner.log"
HEARTBEATS = (FLIGHT_DIR / "heartbeat.json", Path("/Users/Shared/heartbeat-t626.json"))
OUTCOME = Path("/Users/Shared/outcome-t626.txt")

EXPECTED_SHA256 = {
    KERNEL: "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
    DTB: "f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0",
    INITRD: "49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38",
}
PID1_MARKERS = (
    b"[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)",
    b"M3-PID1 ",
)
EXPECTED_FUTEX_VECTOR = {
    "esr_el1": 0x96000005,
    "far_el1": 0x0,
    "elr_el1": 0xFFFFFF80082591E4,
}
EXPECTED_FUTEX_RESUME_PC = 0xFFFFFF8008C85A8C

progress_re = re.compile(
    r"\[progress\] step (\d+) \((\d+)M\), PC=(0x[0-9a-fA-F]+), "
    r"console_bytes=(\d+), irq_in=(\d+), irq_out=(\d+)"
)
abort_re = re.compile(
    r"\[data-abort\] step=(\d+) count=(\d+) ESR_EL1=(0x[0-9a-fA-F]+) "
    r"FAR_EL1=(0x[0-9a-fA-F]+) ELR_EL1=(0x[0-9a-fA-F]+) "
    r"vector_pc=(0x[0-9a-fA-F]+) resume_lr=(0x[0-9a-fA-F]+)"
)
done_re = re.compile(
    r"\[done\] Executed (\d+) steps\. PC=(0x[0-9a-fA-F]+), "
    r"irq_in=(\d+), irq_out=(\d+), da_in=(\d+), brk_in=(\d+)"
)
marker_re = re.compile(r"\[marker\] EXACT PID-1 MARKER OBSERVED at step (\d+)!?")
fixup_re = re.compile(
    r"\[futex-fixup\] event_step=(\d+) fixup_resume=(0x[0-9a-fA-F]+) resume_step=(\d+)"
)
futex_summary_re = re.compile(
    r"\[futex-vector-check\] exact_occurrences=(\d+) resume_observed=(\d+) "
    r"mismatches=(\d+) unresolved=(\d+) expected_occurrences=(\d+) verdict=(PASS|FAIL)"
)
guest_time_re = re.compile(r"\[\s*(\d+\.\d+)\]\s+Run /init as init process")

state_lock = threading.Lock()
heartbeat_stop = threading.Event()
state = {
    "lane": "T626",
    "status": "starting",
    "supervisor_pid": os.getpid(),
    "runner_pid": 0,
    "current_step": 0,
    "current_pc": "0x0",
    "console_bytes": 0,
    "irq_in": 0,
    "irq_out": 0,
    "da_in": 0,
    "brk_in": 0,
    "last_milestone": "starting",
    "started_epoch": time.time(),
}


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def atomic_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(content, encoding="utf-8")
    os.replace(temporary, path)


def snapshot_state() -> dict[str, object]:
    with state_lock:
        result = dict(state)
    result["timestamp_utc"] = utc_now()
    result["elapsed_seconds"] = round(time.time() - float(result["started_epoch"]), 1)
    return result


def write_heartbeat() -> None:
    content = json.dumps(snapshot_state(), indent=2) + "\n"
    for path in HEARTBEATS:
        try:
            atomic_text(path, content)
        except OSError as error:
            print(f"[supervisor] heartbeat write failed for {path}: {error}", file=sys.stderr)


def heartbeat_worker() -> None:
    while not heartbeat_stop.wait(15):
        write_heartbeat()


def git_value(*args: str) -> str:
    result = subprocess.run(
        ["git", *args], cwd=WORKTREE, capture_output=True, text=True, check=False
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def preflight() -> tuple[dict[str, str], list[str]]:
    actual: dict[str, str] = {}
    mismatches: list[str] = []
    for path, expected in EXPECTED_SHA256.items():
        if not path.is_file():
            actual[str(path)] = "missing"
            mismatches.append(f"missing input: {path}")
            continue
        digest = sha256_file(path)
        actual[str(path)] = digest
        if digest != expected:
            mismatches.append(f"SHA-256 mismatch for {path}: expected {expected}, got {digest}")
    if not RUNNER.is_file():
        actual[str(RUNNER)] = "missing"
        mismatches.append(f"missing runner binary: {RUNNER}")
    else:
        actual[str(RUNNER)] = sha256_file(RUNNER)
    return actual, mismatches


def parse_uart() -> dict[str, object]:
    if UART_RAW.exists():
        data = UART_RAW.read_bytes()
    elif UART_GZ.exists():
        with gzip.open(UART_GZ, "rb") as stream:
            data = stream.read()
    else:
        return {"bytes": 0, "sha256": "missing", "handoff_offset": None, "marker": None}
    handoff = data.find(b"Run /init as init process")
    marker_info = None
    for marker in PID1_MARKERS:
        marker_offset = data.find(marker)
        if marker_offset < 0:
            continue
        line_start = data.rfind(b"\n", 0, marker_offset) + 1
        line_end = data.find(b"\n", marker_offset)
        if line_end < 0:
            line_end = len(data)
        line = data[line_start:line_end].rstrip(b"\r")
        marker_info = {
            "offset": marker_offset,
            "line_bytes_repr": repr(line),
            "after_kernel_handoff": handoff >= 0 and marker_offset > handoff,
        }
        break
    handoff_match = guest_time_re.search(data.decode("utf-8", errors="replace"))
    return {
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "handoff_offset": handoff if handoff >= 0 else None,
        "handoff_guest_time_seconds": handoff_match.group(1) if handoff_match else None,
        "marker": marker_info,
    }


def gzip_uart_log() -> None:
    if not UART_RAW.exists():
        return
    with UART_RAW.open("rb") as source, gzip.open(UART_GZ, "wb", compresslevel=6) as target:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            target.write(block)
    UART_RAW.unlink()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--max-steps", type=int, default=300_000_000)
    parser.add_argument("--preflight-only", action="store_true")
    args = parser.parse_args()

    FLIGHT_DIR.mkdir(parents=True, exist_ok=True)
    if UART_RAW.exists() or RUNNER_LOG.exists():
        print(f"Refusing to overwrite existing T626 flight logs in {FLIGHT_DIR}", file=sys.stderr)
        return 2

    hashes, mismatches = preflight()
    print(json.dumps({"input_and_runner_sha256": hashes, "mismatches": mismatches}, indent=2))
    if mismatches:
        with state_lock:
            state.update(status="preflight_failed", last_milestone="SHA-256 preflight rejected boot")
        write_heartbeat()
        atomic_text(OUTCOME, "outcome: preflight-failed\n" + "\n".join(mismatches) + "\n")
        return 2
    if args.preflight_only:
        return 0

    runner_rev = git_value("rev-parse", "HEAD")
    author_name = git_value("config", "user.name")
    author_email = git_value("config", "user.email")
    binary_sha256 = hashes[str(RUNNER)]
    command = [
        str(RUNNER),
        "--kernel", str(KERNEL),
        "--dtb", str(DTB),
        "--initrd", str(INITRD),
        "--max-steps", str(args.max_steps),
        "--console-output", str(UART_RAW),
        "--dump-console",
    ]

    with state_lock:
        state.update(status="running", last_milestone="cold boot launched")
    write_heartbeat()
    heartbeat_thread = threading.Thread(target=heartbeat_worker, daemon=True)
    heartbeat_thread.start()

    process: subprocess.Popen[str] | None = None
    observed_marker_step: int | None = None
    marker_observed_utc: str | None = None
    abort_vectors: list[dict[str, object]] = []
    fixup_events: list[dict[str, int]] = []
    futex_summary: dict[str, object] = {}
    trap_inventory = {"data_abort_lines": 0, "brk_lines": 0, "trap_lines": 0, "exception_lines": 0, "fault_lines": 0}
    halt_lines: list[str] = []
    done_fields: dict[str, object] = {}
    supervisor_error: str | None = None

    def stop_child(_signum: int, _frame: object) -> None:
        if process is not None and process.poll() is None:
            process.terminate()

    signal.signal(signal.SIGTERM, stop_child)
    signal.signal(signal.SIGINT, stop_child)

    try:
        process = subprocess.Popen(
            command,
            cwd=WORKTREE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
        )
        with state_lock:
            state["runner_pid"] = process.pid
        assert process.stdout is not None
        with RUNNER_LOG.open("w", encoding="utf-8") as runner_log:
            for raw_line in process.stdout:
                line = raw_line.rstrip("\n")
                runner_log.write(line + "\n")
                runner_log.flush()
                with state_lock:
                    state["last_line"] = line[:240]
                match = progress_re.search(line)
                if match:
                    with state_lock:
                        state.update(
                            current_step=int(match.group(1)),
                            current_pc=match.group(3),
                            console_bytes=int(match.group(4)),
                            irq_in=int(match.group(5)),
                            irq_out=int(match.group(6)),
                            last_milestone=f"step_{match.group(2)}M",
                        )
                match = abort_re.search(line)
                if match:
                    vector = {
                        "step": int(match.group(1)),
                        "count": int(match.group(2)),
                        "esr_el1": int(match.group(3), 16),
                        "far_el1": int(match.group(4), 16),
                        "elr_el1": int(match.group(5), 16),
                        "vector_pc": int(match.group(6), 16),
                        "resume_lr": int(match.group(7), 16),
                    }
                    abort_vectors.append(vector)
                    trap_inventory["data_abort_lines"] += 1
                    with state_lock:
                        state.update(current_step=vector["step"], da_in=vector["count"], last_milestone="futex data-abort vector")
                match = marker_re.search(line)
                if match:
                    observed_marker_step = int(match.group(1))
                    marker_observed_utc = utc_now()
                    with state_lock:
                        state.update(current_step=observed_marker_step, last_milestone="guest PID-1 marker observed")
                match = fixup_re.search(line)
                if match:
                    fixup = {
                        "event_step": int(match.group(1)),
                        "fixup_resume": int(match.group(2), 16),
                        "resume_step": int(match.group(3)),
                    }
                    fixup_events.append(fixup)
                    with state_lock:
                        state.update(current_step=fixup["resume_step"], last_milestone="futex exception-table fixup resumed")
                match = futex_summary_re.search(line)
                if match:
                    futex_summary = {
                        "exact_occurrences": int(match.group(1)),
                        "resume_observed": int(match.group(2)),
                        "mismatches": int(match.group(3)),
                        "unresolved": int(match.group(4)),
                        "expected_occurrences": int(match.group(5)),
                        "verdict": match.group(6),
                    }
                match = done_re.search(line)
                if match:
                    done_fields = {
                        "step": int(match.group(1)),
                        "pc": match.group(2),
                        "irq_in": int(match.group(3)),
                        "irq_out": int(match.group(4)),
                        "da_in": int(match.group(5)),
                        "brk_in": int(match.group(6)),
                    }
                    with state_lock:
                        state.update(current_step=done_fields["step"], current_pc=done_fields["pc"], irq_in=done_fields["irq_in"], irq_out=done_fields["irq_out"], da_in=done_fields["da_in"], brk_in=done_fields["brk_in"])
                if "[brk]" in line:
                    trap_inventory["brk_lines"] += 1
                if "[trap]" in line:
                    trap_inventory["trap_lines"] += 1
                if "[exception]" in line:
                    trap_inventory["exception_lines"] += 1
                if "[fault]" in line:
                    trap_inventory["fault_lines"] += 1
                if "[halt]" in line:
                    halt_lines.append(line)
        return_code = process.wait()
    except Exception as error:  # Always leave a terminal outcome and heartbeat.
        supervisor_error = f"{type(error).__name__}: {error}"
        return_code = process.wait() if process is not None else 127
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        heartbeat_stop.set()
        heartbeat_thread.join(timeout=2)
        gzip_uart_log()

    uart = parse_uart()
    marker = uart.get("marker")
    marker_ok = bool(
        observed_marker_step is not None
        and isinstance(marker, dict)
        and marker.get("after_kernel_handoff")
    )
    futex = abort_vectors[0] if abort_vectors else None
    futex_check = {
        key: bool(futex is not None and futex.get(key) == expected)
        for key, expected in EXPECTED_FUTEX_VECTOR.items()
    }
    exact_fixup = (
        len(fixup_events) == 1
        and futex is not None
        and fixup_events[0]["event_step"] == futex["step"]
        and fixup_events[0]["fixup_resume"] == EXPECTED_FUTEX_RESUME_PC
        and fixup_events[0]["resume_step"] > fixup_events[0]["event_step"]
    )
    futex_check.update(
        resume_pc=exact_fixup,
        exact_occurrences=futex_summary.get("exact_occurrences") == 1,
        resume_observed=futex_summary.get("resume_observed") == 1,
        mismatches=futex_summary.get("mismatches") == 0,
        unresolved=futex_summary.get("unresolved") == 0,
        summary_verdict=futex_summary.get("verdict") == "PASS",
        da_in=done_fields.get("da_in") == 1,
        brk_in=done_fields.get("brk_in") == 0,
    )
    futex_check["pass"] = all(futex_check.values())
    status = (
        "PASS"
        if marker_ok and futex_check["pass"] and return_code == 0 and supervisor_error is None
        else "FAIL"
    )
    with state_lock:
        state.update(status="complete" if status == "PASS" else "failed", last_milestone="flight complete")
    write_heartbeat()

    outcome = {
        "outcome": status,
        "flight_started_utc": dt.datetime.fromtimestamp(float(state["started_epoch"]), dt.timezone.utc).isoformat(timespec="seconds"),
        "flight_finished_utc": utc_now(),
        "runner_revision": runner_rev,
        "runner_binary_sha256": binary_sha256,
        "locked_attribution": f"{author_name} <{author_email}>",
        "return_code": return_code,
        "step_budget": args.max_steps,
        "guest_marker_step": observed_marker_step,
        "guest_marker_observed_utc": marker_observed_utc,
        "guest_uart": uart,
        "guest_uart_gzip_path": str(UART_GZ) if UART_GZ.exists() else "missing",
        "booted_images_sha256": {str(path): hashes[str(path)] for path in EXPECTED_SHA256},
        "initramfs_decision": "T612 canonical initramfs booted unchanged; no replacement image was built",
        "futex_vector": futex,
        "futex_fixup_events": fixup_events,
        "futex_runner_summary": futex_summary,
        "futex_vector_check": futex_check,
        "trap_inventory": trap_inventory,
        "done_fields": done_fields,
        "halt_lines": halt_lines,
        "supervisor_error": supervisor_error,
        "runner_log": str(RUNNER_LOG),
    }
    atomic_text(OUTCOME, json.dumps(outcome, indent=2) + "\n")
    print(json.dumps(outcome, indent=2))
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
