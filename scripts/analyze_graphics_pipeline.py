import re

UART_01 = "evidence/t700-flight-01-uart.log"

def analyze_graphics():
    with open(UART_01, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    terms = [
        "SurfaceFlinger",
        "HWComposer",
        "Composer",
        "BufferQueue",
        "BLAST",
        "Layer",
        "Vsync",
        "vsync",
        "DispSync",
        "EventThread",
        "MessageQueue",
        "FrameTracker",
        "onMessageReceived",
        "composite",
        "postComposition",
        "setUpHWComposer",
        "prepareFrame",
        "commit",
        "latchBuffer",
        "updateTexImage",
        "acquireBuffer",
        "releaseBuffer",
    ]

    counts = {t: 0 for t in terms}
    samples = {t: [] for t in terms}

    for idx, line in enumerate(lines, 1):
        for t in terms:
            if t.lower() in line.lower():
                counts[t] += 1
                if len(samples[t]) < 10:
                    samples[t].append((idx, line.strip()))

    print("=== GRAPHICS PIPELINE LOG COUNTS ===")
    for t in terms:
        print(f"{t}: {counts[t]}")
        for s in samples[t][:3]:
            print(f"   L{s[0]}: {s[1][:110]}")

if __name__ == "__main__":
    analyze_graphics()
