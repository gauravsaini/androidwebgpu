#!/usr/bin/env python3
"""Comprehensive analyzer for Track T673 Flight-01 raw uart.log.
Verifies Stage 5 Display frame-commit evidence and audits all T664-T672 checkable regressions.
"""
from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
FLIGHT = WORKTREE / "flights/flight-01"
LOG = FLIGHT / "uart.log"
NUMBERED_LOG = FLIGHT / "uart.log.numbered.txt"


def analyze():
    target_log = NUMBERED_LOG if NUMBERED_LOG.exists() else LOG
    if not target_log.exists():
        print(f"Error: Log file {target_log} does not exist.")
        sys.exit(1)

    print(f"Analyzing {target_log} ...")

    # Regex patterns
    guest_time_re = re.compile(r"\[\s*([0-9]+\.[0-9]+)\]")
    system_server_start_alt = re.compile(r"(\d+)\s+(\d+)\s+I\s+SystemServer:\s+Entered the Android system server!")
    system_server_start_re = re.compile(r"SystemServer:\s+Entered the Android system server!.*?pid[:=]?\s*(\d+)", re.I)
    boot_state_re = re.compile(r"T642_BOOT_STATE.*uptime=([0-9.]+).*system_server_pids=([0-9,]*).*boot_completed=([0-9]).*ce_available=(true|false)")
    boot_completed_re = re.compile(r"processing action \(sys\.boot_completed=1\)")
    unlocked_ce_re = re.compile(r"Unlocked CE storage for unsecured user (\d+)")
    finish_unlock_re = re.compile(r"finishUserUnlocking-(\d+)")
    before_unlock_re = re.compile(r"UM\.onBeforeUnlockUser-(\d+)")
    watchdog_kill_re = re.compile(r"WATCHDOG KILLING SYSTEM PROCESS")
    fatal_re = re.compile(r"FATAL EXCEPTION IN SYSTEM PROCESS")
    autocorker_re = re.compile(r"Slow delivery took (\d+)ms main h=.*PropertyInvalidatedCache\$AutoCorker")
    idmap_stop_init_re = re.compile(r"init:\s+Control message:\s+Processed ctl\.stop for 'idmap2d'")
    idmap_stop_signal_re = re.compile(r"init:\s+Sending signal 9 to service 'idmap2d'")
    idmap_slow_dispatch_re = re.compile(r"Slow dispatch took (\d+)ms android\.fg.*IdmapDaemon")
    gatekeeper_npe_re = re.compile(r"Unable to acquire GateKeeperService|NullPointerException.*GateKeeper")
    posix_acl_err_re = re.compile(r"EOPNOTSUPP.*acl|failed to set default acl", re.I)
    zygote_sigsegv_re = re.compile(r"Fatal signal 11 \(SIGSEGV\).*zygote", re.I)
    odrefresh_err_re = re.compile(r"odrefresh.*exit\s*\((79|1)\)")
    media_c2_registered_re = re.compile(r"Registered android\.hardware\.media\.c2.*software")
    media_c2_drop_re = re.compile(r"cannot find.*android\.hardware\.media\.c2.*software|dropping control message.*software", re.I)
    fp_registered_re = re.compile(r"Registering service.*fingerprint\.IFingerprint|servicemanager: Found.*fingerprint\.IFingerprint")
    ce_available_true_re = re.compile(r"ce_available=true")
    ce_prop_re = re.compile(r"Setting property:\s+sys\.user\.0\.ce_available=true")

    # Display-specific patterns
    simplefb_composer_start_re = re.compile(r"SimpleFB Composer 2.1 service starting")
    simplefb_mapped_re = re.compile(r"Successfully mapped /dev/fb0 at ([0-9a-fx]+) \((\d+) bytes\)")
    simplefb_create_client_re = re.compile(r"createClient\(\) called by client \(SurfaceFlinger\)")
    simplefb_hotplug_re = re.compile(r"onHotplug callback delivered successfully")
    sf_start_re = re.compile(r"SurfaceFlinger is starting")
    sf_choose_strategy_failed_re = re.compile(r"chooseCompositionStrategy failed for SimpleFB:\s+(-?\d+)")
    sf_present_or_validate_failed_re = re.compile(r"presentOrValidate failed for display (\d+):\s+([A-Za-z0-9_]+)\s+\((\d+)\)")
    sf_present_failed_re = re.compile(r"present failed for display (\d+):\s+([A-Za-z0-9_]+)\s+\((\d+)\)")
    launcher3_fatal_re = re.compile(r"FATAL EXCEPTION: main.*?Process:\s+com\.android\.launcher3")

    total_lines = 0
    max_guest_time = 0.0
    system_server_starts = []
    boot_states = []
    boot_completed_lines = []
    unlocked_ce_lines = []
    finish_unlock_lines = []
    before_unlock_lines = []
    watchdog_kills = []
    system_fatals = []
    autocorker_deliveries = []
    idmap_stop_inits = []
    idmap_stop_signals = []
    idmap_slow_dispatches = []
    gatekeeper_npes = []
    posix_acl_errors = []
    zygote_sigsegvs = []
    odrefresh_errors = []
    media_c2_registered = []
    media_c2_drops = []
    fp_registered = []
    ce_available_lines = []
    ce_prop_lines = []

    # Display matches
    simplefb_composer_starts = []
    simplefb_mapped = []
    simplefb_create_clients = []
    simplefb_hotplugs = []
    sf_starts = []
    sf_choose_strategy_failures = []
    sf_present_or_validate_failures = []
    sf_present_failures = []
    launcher3_fatals = []

    with open(target_log, "r", encoding="latin1") as f:
        for idx, raw_line in enumerate(f, start=1):
            total_lines += 1
            line = raw_line.strip()

            gt = guest_time_re.search(line)
            if gt:
                try:
                    t = float(gt.group(1))
                    if t > max_guest_time:
                        max_guest_time = t
                except ValueError:
                    pass

            # SystemServer start
            m = system_server_start_alt.search(line)
            if m:
                system_server_starts.append((idx, int(m.group(1)), line))
            else:
                m2 = system_server_start_re.search(line)
                if m2:
                    system_server_starts.append((idx, int(m2.group(1)), line))
                elif "Entered the Android system server!" in line:
                    system_server_starts.append((idx, None, line))

            # Boot State
            bs = boot_state_re.search(line)
            if bs:
                boot_states.append((idx, line, bs.groups()))

            # Boot completed
            if boot_completed_re.search(line):
                boot_completed_lines.append((idx, line))

            # Unlock events
            if unlocked_ce_re.search(line):
                unlocked_ce_lines.append((idx, line))
            if finish_unlock_re.search(line):
                finish_unlock_lines.append((idx, line))
            if before_unlock_re.search(line):
                before_unlock_lines.append((idx, line))

            # Watchdog kills
            if watchdog_kill_re.search(line) or "WATCHDOG KILLING SYSTEM PROCESS" in line:
                watchdog_kills.append((idx, line))

            # Fatal exceptions
            if fatal_re.search(line):
                system_fatals.append((idx, line))

            # CE properties
            if ce_available_true_re.search(line):
                ce_available_lines.append((idx, line))
            if ce_prop_re.search(line):
                ce_prop_lines.append((idx, line))

            # AutoCorker
            ac = autocorker_re.search(line)
            if ac:
                autocorker_deliveries.append((idx, int(ac.group(1)), line))

            # idmap stop
            if idmap_stop_init_re.search(line):
                idmap_stop_inits.append((idx, line))
            if idmap_stop_signal_re.search(line):
                idmap_stop_signals.append((idx, line))
            isd = idmap_slow_dispatch_re.search(line)
            if isd:
                idmap_slow_dispatches.append((idx, int(isd.group(1)), line))

            # Regressions
            if gatekeeper_npe_re.search(line):
                gatekeeper_npes.append((idx, line))
            if posix_acl_err_re.search(line):
                posix_acl_errors.append((idx, line))
            if zygote_sigsegv_re.search(line):
                zygote_sigsegvs.append((idx, line))
            if odrefresh_err_re.search(line):
                odrefresh_errors.append((idx, line))
            if media_c2_registered_re.search(line):
                media_c2_registered.append((idx, line))
            if media_c2_drop_re.search(line):
                media_c2_drops.append((idx, line))
            if fp_registered_re.search(line):
                fp_registered.append((idx, line))

            # Display
            if simplefb_composer_start_re.search(line):
                simplefb_composer_starts.append((idx, line))
            if simplefb_mapped_re.search(line):
                simplefb_mapped.append((idx, line))
            if simplefb_create_client_re.search(line):
                simplefb_create_clients.append((idx, line))
            if simplefb_hotplug_re.search(line):
                simplefb_hotplugs.append((idx, line))
            if sf_start_re.search(line):
                sf_starts.append((idx, line))
            if sf_choose_strategy_failed_re.search(line):
                sf_choose_strategy_failures.append((idx, line))
            if sf_present_or_validate_failed_re.search(line):
                sf_present_or_validate_failures.append((idx, line))
            if sf_present_failed_re.search(line):
                sf_present_failures.append((idx, line))
            if "FATAL EXCEPTION: main" in line and "com.android.launcher3" in line:
                launcher3_fatals.append((idx, line))

    results = {
        "total_lines": total_lines,
        "max_guest_time": max_guest_time,
        "system_server_starts": system_server_starts,
        "system_server_cycle_count": len(system_server_starts),
        "boot_states_count": len(boot_states),
        "last_boot_state": boot_states[-1] if boot_states else None,
        "boot_completed_lines": boot_completed_lines,
        "unlocked_ce_lines": unlocked_ce_lines,
        "finish_unlock_lines": finish_unlock_lines,
        "before_unlock_lines": before_unlock_lines,
        "ce_available_lines_count": len(ce_available_lines),
        "ce_prop_lines": ce_prop_lines,
        "watchdog_kills": watchdog_kills,
        "system_fatals": system_fatals,
        "autocorker_deliveries": autocorker_deliveries,
        "idmap_stop_inits": idmap_stop_inits,
        "idmap_stop_signals": idmap_stop_signals,
        "idmap_slow_dispatches": idmap_slow_dispatches,
        "gatekeeper_npes": gatekeeper_npes,
        "posix_acl_errors": posix_acl_errors,
        "zygote_sigsegvs": zygote_sigsegvs,
        "odrefresh_errors": odrefresh_errors,
        "media_c2_registered": media_c2_registered,
        "media_c2_drops": media_c2_drops,
        "fp_registered": fp_registered,
        "display": {
            "simplefb_composer_starts": simplefb_composer_starts,
            "simplefb_mapped": simplefb_mapped,
            "simplefb_create_clients": simplefb_create_clients,
            "simplefb_hotplugs": simplefb_hotplugs,
            "sf_starts": sf_starts,
            "sf_choose_strategy_failures_count": len(sf_choose_strategy_failures),
            "sf_present_or_validate_failures_count": len(sf_present_or_validate_failures),
            "sf_present_failures_count": len(sf_present_failures),
            "first_sf_choose_strategy_failure": sf_choose_strategy_failures[0] if sf_choose_strategy_failures else None,
            "first_sf_present_or_validate_failure": sf_present_or_validate_failures[0] if sf_present_or_validate_failures else None,
            "first_sf_present_failure": sf_present_failures[0] if sf_present_failures else None,
        }
    }

    report_path = FLIGHT / "flight-analysis.json"
    with open(report_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)

    print("\n=== FLIGHT-01 AUDIT SUMMARY ===")
    print(f"Total log lines: {total_lines}")
    print(f"Max guest time: {max_guest_time:.2f}s")
    print(f"system_server starts: {len(system_server_starts)} (cycles: {len(system_server_starts)})")
    for s in system_server_starts:
        print(f"  Line {s[0]}: PID={s[1]}")
    print(f"sys.boot_completed lines: {len(boot_completed_lines)}")
    for b in boot_completed_lines:
        print(f"  Line {b[0]}: {b[1]}")
    print(f"Unlocked CE storage lines: {len(unlocked_ce_lines)}")
    for u in unlocked_ce_lines:
        print(f"  Line {u[0]}: {u[1]}")
    print(f"finishUserUnlocking lines: {len(finish_unlock_lines)}")
    for f_line in finish_unlock_lines:
        print(f"  Line {f_line[0]}: {f_line[1]}")
    print(f"UM.onBeforeUnlockUser lines: {len(before_unlock_lines)}")
    for b_line in before_unlock_lines:
        print(f"  Line {b_line[0]}: {b_line[1]}")
    print(f"CE available=true lines: {len(ce_available_lines)}")
    for cp in ce_prop_lines:
        print(f"  First CE prop line {cp[0]}: {cp[1]}")
    print(f"Watchdog kills: {len(watchdog_kills)}")
    print(f"System FATAL exceptions: {len(system_fatals)}")
    print(f"AutoCorker slow deliveries: {len(autocorker_deliveries)}")
    for ac in autocorker_deliveries:
        print(f"  Line {ac[0]}: {ac[1]}ms")
    print(f"idmap stop inits: {len(idmap_stop_inits)}")
    for isi in idmap_stop_inits:
        print(f"  Line {isi[0]}: {isi[1]}")
    if boot_states:
        print(f"Final BOOT_STATE: Line {boot_states[-1][0]}: {boot_states[-1][1]}")

    print("\n=== DISPLAY STACK AUDIT ===")
    print(f"SimpleFB Composer starts: {len(simplefb_composer_starts)}")
    print(f"SimpleFB Mappings: {len(simplefb_mapped)}")
    print(f"SimpleFB Client creations: {len(simplefb_create_clients)}")
    print(f"SimpleFB onHotplug delivered: {len(simplefb_hotplugs)}")
    print(f"SurfaceFlinger starts: {len(sf_starts)}")
    print(f"SurfaceFlinger chooseCompositionStrategy failures: {len(sf_choose_strategy_failures)}")
    print(f"SurfaceFlinger presentOrValidate failures: {len(sf_present_or_validate_failures)}")
    print(f"SurfaceFlinger present failures: {len(sf_present_failures)}")
    if sf_present_or_validate_failures:
        print(f"  First presentOrValidate failure: Line {sf_present_or_validate_failures[0][0]}: {sf_present_or_validate_failures[0][1]}")
    if sf_choose_strategy_failures:
        print(f"  First chooseCompositionStrategy failure: Line {sf_choose_strategy_failures[0][0]}: {sf_choose_strategy_failures[0][1]}")

    print("\n=== REGRESSION CHECKS ===")
    print(f"GateKeeper NPEs: {len(gatekeeper_npes)}")
    print(f"POSIX ACL errors: {len(posix_acl_errors)}")
    print(f"Zygote SIGSEGV: {len(zygote_sigsegvs)}")
    print(f"odrefresh errors: {len(odrefresh_errors)}")
    print(f"media.c2 registered: {len(media_c2_registered)}")
    print(f"media.c2 drop floods: {len(media_c2_drops)}")
    print(f"Fingerprint registered: {len(fp_registered)}")

    return results


if __name__ == "__main__":
    analyze()
