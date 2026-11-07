# DEATH NOTE — T696

Lane: Track T696 — Route B rebuild: non-LTO + linker-script globs (Mac agyx, build on box)
Status: CLOSED — PASS (Route B kernel rebuilt and flown to userspace target window with fb0 registered)
UTC close: 2026-11-07T23:25:00Z
Worktree: /Users/Shared/wt-track696-routeb-rebuild-agyx
Branch: t696 (from T695 commit a5b1393)

## What ended

The lane was opened to execute Route B from `rebuild-recipe.md`: the deterministic fix combining non-LTO (`CONFIG_LTO_NONE=y`, Clang 21) with ONE linker-script edit teaching the Linux 4.19 linker script Clang's section suffix globs in `TEXT_TEXT` (`include/asm-generic/vmlinux.lds.h`).

The lane succeeded completely:
1. Rebuild: The single linker-script edit was applied to `include/asm-generic/vmlinux.lds.h` in the box tree `/home/muse/t686-kernel/src`. The rebuild with Clang 21.0.0 and LLD 21.0.0 completed in 26 seconds with exit code 0. Image SHA-256: `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` (20,126,208 B), copied to Mac `guest-inputs/routeB-Image`.
2. Gate 3a (symbol gate): PASS. All key functions are strictly below `_etext` `0xffffff8008b50000` (`printk` 0xffffff8008b30a40, `__die` 0xffffff8008b2ee50, `panic` 0xffffff8008b2f4dc, `dump_stack` 0xffffff8008b37ecc, `show_regs` 0xffffff8008b2edd8, `free_initmem` 0xffffff80080a4200). Defect 1 (T686 cold section orphaning into init) is eliminated.
3. Gate 3b (initcall gate): PASS. `__initcall_start` `0xffffff8009250b48` < `__initcall_end` `0xffffff800925165c` (2,836 bytes, 354 entries across all 8 initcall levels). Count of suffixed initcall sections is exactly 0. Defect 2 (ThinLTO initcall orphaning) is eliminated.
4. Step 4 flight (`t696-flight-01`): PASS. Flown with `modified.dtb` (`e83a6437…`), `base-t683-fix.cpio.gz` (`8fb3c9de…`), and canonical `system.img` (`98fc0b02…`) under QEMU 11.1.2:
   - `simple-framebuffer ... fb0: simplefb registered!` observed at guest 0.323750 s.
   - `Freeing unused kernel memory: 960K` observed at guest 27.151864 s.
   - `init: init first stage started!` observed at guest 30.236134 s.
   - `init: init second stage started!` observed at guest 36.272276 s.
   - Reached the full 1200 s guest target window: guest 1200.991 s (host 1202.89 s, QEMU exit code 0, 51,156 lines, UART SHA-256 `7d6d4107…`).
   - Zero hang at 25 s, zero `udf #0x0`, zero Oops, zero panic.
5. Step 5 A/B control flight (`t696-flight-ab`): PASS. Canonical Image (`05c43917…`) reached `Freeing unused kernel memory: 1024K` at guest 26.114348 s and `init: init first stage started!` at guest 29.243520 s, running to guest 242.623 s (host 245.53 s, exit 0, 12,260 lines, UART SHA-256 `39466229…`), proving the Mac QEMU harness 100% sound.

## Final artifacts

- `findings-t696.md` — full adjudication, gate outputs, and preserved evidence citations.
- `outcome-t696.txt` — single word `PASS`.
- `progress-t696.txt` — Steps 0–6 record.
- `heartbeat.json` — updated timestamp and status.
- `guest-inputs/routeB-Image` — built Route B kernel Image (`3f45272f…`).
- `scripts/supervisor_t696.py` and `scripts/launch_t696.py` — adapted flight supervisor and launcher.
- `flights/t696-flight-01/` — Route B flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`).
- `flights/t696-flight-ab/` — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`).
- `diagnostics/`:
  - `t696-nm-symbols.txt` (Gate 3a symbols)
  - `t696-initcall-table.txt` (Gate 3b initcall table symbols)
  - `t696-routeB-git-diff.txt` (exact git diff of linker script edit)
  - `t696-build.log` (box build log)
  - `t696-routeB-uart.log` (copy of flight-01 UART)
  - `t696-canonical-ab-uart.log` (copy of flight-ab UART)

## What is not claimed

- No `sys.boot_completed=1`: the 1200 s guest window was not expected to reach a completed Android boot; `T642_BOOT_STATE` lines reported `boot_completed=` empty as expected.
- No frame-commit claim: this lane proves the rebuilt kernel reaches userspace with `fb0: simplefb registered!`. Frame commit by guest-origin evidence belongs to the next lane.
- TEN-PASSES was NOT run in this lane (one Route B flight to 1200 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane

The Route B kernel Image (`guest-inputs/routeB-Image`, SHA-256 `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3`) is certified bootable, stable, non-LTO, and possesses a working simplefb (`fb0`).
The next lane can proceed directly to testing userspace graphics and display frame commit on this kernel.
