# Gates: Track T626 — M3 runner fix (Chain B, engine)

## Scope

Fix `run_kernel` so the kernel's `Run /init as init process` printk cannot satisfy the M3 PID-1 gate. Run one cold boot using the pinned T612 Image, DTB, and initramfs. Record the guest-emitted PID-1 UART bytes, exact futex abort vector, trap inventory, and artifact hashes.

## Gates

- [x] G626-1: Work in isolated `t626-m3-runner-fix` worktree; leave `/Users/Shared/wt-track612-and-rd31-sp` untouched.
- [x] G626-2: Red-green runner regression. RED: kernel handoff line matched the old predicate. GREEN: focused marker test passes for the pinned T612 marker and `M3-PID1 `, while rejecting the kernel line.
- [x] G626-3: Build optimized `run_kernel` on runner revision `9d55dc244110919322dae62ea1487a272d693a93`; binary SHA-256 `93119a968bcbd25a7f1be94a73a43f80b3b32526d727f97e1ecd9e5e8018d75e`.
- [x] G626-4: Preflight canonical input hashes before cold boot; all match.
- [ ] G626-5: Capture a post-handoff PID-1 marker from guest UART, with guest step and time.
- [ ] G626-6: Reproduce the futex vector exactly: ESR `0x96000005`, FAR `0x0`, ELR `0xffffff80082591e4`, then observe Linux's exception-table fixup PC `0xffffff8008c85a8c` after ERET. This is the resume PC, not saved `x30` LR.
- [ ] G626-7: Record DA/BRK/trap inventory, gzip the raw UART log at flight end, and write `/Users/Shared/outcome-t626.txt`.
- [ ] G626-8: Deliver findings at `/Users/Shared/codex-track626-m3-runner-fix.md` and record the final result in `TIMELINE.md` and `okf/branches/t626-m3-runner-fix.md`.

## Pinned inputs

- Image: `/Users/Shared/guest-inputs/Image` — `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04`
- DTB: `/Users/Shared/guest-inputs/minimal-virt-fixed.dtb` — `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0`
- T612 initramfs: `/Users/Shared/guest-inputs/initramfs.cpio` — `49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38`

Flight artifacts are kept under `/Users/Shared/t626-flight/`; they remain outside the curated `okf/` branch notes. Attempt 1 was stopped at 52,000,000 steps after identifying that `resume_lr` is not the expected Linux fixup PC; its UART log is gzip-compressed and its outcome is preserved at `/Users/Shared/t626-flight-attempt1/`. It did not establish that the T612 initramfs is silent. The final cold boot is running on the corrected runner, with the supervisor writing heartbeats every 15 seconds.

The supervisor is a persistent managed execution session because the sandbox reaped shell-detached children and blocked tmux sockets and `launchctl submit`. OS-detached supervision could not be established in this environment.
