Verdict: FAIL

# Track T687 — DTB framebuffer flight (recovery)

**First failing link:** the guest boot never leaves the kernel. The T686 rebuilt
kernel prints the expected simplefb bind, then freezes deterministically at
guest ~24–26 s right after `uart-pl011 9000000.pl011: no DMA platform data`,
before `Freeing unused kernel memory`, before any init, and before any Android
or Composer process. A control boot with the unmodified base DTB
(`minimal-virt-fixed.dtb`) reproduces the identical hang, so the staged
`modified.dtb` is not the cause. With no userspace there is no Composer
service, no `/dev/mem` mmap attempt, no guest-origin present/fb0 evidence, and
no first-cycle boot bar.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest kernel and guest Composer 2.1 path are
required regardless of implementation language; the wall is inside the guest
kernel, not the DTB, the initramfs, or the harness.
sys.boot_completed: not observed (cycle 1; the flight-04 UART has no
`boot_completed=` line, no userspace init, and the guest stopped logging at
25.117 s).

## Step 0 — Mac disk gate

PASS. `df -h /System/Volumes/Data` reported 18 GiB available, above the 8 GiB
gate.

## Step 1 — Input verification

PASS. All eight staged inputs plus the QEMU binary were re-verified by SHA-256
and size. The T683 fix initramfs still contains the CPIO member
`ramdisk-images/vendor.img` at exactly 134,217,728 bytes.

