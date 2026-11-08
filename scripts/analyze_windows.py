import re

UART_LOG = "evidence/t700-flight-01-uart.log"

def analyze_windows():
    with open(UART_LOG, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    print(f"Total lines: {len(lines)}")

    # WindowManager, SurfaceControl, ViewRootImpl, relayout, addWindow
    patterns = [
        "relayoutWindow",
        "addWindow",
        "ViewRootImpl",
        "SurfaceControl",
        "WindowManager",
        "wm_activity_launch_time",
        "wm_set_resumed_activity",
        "wm_task_moved",
        "performTraversals",
        "BLASTBufferQueue",
        "queueBuffer",
        "dequeueBuffer",
        "commitTransaction",
        "setPowerMode",
        "Boot is finished",
        "Unable to match the desired swap behavior"
    ]

    found = {p: [] for p in patterns}

    for idx, line in enumerate(lines, 1):
        for p in patterns:
            if p in line:
                found[p].append((idx, line.strip()))

    for p in patterns:
        print(f"\n=== Pattern: {p} (count: {len(found[p])}) ===")
        for item in found[p][:10]:
            print(f"  L{item[0]}: {item[1][:120]}")
        if len(found[p]) > 10:
            print(f"  ... (last 3):")
            for item in found[p][-3:]:
                print(f"  L{item[0]}: {item[1][:120]}")

if __name__ == "__main__":
    analyze_windows()
