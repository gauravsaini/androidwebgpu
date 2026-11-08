# DEATH NOTE — T697

Lane: Track T697 — Frame commit on the Route B kernel (Mac agyx, flight only, no rebuild)
Status: CLOSED — FAIL (No guest-origin frame commit into fb0 observed; system_server never forked; SurfaceFlinger present never dispatched)
UTC close: 2026-11-08T00:30:00Z
Worktree: /Users/Shared/wt-track697-framecommit-routeb-agyx
Branch: t697 (from T696 commit cb0ff77)

## What ended

The lane was opened to re-fly the T683 fix initramfs (`base-t683-fix.cpio.gz`, carrying the replacement Composer 2.1 service ELF `ddc1e4dd…`) on the certified Route B kernel (`routeB-Image`, SHA-256 `3f45272f…`) and modified DTB (`modified.dtb`, SHA-256 `e83a6437…`) to determine whether the display path completes under the Route B kernel which carries `CONFIG_DEVMEM=y`, `CONFIG_FB=y`, `CONFIG_FB_SIMPLE=y`, and `CONFIG_DRM_FBDEV_EMULATION=y`.

The outcome is an honest Verdict FAIL:
1. **Inputs & Staging**: All 10 staged inputs were SHA-verified. No rebuild of kernel, initramfs, or binaries was conducted.
2. **Flight on Route B Kernel (`t697-flight-01`)**: The Route B kernel flew on Mac QEMU 11.1.2 for the full 1200 s guest target window (guest 1201.365 s, host 1202.85 s, exit 0, 51,301 lines, UART SHA-256 `ae12556e2449bc558edb10da5540d8ae903dcaa6003db76f188363c3a31bac97`).
   - All T696 userspace regression markers held: `fb0: simplefb registered!` at guest 0.337292 s, `Freeing unused kernel memory: 960K` at guest 26.061646 s, `init first stage` at guest 29.070790 s, `init second stage` at guest 35.115832 s.
   - Zero hang, zero `udf #0x0`, zero Oops, zero panic.
   - SurfaceFlinger (pid 21261) initialized with SkiaGL Backend, loaded SwiftShader (`/vendor/lib64/egl/libGLES_swiftshader.so`), SimpleFB CPU GLES Rasterizer, and launched `bootanim` (pid 21327).
3. **Display Path Wall**:
   - SurfaceFlinger never dispatched present (0 `presentOrValidate` or `presentDisplay` calls appeared in the UART stream).
   - Composer 2.1 service (`android.hardware.graphics.composer@2.1-service`, pid 20484) received binder calls but never dispatched frame commits to `fb0`.
   - Zero guest-origin writes to the hardware framebuffer device (`0x5f000000`) occurred.
   - `system_server` never forked from Zygote: all 82 periodic `T642_BOOT_STATE` watcher probes across 1200 s guest window showed empty `system_server_pids=` and empty `system_server_prop=`.
   - `sys.boot_completed` was not observed (`boot_completed=` empty).
4. **A/B Control (`t697-flight-ab`)**: Canonical Image (`05c43917…`) booted cleanly on the same Mac QEMU 11.1.2 harness, reaching `Freeing unused kernel memory: 1024K` at guest 24.374130 s and `init first stage` at guest 27.269416 s, running to guest 242.40 s (host 245.53 s, exit 0, 13,371 lines, UART SHA-256 `181518276f8676df52032b3c302dadfbf5221aaed8b62a2236bf7b59a269bf01`), re-proving Mac QEMU 11.1.2 harness sound.

## Final artifacts

- `findings-t697.md` — full adjudication, verbatim evidence citations, and marker lines.
- `outcome-t697.txt` — single word `FAIL`.
- `DEATH_NOTE-T697.md` — this death certificate.
- `progress-t697.txt` — Steps 0–5 completion record.
- `heartbeat.json` — updated timestamp and status.
- `scripts/supervisor_t697.py` — adapted flight supervisor.
- `flights/t697-flight-01/` — Route B flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `flights/t697-flight-ab/` — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).

## What is not claimed

- No `sys.boot_completed=1`: not observed; all 82 `T642_BOOT_STATE` lines reported `boot_completed=` empty.
- No frame commit claimed: no guest-origin evidence of a frame commit into `fb0` was observed.
- Snapshot byte differences were not claimed or used as frame commit proof (per T673 standing adjudication).
- TEN-PASSES was NOT run in this lane (one Route B flight to 1200 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane

The Route B kernel (`CONFIG_DEVMEM=y`, `CONFIG_FB=y`, `CONFIG_FB_SIMPLE=y`, `CONFIG_DRM_FBDEV_EMULATION=y`) and `modified.dtb` register `fb0` and boot cleanly to userspace without regression.
However, Stage 5 frame commit is blocked on userspace:
1. `system_server` never forks from Zygote.
2. SurfaceFlinger does not dispatch `present` calls to Composer 2.1.
Investigation of the Zygote/system_server fork trigger and SurfaceFlinger present dispatch mechanism is the required next step.
