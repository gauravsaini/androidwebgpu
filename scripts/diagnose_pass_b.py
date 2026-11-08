import re

UART_01 = "evidence/t700-flight-01-uart.log"
UART_AB = "evidence/t700-flight-ab-uart.log"

def analyze_composer_and_egl():
    with open(UART_01, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    print(f"Total lines in flight-01: {len(lines)}")

    # 1. Composer 2.1 PID and binder traffic
    composer_lines = []
    composer_pids = set()
    bc_commands = {}
    br_commands = {}

    for idx, line in enumerate(lines, 1):
        if "android.hardware.graphics.composer@2.1-service" in line:
            composer_lines.append((idx, line.strip()))
            # extract PID e.g. " 8132  8132 I android.hardware.graphics.composer..."
            m = re.search(r'\s+(\d+)\s+(\d+)\s+[IWEVD]\s+android\.hardware\.graphics\.composer', line)
            if m:
                composer_pids.add(int(m.group(1)))

        if "BC_FREE_BUFFER" in line:
            bc_commands["BC_FREE_BUFFER"] = bc_commands.get("BC_FREE_BUFFER", 0) + 1

        # Look for other BC_ or BR_ commands
        m_bc = re.findall(r'\b(BC_[A-Z_]+)\b', line)
        for cmd in m_bc:
            if cmd != "BC_FREE_BUFFER":
                bc_commands[cmd] = bc_commands.get(cmd, 0) + 1

        m_br = re.findall(r'\b(BR_[A-Z_]+)\b', line)
        for cmd in m_br:
            br_commands[cmd] = br_commands.get(cmd, 0) + 1

    print(f"\n=== COMPOSER 2.1 SERVICE ===")
    print(f"Composer lines count: {len(composer_lines)}")
    print(f"Composer PIDs: {composer_pids}")
    print(f"BC commands: {bc_commands}")
    print(f"BR commands: {br_commands}")

    # Check for executeCommands in Composer lines
    exec_cmds = [c for c in composer_lines if "executeCommands" in c[1]]
    print(f"executeCommands count in composer lines: {len(exec_cmds)}")

    # Check for any validateDisplay, presentDisplay, presentOrValidate
    val_disp = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "validateDisplay" in line]
    pres_disp = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "presentDisplay" in line]
    pres_or_val = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "presentOrValidate" in line]
    print(f"validateDisplay count: {len(val_disp)}")
    print(f"presentDisplay count: {len(pres_disp)}")
    print(f"presentOrValidate count: {len(pres_or_val)}")

    # 2. Power mode / display state
    power_modes = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "Setting power mode" in line or "setPowerMode" in line]
    print(f"\n=== DISPLAY POWER MODES ===")
    print(f"Power mode lines ({len(power_modes)}):")
    for pm in power_modes:
        print(f"  L{pm[0]}: {pm[1]}")

    active_configs = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "setActiveConfig" in line or "setActiveMode" in line]
    print(f"Active config / mode lines ({len(active_configs)}):")
    for ac in active_configs:
        print(f"  L{ac[0]}: {ac[1]}")

    display_state = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "setDisplayState" in line]
    print(f"setDisplayState count: {len(display_state)}")

    # 3. EGL and SwiftShader lines
    egl_lines = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "libEGL" in line or "EglManager" in line or "eglChooseConfig" in line or "eglCreate" in line]
    print(f"\n=== EGL / SWIFTSHADER TRACE ===")
    print(f"EGL lines count: {len(egl_lines)}")
    for el in egl_lines[:15]:
        print(f"  L{el[0]}: {el[1]}")
    if len(egl_lines) > 15:
        print(f"  ... last 5:")
        for el in egl_lines[-5:]:
            print(f"  L{el[0]}: {el[1]}")

    # Check OpenGLRenderer lines
    ogl_lines = [(idx, line.strip()) for idx, line in enumerate(lines, 1) if "OpenGLRenderer" in line]
    print(f"\n=== OPENGLRENDERER TRACE ===")
    print(f"OpenGLRenderer lines count: {len(ogl_lines)}")
    swap_mismatch = [l for l in ogl_lines if "Unable to match the desired swap behavior" in l[1]]
    print(f"Swap mismatch count: {len(swap_mismatch)}")
    for sm in swap_mismatch[:5]:
        print(f"  L{sm[0]}: {sm[1]}")
    if len(swap_mismatch) > 5:
        print(f"  ... last 5:")
        for sm in swap_mismatch[-5:]:
            print(f"  L{sm[0]}: {sm[1]}")

    # 4. Context around L108483-L108505
    print(f"\n=== CONTEXT AROUND L108480 - L108514 ===")
    for i in range(108480, min(len(lines), 108514)):
        print(f"L{i+1}: {lines[i].strip()}")

    # 5. Cross-check flight-ab
    print(f"\n=== CROSS-CHECK FLIGHT-AB ===")
    with open(UART_AB, "r", newline="\n", errors="replace") as f_ab:
        lines_ab = f_ab.readlines()
    print(f"Total lines in flight-ab: {len(lines_ab)}")
    ab_composer = [l for l in lines_ab if "composer" in l.lower()]
    ab_power = [l for l in lines_ab if "Setting power mode" in l]
    ab_egl = [l for l in lines_ab if "libEGL" in l]
    ab_swap = [l for l in lines_ab if "Unable to match" in l]
    print(f"flight-ab composer mentions: {len(ab_composer)}")
    print(f"flight-ab power mode calls: {len(ab_power)}")
    print(f"flight-ab libEGL lines: {len(ab_egl)}")
    print(f"flight-ab swap mismatch lines: {len(ab_swap)}")

if __name__ == "__main__":
    analyze_composer_and_egl()
