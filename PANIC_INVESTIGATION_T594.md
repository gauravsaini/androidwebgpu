# Investigation of T594 dynamic_panics: Root Cause, String Artifact Analysis, and Architectural Classification

## Status Token
DYNAMIC_PANICS_ANALYSIS_OK

## 1. Context & Prompt Question
In Track T594 (`wt-track594-brk-vector`), `outcome-t594.txt` recorded:
```text
outcome: completed-150M at step 150000000 PC 0xffffff8008c62338 UART 13456 bytes
traps: 0
dynamic_panics: 5
halt_reason: none
```
Task 5 mandates: "Investigate T594's 5 dynamic_panics: read `/Users/Shared/wt-track594-brk-vector/guest-uart.log`, characterize each panic (benign driver-probe failure vs real wall). If a panic is a NEW engine wall, name it with step/PC evidence."

## 2. Source Code & Telemetry Audit
We audited `wt-track594-brk-vector/scripts/supervisor-t594.py`, `run_kernel.rs`, and the terminal stdout recorded in `outcome-t594.txt`.

### A. Supervisor Metric Counter
In `scripts/supervisor-t594.py`:
```python
panic_re = re.compile(r"Kernel panic.*")
...
m_panic = panic_re.search(line_str)
if m_panic:
    dynamic_panics += 1
```
`panic_re.search(line_str)` matches ANY appearance of the substring `Kernel panic` in any line emitted to stdout by the runner process `run_kernel`.

### B. Output Stream Breakdown
During the T594 cold boot to 150,000,000 steps, `proc.stdout` received lines from two sources:
1. Live UART console dump as forwarded by `run_kernel`.
2. Post-execution RAM panic scanner (`[ram-search]`).

Let us trace every line containing `Kernel panic`:
- **Occurrence 1 (Guest UART Console at Step 145,410,118)**:
  ```text
  [    8.042758] Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b
  ```
  This is emitted by `kernel/panic.c` following PID 1 exit.
- **Occurrence 2 (RAM Scanner - rodata string literal 1)**:
  ```text
    Found 'Kernel panic - not syncing' at PA 0x412671ca (rodata=true)
  ```
  Matched `Kernel panic.*` via `panic_re.search`.
- **Occurrence 3 (RAM Scanner - rodata string literal 2)**:
  ```text
    Found 'Kernel panic - not syncing' at PA 0x412a9bd9 (rodata=true)
  ```
  Matched `Kernel panic.*` via `panic_re.search`.
- **Occurrence 4 (RAM Scanner - dynamic RAM log buffer)**:
  ```text
    Found 'Kernel panic - not syncing' at PA 0x416945b4 (rodata=false)
  ```
  Matched `Kernel panic.*` via `panic_re.search`. This is the printk logbuf entry storing the text of Occurrence 1 in RAM.
- **Occurrence 5 (RAM Scanner - detection summary line)**:
  ```text
    [PANIC DETECTED] 1 dynamic occurrences of 'Kernel panic - not syncing'
  ```
  Matched `Kernel panic.*` via `panic_re.search`.

**Conclusion on count**:
The "5 dynamic_panics" reported by `supervisor-t594.py` is an exact arithmetic artifact of the unanchored regex `re.compile(r"Kernel panic.*")` matching:
- 1 real guest panic in the console log.
- 4 lines output by the RAM search tool reporting on the static string literals and RAM logbuf copy.

## 3. Characterization of the Real Panic: Real Wall vs Benign Driver Probe
There is exactly **one** distinct panic in the guest run.

### Telemetry Evidence:
- **Step**: `145,410,118`
- **Faulting PC**: `0xffffff80082e0f4c` (`__cfi_check_fail+0x18/0x1c`)
- **Faulting Instruction**: `brk #0x800` (`0xd4210000`)
- **Preceding Subsystem**: `cfg80211` loading compiled-in X.509 certificates for the regulatory database.
- **Kernel Oops**:
  ```text
  [    8.005877] cfg80211: Loading compiled-in X.509 certificates for regulatory database
  [    8.008095] CFI failure (target: sha256_block_data_order.cfi_jt+0x4/0x8):
  [    8.009872] ------------[ cut here ]------------
  [    8.010365] kernel BUG at kernel/cfi.c:32!
  [    8.010886] Internal error: Oops - BUG: 0 [#1] PREEMPT SMP
  [    8.011910] Process swapper/0 (pid: 1, stack limit = 0x(____ptrval____))
  [    8.014535] pc : __cfi_check_fail+0x18/0x1c
  [    8.015171] lr : __cfi_check_fail+0x18/0x1c
  [    8.015630] sp : ffffff800800b850
  ...
  [    8.026566] Call trace:
  [    8.027150]  __cfi_check_fail+0x18/0x1c
  [    8.027778]  __cfi_check+0x2f8c8/0x31508
  [    8.028459]  sha256_update_neon+0x2d8/0x320
  [    8.029148]  sha256_finup_neon+0x1f8/0x430
  [    8.029852]  shash_digest_unaligned+0xdc/0x128
  [    8.030507]  crypto_shash_digest+0xd4/0x13c
  [    8.031202]  x509_get_sig_params+0x148/0x184
  [    8.031881]  x509_cert_parse+0x134/0x200
  [    8.032610]  x509_key_preparse+0x2c/0x1a4
  [    8.033333]  asymmetric_key_preparse+0x68/0xd8
  [    8.033980]  key_create_or_update+0x1b0/0x4ec
  [    8.034612]  load_keys_from_buffer+0xd0/0x120
  [    8.035248]  load_builtin_regdb_keys+0x6c/0x74
  [    8.035920]  regulatory_init_db+0x3c/0xc4
  [    8.036559]  do_one_initcall+0x1e8/0x3a0
  [    8.037210]  do_initcall_level+0x188/0x1b0
  [    8.037857]  do_basic_setup+0x34/0x50
  [    8.038503]  kernel_init_freeable+0xb4/0x134
  [    8.039243]  kernel_init+0x18/0x2a8
  [    8.039833]  ret_from_fork+0x10/0x18
  [    8.040562] Code: 910003fd 90007bc0 91167800 97fcd68e (d4210000) 
  [    8.041151] ---[ end trace 0fd7a906c381deec ]---
  [    8.041687] note: swapper/0[1] exited with preempt_count 1
  [    8.042758] Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b
  [    8.044934] Rebooting in 5 seconds..
  ```

### Analysis:
1. Is it a benign driver probe failure?
   **NO**. It kills PID 1 (`swapper/0` executing `kernel_init`). When PID 1 dies, the kernel cannot proceed to userspace (`/init`) and immediately calls `panic("Attempted to kill init!")`.
2. Is it a NEW engine wall?
   **NO**. The engine behavior is 100% architecturally compliant:
   - When the instruction `brk #0x800` was executed at `0xffffff80082e0f4c`, the engine did NOT crash or trap (`traps: 0`).
   - It cleanly vectored to `VBAR_EL1 + 0x200` (`el1h_sync`) with ESR_EL1=`0xf2000800`.
   - The Linux exception handler executed cleanly, printed the trace, and called panic.
   - The engine continued stepping cleanly through kernel panic and reboot delay handlers up to 150M.
3. Wall Classification:
   - **WALL-TYPE-TAG**: `guest` (Type-1 guest kernel Clang Forward-Edge CFI jump table mismatch on assembly symbol `sha256_block_data_order`).
   - **Rust par bhi lagu**: `haan` (affects both Rust emulator and any hardware/QEMU running this exact kernel binary without permissive CFI).
