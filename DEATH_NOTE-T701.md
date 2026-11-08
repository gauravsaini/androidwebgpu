# DEATH NOTE — T701

Lane: Track T701 — present-path diagnosis (Mac agyx, why SurfaceFlinger never presents after boot_completed)
Status: CLOSED — PASS (All three hypotheses rigorously adjudicated from preserved flight evidence; root causes isolated; exact next fixes named in priority order)
UTC close: 2026-11-08T05:35:00Z
Worktree: /Users/Shared/wt-track701-present-path-diagnosis
Branch: t701 (from T700 commit 836d758)

## What ended

Track T701 was opened as a diagnosis-first investigation into why SurfaceFlinger never dispatched present in T700 despite `sys.boot_completed=1` being achieved for the first time in the chain.

The diagnosis is definitively closed from preserved evidence (`evidence/t700-flight-01-uart.log`, SHA-256 `7ef491cc…`, 108,514 lines; and `evidence/t700-flight-ab-uart.log`, SHA-256 `8ac276e1…`, 12,119 lines). All three hypotheses were adjudicated with verbatim citations and line numbers:

1. **Hypothesis 1 (Watchdog churn): REFUTED as the root cause of 0 present.**
   - While Android Watchdog killed system_server instances 1 (PID 9301 killed at L47263 after 667.7 s of life) and 2 (PID 12708 killed at L76976 after 295.6 s of life), instance 3 (PID 14487) forked at L81151 (uptime ~1811.2 s), entered at L81198, triggered the second `sys.boot_completed=1` wave (L92063), and survived with **0 Watchdog kills** for 591.8 guest seconds (~9.9 guest minutes) until the 2403.03 s cutoff (L108514).
   - Even with `system_server` alive, stable, and executing for nearly 10 minutes, SurfaceFlinger never dispatched present. Therefore, Watchdog churn alone was not the blocker preventing present.

2. **Hypothesis 2 (EGL / OpenGLRenderer swap mismatch): REFUTED as the proximate blocker.**
   - In `frameworks/base/libs/hwui/renderthread/EglManager.cpp`, RenderThread invokes `eglSurfaceAttrib(..., EGL_SWAP_BEHAVIOR, EGL_BUFFER_PRESERVED)`. SwiftShader GLES does not implement buffer preservation (`EGL_BUFFER_PRESERVED`) and returns `EGL_FALSE`.
   - `EglManager` logs `ALOGE("Unable to match the desired swap behavior.")` and falls back gracefully to standard `EGL_BUFFER_DESTROYED` swap behavior. This is a non-fatal informational error in AOSP.
   - Across the UART log, `Unable to match the desired swap behavior.` appears **27 times** across 27 different client processes (first occurrence at L96167 in PID 15418). Zero processes crashed or aborted from this error.
   - At L108502 (PID 17866), the message logged at 11-10 23:48:00.016, merely 201 ms before the flight cutoff at 2403.03 s (11-10 23:48:00.217, L108514). It did not block frame presentation; it merely logged during normal startup.

3. **Hypothesis 3 (Composer / display pipeline state): CONFIRMED with exact root cause.**
   - SurfaceFlinger received display power mode 2 (NORMAL) 4 times (lines 40822, 57995, 77590, 86322) and declared `Boot is finished` twice (lines 69987, 92481).
   - SurfaceFlinger's Scheduler actually attempted frame composition cycles (lines 107840–108075: `Scheduler: trackPendingFrame: Invalid present fence` ×8).
   - However, SurfaceFlinger never issued `validateDisplay` or `presentDisplay` to Composer 2.1 because **0 client buffers were ever queued** (`queueBuffer`: 0).
   - The reason no buffer was queued: `com.android.launcher3` was caught in a **10-instance fatal crash loop** (PIDs 15418, 16215, 16305, 16355, 16592, 16870, 17004, 17216, 17600, 17657). In every single instance, Launcher3 crashed during `onCreate()` with an unhandled fatal `NullPointerException` in `com.android.launcher3.icons.DotRenderer.<init>` (`Bitmap.getHeight()` on null bitmap, lines 96222–96237, 98640–98656).
   - This crash prevented Launcher3 from completing window attachment, creating a `BLASTBufferQueue`, and submitting its initial window frame, causing `BLASTSyncEngine` in WindowManager to repeatedly time out (lines 98439, 98839, 99190, 99576, 100654, 102366, 103972, 104571, 106905, 108420).
   - The 11th instance (PID 17866) was forked at guest ~2397 s (L108303) and had just loaded SwiftShader (L108494) 201 ms before flight cutoff.
   - Furthermore, Composer 2.1's `executeCommands` HAL implementation (`evidence/t674_composer_binary_audit.md`) carries a known Link 4 empty-handle bypass (`inHandles.size() == 0`), which will require attention once buffers are queued.

## Final artifacts

- `findings-t701.md` — full diagnosis report with verbatim evidence citations, line numbers, and markers.
- `outcome-t701.txt` — single word `PASS`.
- `DEATH_NOTE-T701.md` — this death certificate.
- `progress-t701.txt` — Steps 0–4 progress record.
- `heartbeat.json` — status and timestamp record.
- `TIMELINE.md` — updated project timeline entry.

## What is not claimed

- `sys.boot_completed=1` observed in this lane: explicitly recorded as observed in predecessor flight `t700-flight-01` (at guest 1679.6 s and 2050.8 s); no new flight was run in T701 (`fly none`).
- No rebuild was performed: all images, kernels, and ELFs remained untouched.
- No frame commit claimed: no guest-origin frame commit into fb0 occurred.
- Snapshot byte differences were not claimed or used as frame commit proof.
- TEN-PASSES was NOT run in this lane (diagnosis lane, fly none). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane (T702)

The path forward for Stage 5 frame presentation requires addressing the two diagnosed barriers in order:
1. **Fix 1 (Cure Launcher3 Crash Loop)**: Patch `DotRenderer.<init>` in `Launcher3QuickStep.apk` against null bitmap, or disable notification dots via system properties / overlay, or configure default home to `com.android.settings/.FallbackHome` (which already initialized cleanly without crash). This allows the home application to complete `onCreate()`, bind `ViewRootImpl`, create a `BLASTBufferQueue`, and submit the first client buffer to SurfaceFlinger.
2. **Fix 2 (Composer 2.1 FMQ Dispatch)**: In `SimpleFBClient::executeCommands`, ensure the incoming client target buffer is read from FMQ / binder and blitted via `FramebufferDevice::blitFromBuffer` to the hardware simplefb character device node at physical MMIO `0x5f000000`.
