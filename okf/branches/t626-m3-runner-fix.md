# T626 — M3 runner fix (Chain B, engine)

## Objective

Make `run_kernel` continue past the kernel's `Run /init as init process` printk and stop only when guest UART bytes contain an initramfs PID-1 marker. Prove the marker on one cold boot using the pinned T612 inputs.

## Hypothesis

The T612 runner stops at a kernel message before userspace `/init` runs. Moving the stop predicate to distinct guest PID-1 marker bytes should let the same cold boot continue until the initramfs process speaks on UART.

## Implementation

- Runner fix commit: `1af433421787b1023e886273216844be24efd577`.
- Abort-vector evidence commit: `051c85c`.
- Regression: kernel handoff bytes are rejected; the pinned T612 marker and `M3-PID1 ` marker are accepted.
- RED: `only_guest_pid1_uart_bytes_stop_the_runner` failed on the kernel handoff assertion before the predicate fix.
- GREEN: focused `run_kernel` marker test passed after the fix.

## Input identity

- Kernel Image: `/Users/Shared/guest-inputs/Image`, SHA-256 `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04`.
- DTB: `/Users/Shared/guest-inputs/minimal-virt-fixed.dtb`, SHA-256 `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0`.
- T612 initramfs: `/Users/Shared/guest-inputs/initramfs.cpio`, SHA-256 `49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38`.
- These are read-only pinned inputs. The preflight matched every expected SHA-256. No replacement initramfs has been created.

## Flight status

Cold boot is running under the T626 supervisor in a persistent managed execution session. It writes a heartbeat every 15 seconds. OS-detached launch attempts were blocked by the execution sandbox (background children were reaped, tmux could not create its socket, and `launchctl submit` was rejected); Terminal automation was also denied. Raw flight output stays outside `okf/` in `/Users/Shared/t626-flight/`; the UART capture is gzip-compressed at flight end. Final marker, futex vector, trap inventory, runner hash, and verdict will be recorded here and in `/Users/Shared/codex-track626-m3-runner-fix.md`.
