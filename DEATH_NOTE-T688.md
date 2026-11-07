# DEATH NOTE — T688

Lane: T688 — T686 kernel boot-hang diagnosis
Status: CLOSED — PASS (kernel boot hang root-caused)
UTC close: 2026-11-07T02:05:00Z
Workdir: /Users/Shared/wt-track688-kernel-boot-hang-diagnosis

## What ended

The lane was opened to root-cause the T686 rebuilt-kernel freeze at ~25 s,
after T687 proved the DTB, initramfs and system image halves were sound.
The hang is root-caused to the T686 Image's cold-function text placement.

First failing link: the T686 Image's cold-function section
(`.text.unlikely.`) was orphan-placed inside
`[__init_begin,__init_end]`; `free_initmem()` zeroed that range; the
`bl printk` at `free_reserved_area+420` landed on `udf #0x0`; the nested
`die()` self-deadlocked on `die_lock`.

The decisive live capture (`diagnostics/t688-lldb-all-cpus.log`, SHA-256
`db3cace1…`) shows CPU0 at
`queued_spin_lock_slowpath+132` (`0xffffff800810c7b4`) with
`die_lock` (`0xffffff80093b514c`) value `0x101`; the outer fault pt_regs
at `0xffffff800800bcd0` has `pc=printk+0` (`0xffffff8009232ca0`),
`lr=free_reserved_area+420` (`0xffffff80081ec4bc`); the inner fault
pt_regs at `0xffffff800800ba50` has `pc=__die+0` (`0xffffff80092310b0`),
`lr=die+92` (`0xffffff800808fdf8`); both PCs read `udf #0x0` live.
CPUs 2–4 are parked at PC/SP/LR 0; the guest reports
`smp: Brought up 1 node, 1 CPU`, so the lock holder is the outer `die()`
frame on CPU0 itself — a re-entrant self-deadlock.

The compiler-side cause: clang 21.0.0 (T686) emits `.text.unlikely.` with a
trailing dot (LLVM D92493 / rG3733463dbb58, 2020-12); Linux 4.19's
`TEXT_TEXT` matches only `.text.unlikely` exactly, so LLD orphan-placed the
section into the freeable init range. The canonical A/B Image
(`05c43917…`, clang 11.0.1) places `printk` below `_etext` and boots through
the same point to userspace with identical DTB/initramfs/system image.

## Final artifacts

- `findings-t688.md` — full adjudication and evidence.
- `outcome-t688.txt` — `PASS`.
- `progress-t688.txt` — Steps 0–6 record.
- `rebuild-recipe.md` — executable T689 kernel rebuild recipe (no rebuild
  performed in T688).
- `diagnostics/` — repro UART, A/B UART, lldb capture, kallsyms,
  free_initmem disassembly, symbol-placement A/B, clang section test,
  v4.19 linker-script excerpt, SHA-256 manifest.
- `flights/t688-flight-01/`, `-02/`, `-03/`, `-04/`, `t688-flight-ab/` —
  flight-end records and supervisor logs.

## What is not claimed

- No `sys.boot_completed=1`; the T686 boots never reached userspace, and
  the canonical A/B boot reached init but not a completed Android boot.
- No frame commit and no guest-origin display progress. The T686 kernel
  registers `fb0: simplefb registered!` but dies before userspace; the
  canonical A/B kernel has no `CONFIG_FB`, so its tmpfs `/dev/fb0` node is
  not a framebuffer.
- No kernel rebuild, DTB rebuild or initramfs rebuild in this lane.
- TEN-PASSES was not run: 4/4 T688 boots hung; 7 total across T687/T688.
  No 10/10 reproducibility claim is made.
- The exact T686 `.config` (SHA `502e79c3…`) is missing locally; the
  recorded delta in `findings-t686.md` was used and this gap is named.
- No push, no commit, no second agent.

## Handoff

T689 should execute `rebuild-recipe.md`:

1. Route A (config-only test): restore `CONFIG_LTO=y`, `CONFIG_THINLTO=y`,
   `# CONFIG_LTO_NONE is not set`, `CONFIG_LTO_CLANG=y` in the T686 config
   and rebuild; gate on `printk`/`__die`/`panic`/`dump_stack`/`show_regs`
   all below `_etext`.
2. If the gate fails, Route B (recommended): add
   `.text.unlikely.*`/`.text.hot.*`/`.text.unknown.*`/`.ref.text.*` globs
   to `TEXT_TEXT` in `include/asm-generic/vmlinux.lds.h`, keep the T686
   config, rebuild with the exact T686 clang 21 command.
3. Route C fallback: canonical clang 11.0.1 / LLD 11.0.1 with the canonical
   config plus only the four framebuffer lines.

Do not touch the verified `modified.dtb` (`e83a6437…`),
`base-t683-fix.cpio.gz` (`8fb3c9de…`), `system.img` (`98fc0b02…`) or
Composer ELF (`ddc1e4dd…`). Boot with the T688 supervisor shape; pass =
`Freeing unused kernel memory` then `init: init first stage started!`, no
uart-pl011 silence, no `udf`/Oops/panic, and `fb0: simplefb registered!`
still present.
