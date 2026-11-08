import re

UART_01 = "evidence/t700-flight-01-uart.log"

def analyze_launcher():
    with open(UART_01, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    pids = [15418, 17866]

    for pid in pids:
        print(f"\n==========================================")
        print(f"       ALL LINES FOR LAUNCHER PID {pid}")
        print(f"==========================================")
        count = 0
        for idx, line in enumerate(lines, 1):
            if f" {pid} " in line:
                count += 1
                print(f"L{idx}: {line.strip()[:140]}")
        print(f"Total lines for PID {pid}: {count}")

if __name__ == "__main__":
    analyze_launcher()
