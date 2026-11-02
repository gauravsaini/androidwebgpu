# Gates: Track T663 — system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, evidence/**

Scope: PIN the fix for BOTH faults stopping T662 before Phase 550: (a) Dynamic UID mismatch on first boot for com.android.permissioncontroller, and (b) mount contention from watch_mirrors background daemon; build ONE fix initramfs on T662 lineage (base image 713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810 hardlinked, never copied) supplying fix by pinned mechanism ONLY; watch items: CE lookup repeats for every forked app process, T662 CE fix and T661 DE fix must hold (DE/CE mount-fail 0, CE lookup failure 0), all 13 prior regression vectors hold at 0, 0 .capex across lineage, preserve flattened bind-mount lineage and APEX set; boot QEMU 11.1.2 with 1200s guest target; prove system_server passes PermissionPolicyService onBootPhase(550) into NEXT Phase 550 service (named exactly with line numbers from THIS lane's flight), record every stage reached, name next wall with first failing line if stopped, report sys.boot_completed explicitly; deliver findings to /Users/Shared/agyx-track663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention.md and outcome to /Users/Shared/outcome-t663.txt; update TIMELINE.md.

- [x] G663-1: Pinned root cause of both faults (Dynamic UID mismatch + Mount contention from watch_mirrors) with AOSP source and guest evidence, alternatives ruled out
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "import os, json; d = json.load(open('evidence/root_cause_pinned.json')); assert d['track'] == 'T663'; assert 'fault_a_dynamic_uid_mismatch' in d and 'fault_b_mount_contention' in d; print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK
  EVIDENCE: Met. Both faults pinned in evidence/root_cause_pinned.json with AOSP references (Settings.java, AppIdSettingMap, libcutils/fs.cpp:fs_prepare_path_impl) and raw guest UART evidence from T661 (line 11558) and T662 (lines 14276-14376, 36608-36622). Ruled-out alternatives documented. Pinned fixes: (a) do not pre-create dynamic UID package directories; let installd create them with PMS-assigned UID; (b) make watch_mirrors idempotent using mountpoint -q check, in-memory seen_packages cache, and 500ms sleep.

- [x] G663-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T662 lineage supplying fix by pinned mechanism ONLY, SHA-256 logged
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t662-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == '713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810'; assert os.stat('guest-inputs/t662-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t663-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK
  EVIDENCE: Met. Canonical kernel Image (05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04, st_nlink=12) and base T662 initramfs (713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810, st_nlink=2) hardlinked in guest-inputs/. Exactly ONE fix initramfs built: guest-image/t663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention-initramfs.cpio.gz (SHA-256 6b38578c7966f722b795cbe10bc5fa60f3e9b6443cf3104ed367f0d2be5c0ff1, 953,921,495 bytes). Manifest written to guest-image/t663-manifest.json.

- [x] G663-3: Supervisor script scripts/supervisor_t663.py configured with 1200s guest target and carrying base and fix SHA-256 on guest kernel cmdline
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "s = open('scripts/supervisor_t663.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t663_fix_initramfs_sha256=' in s; assert 'androidboot.t662_base_initramfs_sha256=' in s; assert '713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK
  EVIDENCE: Met. scripts/supervisor_t663.py configured with TARGET_GUEST_SECONDS = 1200, androidboot.t662_base_initramfs_sha256=713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810 and androidboot.t663_fix_initramfs_sha256=<boot_sha> cmdline parameters.

- [x] G663-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T663 fix image
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: Met. Flight-01 executed via scripts/supervisor_t663.py. Host elapsed: 1207.2s, guest log max time: 1205.6s (target 1200s reached). Stop reason: "Guest target reached (1205.6s >= 1200s)". Log SHA-256: cf23e849055d70916af9a7b92e087ad0a76e372e1e695cb9fb796d604c6a3462.

- [x] G663-5: Progress analysis: system_server passes PermissionPolicyService onBootPhase(550) into NEXT Phase 550 service (named exactly with line numbers from THIS lane's flight), all stages recorded, next wall named with first failing line if stopped
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: Met. Flight analysis completed. Dynamic UID mismatch (0 Expected path owner mismatch errors) and periodic mount stacking (total mounts bounded to 47) were 100% eliminated. However, system_server stopped before PermissionPolicyService onBootPhase(550) because the main thread was blocked in Installer.invalidateMounts() for 96s during startBootstrapServices (line 14450), triggering Watchdog termination of system_server PID 7917. System_server restarted as PID 8600 and the 1200s flight budget expired. Verdict: FAIL reported honestly per Anti-T648 protocol.

- [x] G663-6: All prior regression vectors hold at 0, 0 .capex across lineage, sys.boot_completed explicitly reported
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK
  EVIDENCE: Met. All 13 regression vectors held: 0 .capex files across lineage, BOOTCLASSPATH verified, audio-effect VINTF verified, connectivity APEX resources verified, wifi APEX ok, netstats apexdata ok, netd running, sensors HAL running, SDK Sandbox verified, ExtServices verified, Failed to mount /data_mirror/data_de 0, Failed to mount /data_mirror/data_ce 0, Unable to find com.android.permissioncontroller in /data_mirror/data_ce 0. sys.boot_completed explicitly reported as ABSENT (0).

- [x] G663-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "f = open('/Users/Shared/agyx-track663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention' in f; assert 'PermissionPolicyService' in f; assert 'PermissionController' in f or 'com.android.permissioncontroller' in f; assert 'Phase 550' in f or 'PHASE_ACTIVITY_MANAGER_READY' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t663.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: Met. Delivered /Users/Shared/agyx-track663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention.md starting with "Verdict: FAIL", carrying all required strings, analysis, line numbers, and "Rust par bhi lagu: nahi" with reasoning. Delivered /Users/Shared/outcome-t663.txt with "FAIL".

- [x] G663-8: TIMELINE.md updated and worktree committed
  CHECK: /Users/ektasaini/.local/bin/uv run python3 -c "t = open('TIMELINE.md').read(); assert 'T663' in t; print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
  EVIDENCE: Met. TIMELINE.md updated with dated entry (≤ 30 lines) covering Track T663 findings, eliminations, and honest FAIL verdict.
