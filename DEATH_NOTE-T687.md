# DEATH NOTE — T687

Lane: T687 — DTB framebuffer flight (recovery)
Status: CLOSED — FAIL (guest kernel boot hang)
UTC close: 2026-11-07T00:31:15Z
Workdir: /Users/Shared/wt-track687-dtb-framebuffer-flight

## What ended

The lane was opened to prove the T686 rebuilt kernel consumes the staged
simple-framebuffer DTB and presents a frame from the guest. The staged inputs
all verified, the DTB binds, and flight attempt 4 ran the full supervised
window — but the guest kernel freezes at ~25 s, before userspace.

First failing link: the T686 rebuilt kernel (`guest-inputs/t686-Image`,
`66d3fc1b…`, 20,126,208 B). After `fb0: simplefb registered!` and 713
returned initcalls, CPU0 spins in the qspinlock slow path and the console
stops at `uart-pl011 9000000.pl011: no DMA platform data`. A control boot with
`minimal-virt-fixed.dtb` reproduces the identical hang, so the staged T687 DTB
and the unchanged initramfs are excluded as causes.

Flight attempt 4 (supervisor PID 96149, QEMU PID 96152, both detached):
host 1503.31 s, guest 25.117 s, QEMU exit 0, UART 185 lines / 12,544 B /
SHA-256 `d8b267e12d058d49964ce39f8a65ea50e638abd41cf58791b00f73ac4e58e681`.
No init, no Composer, no `/dev/mem` attempt, no guest-origin present/fb0
evidence, no `sys.boot_completed`.

## Final artifacts

- `findings-t687.md` — full adjudication and diagnostics.
- `outcome-t687.txt` — `FAIL`.
- `progress-t687.txt` — Steps 0–5 record (appended, dead-agent lines intact).
- `flights/t687-flight-04/` — official attempt-4 capture (`uart.log`,
  `preflight.json`, `flight-end.json`, `qemu-command.txt`, `supervisor.log`).
- `flights/t687-diagnostics/` — speed probe, base-DTB control, initcall_debug,
  PC capture, and both forced-fault oops logs.
- `scripts/supervisor_t687.py` — T670-derived no-QMP supervisor.

## Not claimed

- No frame commit and no successful present; the display wall was never
  reached.
- No `/dev/mem` mmap result; the service never ran.
- No first-cycle boot bar; cycle 1 never started.
- No kernel root cause beyond the spinlock/console evidence. The T686 build
  was certified valid but never flown; this lane is its first boot and it
  fails before userspace.
- No push, no commit, no second agent.

## Handoff

T688 or the kernel lane should root-cause the rebuilt kernel's boot hang
(suspects include the LTO/CFI/shadow-call-stack-disabled clang 21 build and
its runtime lock initialization). Once a kernel reaches userspace, reuse the
already-verified `modified.dtb` (`e83a6437…`) and `base-t683-fix.cpio.gz`
(`8fb3c9de…`) unchanged and rerun this flight; the DTB bind and the flight
harness are proven.
