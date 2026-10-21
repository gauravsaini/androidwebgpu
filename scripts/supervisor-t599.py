#!/usr/bin/env python3
"""
T599 Supervisor script:
Owns the 300M cold reset boot gate run, captures bounded logs, updates heartbeat.json,
and guarantees outcome-t599.txt is written upon completion, halt, or unexpected termination.
"""

import sys
import os
import time
import subprocess
import json
import hashlib
import re
import signal
import threading

WORKTREE = "/Users/Shared/wt-track599-rust-300m"
KERNEL_BIN = f"{WORKTREE}/target-t599/release/run_kernel"
KERNEL_IMG = "/Users/Shared/guest-inputs/Image"
DTB_IMG = "/Users/Shared/guest-inputs/minimal-virt-fixed.dtb"
INITRD_IMG = "/Users/Shared/guest-inputs/initramfs.cpio"
MAX_STEPS = int(os.environ.get("OVERRIDE_MAX_STEPS", 300_000_000))

GUEST_UART = f"{WORKTREE}/guest-uart.log"
PROGRESS_LOG = f"{WORKTREE}/runner-progress.log"
RUNNER_LOG = f"{WORKTREE}/runner.log"
HEARTBEAT_WT = f"{WORKTREE}/heartbeat.json"
HEARTBEAT_SHARED = "/Users/Shared/heartbeat-t599.json"
OUTCOME_WT = f"{WORKTREE}/outcome-t599.txt"
OUTCOME_SHARED = "/Users/Shared/outcome-t599.txt"

MAX_LOG_LINES = 100_000

# Global state for heartbeat thread
state_lock = threading.Lock()
state = {
    "current_step": 0,
    "current_pc": "0x0",
    "console_bytes": 0,
    "irq_in": 0,
    "irq_out": 0,
    "da_in": 0,
    "brk_in": 0,
    "last_milestone": "launching",
    "status": "running",
    "runner_pid": 0,
    "start_time": time.time(),
    "stop_heartbeat": False
}

def sha256_file(filepath):
    if not os.path.exists(filepath):
        return "none"
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()

def update_heartbeat_files():
    with state_lock:
        data = {
            "lane": "T599",
            "status": state["status"],
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "runner_pid": state["runner_pid"],
            "supervisor_pid": os.getpid(),
            "current_step": state["current_step"],
            "current_pc": state["current_pc"],
            "console_bytes": state["console_bytes"],
            "irq_in": state["irq_in"],
            "irq_out": state["irq_out"],
            "da_in": state["da_in"],
            "brk_in": state["brk_in"],
            "last_milestone": state["last_milestone"],
            "elapsed_seconds": round(time.time() - state["start_time"], 2)
        }
    for p in [HEARTBEAT_WT, HEARTBEAT_SHARED]:
        try:
            with open(p + ".tmp", "w") as f:
                json.dump(data, f, indent=2)
            os.replace(p + ".tmp", p)
        except Exception as e:
            print(f"[supervisor] Error writing heartbeat to {p}: {e}", file=sys.stderr)

def heartbeat_worker():
    while True:
        with state_lock:
            if state["stop_heartbeat"]:
                break
        update_heartbeat_files()
        time.sleep(15)

def write_outcome(outcome_str, step, pc, c_bytes, uart_sha, irq_in, irq_out, da_in, brk_in, traps, panics, halt_reason, distinct_pcs, jiffies, init_grep, details):
    content = f"""outcome: {outcome_str}
step: {step}
pc: {pc}
uart_bytes: {c_bytes}
uart_sha256: {uart_sha}
irq_in: {irq_in}
irq_out: {irq_out}
da_in: {da_in}
brk_in: {brk_in}
traps: {traps}
dynamic_panics: {panics}
halt_reason: {halt_reason}
distinct_pcs_last_10k: {distinct_pcs}
jiffies: {jiffies}
init_grep: {init_grep}
terminal_details:
{details}
"""
    for p in [OUTCOME_WT, OUTCOME_SHARED]:
        try:
            with open(p, "w") as f:
                f.write(content)
            print(f"[supervisor] Wrote outcome to {p}")
        except Exception as e:
            print(f"[supervisor] Error writing outcome to {p}: {e}", file=sys.stderr)

