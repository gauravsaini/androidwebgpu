# Gates: Track T636 — bootclasspath-fix: get zygote past MediaPlayer by adding framework-connectivity.jar to BOOTCLASSPATH (Chain A, implementation)

OWNS: dafg/GATES.md, GATES.md, TIMELINE.md, scripts/**, flights/**, guest-image/**

Scope: Build exactly ONE fix initramfs appending flattened framework-connectivity.jar to BOOTCLASSPATH, boot on QEMU 11.1.2, prove verbatim zygote environ BOOTCLASSPATH includes framework-connectivity.jar, prove MediaPlayer preloads cleanly without NoClassDefFoundError, and prove boot proceeds past preload (system_server launch or exact new wall).

- [x] G636-1: Pinned guest inputs and QEMU 11.1.2 verified, single fix initramfs overlay built appending framework-connectivity.jar to BOOTCLASSPATH, booted on QEMU 11.1.2, capturing zygote64 PID and /proc/<pid>/environ BOOTCLASSPATH verbatim containing framework-connectivity.jar
  CHECK: python3 scripts/verify_g636_1.py
  EXPECT: G636_1_PASS

- [x] G636-2: MediaPlayer preloads without NoClassDefFoundError (no "Error preloading android.media.MediaPlayer"), zygote proceeds past preload (system_server started or exact new wall named with guest log lines), findings at /Users/Shared/agyx-track636-bootclasspath-fix.md, and outcome at /Users/Shared/outcome-t636.txt
  CHECK: python3 scripts/verify_g636_2.py
  EXPECT: G636_2_PASS
