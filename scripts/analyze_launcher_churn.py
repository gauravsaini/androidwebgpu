import re

UART_01 = "evidence/t700-flight-01-uart.log"

def analyze_launcher_deaths():
    with open(UART_01, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    launcher_pids = [15418, 16215, 16305, 16355, 16592, 16870, 17004, 17216, 17600, 17657, 17866]

    for pid in launcher_pids:
        print(f"\n--- Launcher PID {pid} ---")
        deaths = []
        crashes = []
        anrs = []
        for idx, line in enumerate(lines, 1):
            if f" {pid} " in line:
                if "FATAL" in line or "Exception" in line or "died" in line or "kill" in line.lower() or "signal" in line.lower() or "exit" in line.lower() or "ANR" in line:
                    deaths.append((idx, line.strip()))
            if f"Process {pid} " in line or f"proc {pid}:" in line or f"pid {pid}" in line:
                if "died" in line or "kill" in line.lower() or "signal" in line.lower() or "exit" in line.lower():
                    deaths.append((idx, line.strip()))

        for d in deaths[:10]:
            print(f"  L{d[0]}: {d[1][:120]}")

if __name__ == "__main__":
    analyze_launcher_deaths()