| Input | SHA-256 | Size |
|---|---|---:|
| `guest-inputs/t686-Image` (flown) | `66d3fc1b6832a6caa66cd26bbb4417c9ef1f7a6c017dfc44a93063b8ff2813dc` | 20,126,208 B |
| `guest-inputs/Image` (canonical, reference only, not booted) | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B |
| `guest-inputs/minimal-virt-fixed.dtb` | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` | 1,465 B |
| `guest-inputs/system.img` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B |
| `guest-inputs/base-t683-fix.cpio.gz` | `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70` | 958,780,579 B |
| `guest-inputs/vendor-t683.img` | `bed3d6e989e16c8bbd7dd7e0208753c3a0d8e3407c96d291cd778ff43a7daaf1` | 134,217,728 B |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B |
| `modified.dtb` (flown) | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B |
| `qemu-system-aarch64` 11.1.2 (resolved binary) | `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869` | 31,498,864 B |

The dead agent's Step 1 line is preserved in `progress-t687.txt`; the recovery
re-check matched every value again.

## Step 2 — DTB verification (adopted, independently re-hashed)

PASS. The recovery brief directs adoption of the dead agent's Step 2 work. I
re-verified the staged `modified.dtb` SHA-256 and re-read the preserved
decompiled tree. The relevant nodes are exactly as adjudicated:

```dts
chosen {
    linux,initrd-start = <0x01 0x10000000>;
    linux,initrd-end = <0x01 0x4925d4a3>;
    framebuffer@5f000000 {
        compatible = "simple-framebuffer";
        reg = <0x00 0x5f000000 0x00 0x00384000>;
        memory-region = <&framebuffer_reserved>;
        width = <1280>;
        height = <720>;
        stride = <5120>;
        format = "a8b8g8r8";
        status = "okay";
    };
};
```

with `/reserved-memory/framebuffer@5f000000` marked `no-map`. The guest kernel
does bind this node (see Step 5), so the DTB half of the lane is solid.

## Step 3 — Initramfs decision

PASS, unchanged. The flight used `guest-inputs/base-t683-fix.cpio.gz` exactly
as verified (`8fb3c9de…`, 958,780,579 B). No rebuild, no member edit. The dead
agent's Step 1 CPIO listing confirmed `ramdisk-images/vendor.img` at
134,217,728 B. No second initramfs was built in this recovery.

## Step 4 — Flight attempt 4

PASS as a flight, FAIL as a boot. The flight used the T670 no-QMP supervisor
ported to `scripts/supervisor_t687.py` with only T687 constants and paths
changed. It was launched fully detached (setsid + nohup, stdin from
`/dev/null`, output redirected), so it survived this agent:

- supervisor PID 96149 (detached, PPID 1)
- QEMU PID 96152 (detached, PPID 1)
- flight directory `flights/t687-flight-04/`
- command shape byte-identical to flight-03 except the output directory
  (verified by diff; full command in `flights/t687-flight-04/qemu-command.txt`)
- kernel `guest-inputs/t686-Image` (`66d3fc1b…`)
- DTB `modified.dtb` (`e83a6437…`)
- loader initrd `guest-inputs/base-t683-fix.cpio.gz` at `0x110000000`, force-raw
- `guest-inputs/system.img` as read-only virtio-blk
- QEMU 11.1.2, `-machine virt,gic-version=2,virtio-mmio-transports=1`,
  `-cpu cortex-a57`, `-m 8192`, `-smp 4`, `-display none`, `-monitor none`,
  `-no-reboot`, and the flight-03 `-append` line

Result recorded in `flights/t687-flight-04/flight-end.json`:

- host elapsed: 1503.31 s (supervisor stopped QEMU at the 1500 s host target)
- guest log maximum: 25.117 s
- QEMU exit code: 0
- UART: 185 lines, 12,544 B,
  SHA-256 `d8b267e12d058d49964ce39f8a65ea50e638abd41cf58791b00f73ac4e58e681`
- no separate logcat export; the supervisor captures the guest console only

The guest printed simplefb, then froze on the last line for the remaining
~1478 host seconds. There was no panic, no oops, and no reset. The supervisor
sent the planned SIGTERM; QEMU's stderr records signal 15.

## Step 5 — Adjudication

### Simplefb bind

PASS. The driver bound the DTB node and registered a framebuffer device.
Flight-04 UART lines 95–97:

```text
[    0.315260] simple-framebuffer 5f000000.framebuffer: framebuffer at 0x5f000000, 0x384000 bytes, mapped to 0x(____ptrval____)
[    0.315665] simple-framebuffer 5f000000.framebuffer: format=a8b8g8r8, mode=1280x720x32, linelength=5120
[    0.316513] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!
```

This proves the DTB/kernel binding works. It does not prove any guest process
opened, mapped, or wrote fb0.

### Userspace and service

FAIL. The UART ends at line 185 with no transition to userspace:

```text
[   25.117289] uart-pl011 9000000.pl011: no DMA platform data
```

The log contains no `Freeing unused kernel memory`, no `Run /early-ramdisk`,
no init/zygote/system_server, no `vendor.hwc-2-1`, no Composer process, and no
SurfaceFlinger line (verified by grep: all zero). The T681 Composer ELF was
never executed, so there is no linker, SELinux, or runtime result to quote.

### /dev/mem path

NOT REACHED. Because the guest never entered userspace, the T681 service never
ran and neither of its expected lines (`simplefb: mapped 0x%lx %ux%u ...` or
`simplefb: open /dev/mem failed: %s`) appears. This is downstream of the boot
hang, not a `STRICT_DEVMEM` result.

### Present path

FAIL, no guest-origin evidence. There is no IComposer registration, no
`present`/`presentOrValidate` success, no SurfaceFlinger present line, and no
guest-side fb0 write. Snapshot bytes were not used and are not proof under the
standing rules. The display wall cannot be assessed until the guest boots.

### First-cycle boot bar

FAIL. No unlock, no user CE, no `sys.boot_completed=1`; the guest hangs in the
kernel before init. Cycle 1 never started. No Watchdog kill or system-process
FATAL exists because no system process was ever created.

### Regression check

No T664–T686 user-space artifact was changed by this lane: the initramfs,
vendor image, system image, ACL/CE setup, media.c2 arrangement, and Composer
ELF are byte-identical to their staged, previously adjudicated versions. The
DTB is the staged T687 artifact and it binds.

The regression is in the kernel itself: the T686 rebuilt Image does not reach
userspace. The canonical kernel reached 1200 guest seconds in the T683 flight
(UART `73cfd8d6…`, 50,043 lines), while this Image stops at ~25 s. The T686
build was certified PASS for producing a valid ARM64 Image, but it was never
flown; this lane is that flight, and the boot result is FAIL. That is the
honest first failing link.

## Diagnostics — isolating the hang to the T686 kernel

All diagnostics used the unchanged initramfs and the same QEMU machine shape.
Logs are preserved in `flights/t687-diagnostics/`.

1. **Flight-03 re-read (adopted).** 186 lines, 12,641 B, SHA
   `677a303f…`, simplefb bind at line 97, no inflate_fast/oops/panic, last
   guest line `[23.988056] uart-pl011 ... no DMA platform data`. Flight-03 was
   killed by the agent's death, not by a guest failure, and it already showed
   the same 25 s-class stop.
2. **Speed probe** (`t687-speedprobe.log`): same command, 185 lines, guest
   stopped at 25.758 s on the identical last line.
3. **Base-DTB control** (`t687-diag-basedtb.log`): same kernel and initramfs
   with `minimal-virt-fixed.dtb`. 182 lines, zero `simplefb` lines (that DTB
   has no framebuffer node), guest stopped at 24.390 s on the identical last
   line. This is the decisive control: the hang does not require
   `modified.dtb`, so the DTB is not the failing link.
4. **initcall_debug** (`t687-initcall.log`): 713 `initcall ... returned`
   messages; the last is `alsa_sound_last_init+0x0/0x8c returned 0 after 261
   usecs` at 26.220829 s, immediately followed by the `uart-pl011` line at
   26.223676 s and the freeze. Every initcall returned; the hang is after
   `do_basic_setup()`, in the kernel's path to userspace, not inside a stuck
   initcall.
5. **Register capture at the hang** (lldb over QEMU's gdbstub): CPU0 is in
   EL1h inside the qspinlock slow path. Captures put PC at
   `0xffffff800810c7a4` (the `ldxr w10, [x8]` loop) and later at
   `0xffffff800810c8b4` / `0xffffff800810c8f8`; LR `0xffffff800810c780` then
   `0xffffff8008b35370`; SP `0xffffff800800b8e0` then `0xffffff800800ad80`.
   The lock word at the first capture is `x8 = 0xffffff80093b514c`, a BSS
   address (`Image` file end is `0x1331a00` from the load base), i.e. a
   runtime-initialized lock. This is a spin, not a waitqueue sleep, which is
   why the guest looks alive to the host but never advances.
6. **Forced-fault oops runs.** A gdbstub PC-to-0 fault while the guest was
   still unpacking initramfs (guest 16.19 s) produced a fully symbolized oops
   (`pc: (null)`, `lr: get_page_from_freelist+0xaa8/0xad8`, call trace through
   `shmem_write_begin` → `__alloc_pages_nodemask`), proving the method and the
   guest's kallsyms work. The same fault applied at the 25 s freeze produced
   no console output at all; the UART stayed on the `uart-pl011` line. The
   printk/console path is therefore unavailable at the hang, so the hang's own
   call trace could not be emitted. I do not claim which specific lock is
   held; I claim the CPU is spinning in the qspinlock slow path and that the
   console is blocked.

Kernel lineage context (not a root-cause claim): the flown Image identifies as
`Linux version 4.19.130-g4da740c10 ... clang version 21.0.0 ... #2 SMP PREEMPT`.
Compared with the canonical config, the rebuild has `CONFIG_LTO_NONE=y` (LTO
off), `# CONFIG_SHADOW_CALL_STACK is not set`, no `CONFIG_CFI`, and an empty
`CONFIG_UNUSED_KSYMS_WHITELIST`, while adding `CONFIG_DEVMEM=y`,
`CONFIG_STRICT_DEVMEM=y`, `CONFIG_FB=y`, and `CONFIG_FB_SIMPLE=y`. The
framebuffer capability was added successfully; the boot hang is a property of
this rebuilt kernel that the T686 build-only lane could not see. Root-causing
it is the next lane's job; the brief forbids rebuilding the DTB or initramfs
here, and the base-DTB control already excludes those inputs.

