import re

UART_01 = "evidence/t700-flight-01-uart.log"

def analyze_swap():
    with open(UART_01, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    # Find all 27 occurrences of "Unable to match the desired swap behavior"
    matches = []
    for idx, line in enumerate(lines, 1):
        if "Unable to match the desired swap behavior" in line:
            matches.append((idx, line.strip()))

    print(f"Total swap mismatch lines: {len(matches)}")
    for m in matches:
        print(f"L{m[0]}: {m[1]}")

    # Inspect context around the first occurrence (L96167) and last occurrence (L108502)
    print("\n=== CONTEXT AROUND FIRST SWAP MISMATCH ===")
    start = max(0, matches[0][0] - 15)
    end = min(len(lines), matches[0][0] + 15)
    for i in range(start, end):
        print(f"L{i+1}: {lines[i].strip()[:110]}")

    print("\n=== CONTEXT AROUND LAST SWAP MISMATCH ===")
    start = max(0, matches[-1][0] - 15)
    end = min(len(lines), matches[-1][0] + 15)
    for i in range(start, end):
        print(f"L{i+1}: {lines[i].strip()[:110]}")

if __name__ == "__main__":
    analyze_swap()
