import re
import sys

UART_LOG = "evidence/t700-flight-01-uart.log"

def analyze_timeline():
    with open(UART_LOG, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    print(f"Total lines in {UART_LOG}: {len(lines)}")

    # PIDs to track
    pids = [9301, 12708, 14487]

    # Let's inspect log lines for each PID
    # We want:
    # - Fork line
    # - Enter line
    # - Key services started
    # - Storage unlock
    # - Window additions (WindowManager: addWindow / relayout / etc.)
    # - DisplayManager power mode / display state
    # - Launcher3 interaction
    # - Watchdog kill line
    # - Guest timestamps / kernel uptime

    for pid in pids:
        print(f"\n==========================================")
        print(f"       ANALYSIS OF PID {pid}")
        print(f"==========================================")
        pid_str = f" {pid} "
        
        fork_info = None
        enter_info = None
        goodbye_info = []
        services = []
        storage_unlock = []
        windows_added = []
        power_mode_lines = []
        launcher_lines = []
        last_line_for_pid = None

        for idx, line in enumerate(lines, 1):
            if f"System server process {pid} has been created" in line:
                fork_info = (idx, line.strip())
            if pid_str in line:
                last_line_for_pid = (idx, line.strip())
                if "Entered the Android system server!" in line:
                    enter_info = (idx, line.strip())
                if "GOODBYE!" in line:
                    goodbye_info.append((idx, line.strip()))
                if "Start" in line and ("Service" in line or "SystemServerTiming" in line):
                    # Sample some key services
                    for s in ["ActivityManager", "PowerManager", "DisplayManager", "WindowManager", "PackageManager", "DropBox", "Notification"]:
                        if s in line:
                            services.append((idx, line.strip()))
                            break
                if "unlock" in line.lower() or "storage" in line.lower() or "user_de" in line.lower() or "mirror_de" in line.lower() or "ce_available" in line.lower():
                    storage_unlock.append((idx, line.strip()))
                if "addWindow" in line or "WindowManager" in line and ("Adding window" in line or "addWindow" in line):
                    windows_added.append((idx, line.strip()))
                if "launcher" in line.lower():
                    launcher_lines.append((idx, line.strip()))

            # Also check display power mode / SurfaceFlinger related to this time period
            if "Setting power mode" in line:
                power_mode_lines.append((idx, line.strip()))

        print(f"Fork: {fork_info}")
        print(f"Enter: {enter_info}")
        print(f"Goodbye: {goodbye_info}")
        print(f"Last line mentioning PID {pid}: {last_line_for_pid}")
        print(f"Key services count: {len(services)} (first 5: {services[:5]}, last 3: {services[-3:] if services else []})")
        print(f"Storage/unlock lines count: {len(storage_unlock)} (sample: {storage_unlock[:5]})")
        print(f"Windows added lines count: {len(windows_added)} (sample: {windows_added[:5]})")
        print(f"Launcher lines count: {len(launcher_lines)} (sample: {launcher_lines[:5]})")

if __name__ == "__main__":
    analyze_timeline()