def main():
    print(f"[supervisor] Starting T599 owned 300M runner from cold reset...")
    start_time = time.time()
    with state_lock:
        state["start_time"] = start_time
    
    cmd = [
        KERNEL_BIN,
        "--kernel", KERNEL_IMG,
        "--dtb", DTB_IMG,
        "--initrd", INITRD_IMG,
        "--max-steps", str(MAX_STEPS),
        "--console-output", GUEST_UART,
        "--dump-console"
    ]
    
    print(f"[supervisor] Executing: {' '.join(cmd)}")
    
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1
    )
    
    with state_lock:
        state["runner_pid"] = proc.pid
    
    def sig_handler(signum, frame):
        print(f"[supervisor] Signal {signum} received, terminating child {proc.pid}...", file=sys.stderr)
        try:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
        except Exception as e:
            print(f"[supervisor] Error stopping child: {e}", file=sys.stderr)

    signal.signal(signal.SIGTERM, sig_handler)
    signal.signal(signal.SIGINT, sig_handler)
    signal.signal(signal.SIGHUP, sig_handler)

    # Start heartbeat daemon
    hb_thread = threading.Thread(target=heartbeat_worker, daemon=True)
    hb_thread.start()
    
    current_step = 0
    current_pc = "0x0"
    console_bytes = 0
    irq_in = 0
    irq_out = 0
    da_in = 0
    brk_in = 0
    traps = 0
    dynamic_panics = 0
    halt_reason = "none"
    last_milestone = "booting"
    init_marker_found = False
    distinct_pcs_val = 0
    jiffies_val = "0"
    init_grep_val = "not found"
    
    # regex patterns
    prog_re = re.compile(r"\[progress\] step (\d+) \((\d+)M\), PC=(0x[0-9a-fA-F]+), console_bytes=(\d+), irq_in=(\d+), irq_out=(\d+)")
    done_re = re.compile(r"\[done\] Executed (\d+) steps\. PC=(0x[0-9a-fA-F]+), irq_in=(\d+), irq_out=(\d+), da_in=(\d+)(?:, brk_in=(\d+))?")
    halt_re = re.compile(r"\[halt\] Reason:\s*(.+)")
    panic_re = re.compile(r"\[\s*\d+\.\d+\] Kernel panic.*")
    marker_re = re.compile(r"GUEST_INIT_START|INITRD_SUCCESS|/init starting|Reached target|Welcome to|Run /init as init process")
    limit_re = re.compile(r"Step budget.*reached")
    distinct_re = re.compile(r"\[cap-rule\] distinct_pcs_last_10k:\s*(\d+)")
    jiffies_re = re.compile(r"\[cap-rule\] jiffies:\s*(.+)")
    init_grep_re = re.compile(r"\[cap-rule\] init_grep:\s*(.+)")
    
    milestone_lines = []
    recent_lines = []
    
    try:
        with open(PROGRESS_LOG, "w") as f_prog, open(RUNNER_LOG, "w") as f_run:
            for raw_line in iter(proc.stdout.readline, ''):
                line_str = raw_line.rstrip()
                
                # Bounded runner log
                recent_lines.append(line_str)
                if len(recent_lines) > MAX_LOG_LINES:
                    recent_lines.pop(0)
                f_run.write(line_str + "\n")
                f_run.flush()
                
                m_prog = prog_re.search(line_str)
                if m_prog:
                    current_step = int(m_prog.group(1))
                    current_pc = m_prog.group(3)
                    console_bytes = int(m_prog.group(4))
                    irq_in = int(m_prog.group(5))
                    irq_out = int(m_prog.group(6))
                    last_milestone = line_str
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)
                    with state_lock:
                        state["current_step"] = current_step
                        state["current_pc"] = current_pc
                        state["console_bytes"] = console_bytes
                        state["irq_in"] = irq_in
                        state["irq_out"] = irq_out
                        state["last_milestone"] = last_milestone
                
                m_done = done_re.search(line_str)
                if m_done:
                    current_step = int(m_done.group(1))
                    current_pc = m_done.group(2)
                    irq_in = int(m_done.group(3))
                    irq_out = int(m_done.group(4))
                    da_in = int(m_done.group(5))
                    if m_done.group(6):
                        brk_in = int(m_done.group(6))
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)
                    with state_lock:
                        state["current_step"] = current_step
                        state["current_pc"] = current_pc
                        state["irq_in"] = irq_in
                        state["irq_out"] = irq_out
                        state["da_in"] = da_in
                        state["brk_in"] = brk_in
                
                m_halt = halt_re.search(line_str)
                if m_halt:
                    halt_reason = m_halt.group(1)
                    traps += 1
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)
                
                m_marker = marker_re.search(line_str)
                if m_marker:
                    init_marker_found = True
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)
                    
                m_panic = panic_re.search(line_str)
                if m_panic:
                    dynamic_panics += 1
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)
                
                m_limit = limit_re.search(line_str)
                if m_limit:
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)

                m_distinct = distinct_re.search(line_str)
                if m_distinct:
                    distinct_pcs_val = int(m_distinct.group(1))
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)

                m_jiffies = jiffies_re.search(line_str)
                if m_jiffies:
                    jiffies_val = m_jiffies.group(1).strip()
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)

                m_init_grep = init_grep_re.search(line_str)
                if m_init_grep:
                    init_grep_val = m_init_grep.group(1).strip()
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    print(line_str, flush=True)

                if "[ram-search]" in line_str or "[cpu]" in line_str or "[brk]" in line_str or "sp=" in line_str or "ttbr" in line_str:
                    milestone_lines.append(line_str)
                    f_prog.write(line_str + "\n")
                    f_prog.flush()
                    
    except Exception as e:
        print(f"[supervisor] Exception reading runner stream: {e}", file=sys.stderr)
    
    ret_code = proc.wait()
    print(f"[supervisor] Runner process exited with code {ret_code}")
    
    # Stop heartbeat thread
    with state_lock:
        state["stop_heartbeat"] = True
        state["status"] = "completed"
    update_heartbeat_files()
    
    # Calculate UART info
    uart_bytes = os.path.getsize(GUEST_UART) if os.path.exists(GUEST_UART) else console_bytes
    uart_sha = sha256_file(GUEST_UART)
    
    # Determine outcome
    details = "\n".join(milestone_lines[-50:]) if milestone_lines else "\n".join(recent_lines[-50:])
    
    if halt_reason != "none":
        outcome_str = f"halted at step {current_step} PC {current_pc} UART {uart_bytes} bytes"
    elif init_marker_found:
        outcome_str = f"init-reached at step {current_step} PC {current_pc} UART {uart_bytes} bytes"
    elif current_step >= MAX_STEPS:
        outcome_str = f"completed-300M at step {current_step} PC {current_pc} UART {uart_bytes} bytes"
    else:
        outcome_str = f"stopped at step {current_step} PC {current_pc} UART {uart_bytes} bytes"
        
    write_outcome(
        outcome_str=outcome_str,
        step=current_step,
        pc=current_pc,
        c_bytes=uart_bytes,
        uart_sha=uart_sha,
        irq_in=irq_in,
        irq_out=irq_out,
        da_in=da_in,
        brk_in=brk_in,
        traps=traps,
        panics=dynamic_panics,
        halt_reason=halt_reason,
        distinct_pcs=distinct_pcs_val,
        jiffies=jiffies_val,
        init_grep=init_grep_val,
        details=details
    )
    print(f"[supervisor] Run finalized. Outcome: {outcome_str}")

if __name__ == "__main__":
    main()
