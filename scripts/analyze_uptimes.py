import re

UART_LOG = "evidence/t700-flight-01-uart.log"

def analyze_uptimes():
    with open(UART_LOG, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    # Find kernel uptime tags [ sss.ffffff ] around key events
    events = [
        ("PID 9301 fork", 31156),
        ("PID 9301 enter", 31221),
        ("PID 9301 goodbye", 47263),
        ("PID 12708 fork", 51965),
        ("PID 12708 enter", 52027),
        ("PID 12708 goodbye", 76976),
        ("PID 14487 fork", 81151),
        ("PID 14487 enter", 81198),
        ("Cutoff / end", 108514),
    ]

    uptime_re = re.compile(r'\[\s*(\d+\.\d+)\]')

    for name, line_no in events:
        # search backward and forward for nearest kernel timestamp
        prev_ktime = None
        for i in range(line_no - 1, max(0, line_no - 200), -1):
            m = uptime_re.match(lines[i])
            if m:
                prev_ktime = (i + 1, float(m.group(1)), lines[i].strip())
                break
        
        next_ktime = None
        for i in range(line_no - 1, min(len(lines), line_no + 200)):
            m = uptime_re.match(lines[i])
            if m:
                next_ktime = (i + 1, float(m.group(1)), lines[i].strip())
                break
        
        print(f"Event: {name} at L{line_no}: {lines[line_no - 1].strip()[:90]}")
        if prev_ktime:
            print(f"  Prev ktime: L{prev_ktime[0]} [{prev_ktime[1]} s]")
        if next_ktime:
            print(f"  Next ktime: L{next_ktime[0]} [{next_ktime[1]} s]")

if __name__ == "__main__":
    analyze_uptimes()
