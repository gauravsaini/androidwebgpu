# DEATH_NOTE — T682

- Lane: T682 — Composer 2.1 flight: install the bionic vendor ELF, prove a frame commits (Mac)
- Date of death: 2026-11-07
- Cause of death: agent wedged — opencode PID 93981 produced zero observable output after 16:23:48 on 2026-11-04 (log frozen at 1,592,922 B, no findings/outcome/death note, no fix image, no flight) for ~62 hours, far past the 2–3h worst-case estimate. Killed by the driver on the user's Option 1 decision (2026-11-07 06:41 AEDT): TERM at 06:45:42 AEDT, verified gone 06:45:45 AEDT together with its opencode serve child 93983.
- Worktree: /Users/Shared/wt-track682-composer-2-1-flight
- Verdict: FAIL (outcome-t682.txt)

## What this lane proved

Nothing about the guest. The mission (vendor.img CPIO-member replacement carrying the T681 ELF ddc1e4dd, ONE fix initramfs on base afb2991f, 1200s QEMU 11.1.2 flight, guest-origin frame-commit proof, first-cycle bar) was never executed: the agent stalled before filing any Step 1 output.

## Open items carried to T683

- Verify all staged input SHAs and the base CPIO member ramdisk-images/vendor.img (134,217,728 B).
- Build the replacement ext4 vendor image with the ELF at /vendor/bin/hw/android.hardware.graphics.composer@2.1-service.
- Build exactly ONE fix initramfs, record its SHA.
- Fly 1200s, prove frame commit by guest-origin evidence, hold the first-cycle bar.
- Adjudicate the T681 runtime risks: vendor sepolicy for /dev/mem, simplefb physical/stride, LIBC_R linker requirement.
- sys.boot_completed was NOT observed in T682 (no flight).

Full evidence: findings-t682.md (reconstructed by reviewer 2026-11-07 — the agent filed nothing).
