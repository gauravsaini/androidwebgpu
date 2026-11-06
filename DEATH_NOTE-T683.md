# DEATH NOTE — T683

Verdict: FAIL

The lane built one replacement vendor ext4 image and one fix initramfs with the verified T681 Composer 2.1 ELF at `/vendor/bin/hw/android.hardware.graphics.composer@2.1-service`. A full 1200-second guest window completed under QEMU 11.1.2.

The guest mounted the embedded vendor image and executed the Composer process, but UART evidence stops at binder traffic. No guest-origin successful present or fb0 frame-commit evidence was captured. The first-cycle boot bar also failed: at guest uptime 1175.91 seconds, the T642 probe still reported missing user DE/CE, no system_server PID, and empty `boot_completed=`. No system Watchdog kill or system-process FATAL was observed.

The first QMP-enabled launch failed before guest boot because the worktree Unix socket bind was denied; that exact error is retained under `artifacts-t683/flight/`. The repository's T670 no-QMP supervisor then completed the full guest window. The final detailed evidence and hashes are in `findings-t683.md`; the concise outcome is in `outcome-t683.txt`.