## Flight-03 evidence (lane record)

Flight-03 (`flights/t687-flight-03/uart.log`, 186 lines, 12,641 B, SHA
`677a303f…`) bound simplefb at line 97, showed no inflate_fast fault, no oops,
no panic, and last guest time 23.988 s. It ended because the agent died, not
because the guest failed. Attempt 4 reproduced the same stop and completed the
full 1500 s host window, converting flight-03's truncated observation into a
deterministic, supervised hang.

## What would make T687 PASS

- Fix or replace the T686 rebuilt kernel so the guest reaches userspace
  (`Freeing unused kernel memory`, early-ramdisk, init) and survives the
  first-cycle boot bar.
- Then rerun this exact flight and prove guest-origin present/fb0 evidence and
  the `/dev/mem` outcome.
- The DTB (`e83a6437…`) and the unchanged initramfs are already verified and
  can be reused as-is.

## Not claimed

- No frame commit, no Composer service execution, no `/dev/mem` result.
- No `sys.boot_completed`, unlock, or CE observation.
- No kernel root cause beyond the spinlock/console evidence above.
- No TEN-PASSES reproducibility claim. The hang is deterministic across six
  boots in this lane (flight-03, flight-04, speed probe, base-DTB control,
  initcall_debug, and the two forced-fault boots), but no pass was
  reproduced because none occurred.
- No claim that `guest-inputs/Image` (canonical) was booted; it was reference
  only.
