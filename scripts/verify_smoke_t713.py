#!/usr/bin/env python3
import re
from pathlib import Path

uart = Path("flights/smoke/uart.log").read_text(encoding="latin1")

print("=== T713 SMOKE FLIGHT VERIFICATION ===")
print("Total lines:", len(uart.splitlines()))

# 1. Freeing unused kernel memory
m_mem = re.search(r"\[\s*([0-9.]+)\] Freeing unused kernel memory: (\d+K)", uart)
if m_mem:
    print(f"PASS: Freeing unused kernel memory: {m_mem.group(2)} at guest {m_mem.group(1)}s")
else:
    print("FAIL: Freeing unused kernel memory not found")

# 2. init first stage started
m_init = re.search(r"\[\s*([0-9.]+)\] init: init first stage started!", uart)
if m_init:
    print(f"PASS: Init first stage started at guest {m_init.group(1)}s")
else:
    print("FAIL: Init first stage not found")

# 3. simplefb registered
m_fb = re.search(r"\[\s*([0-9.]+)\] fb0: simplefb registered!", uart)
if m_fb:
    print(f"PASS: fb0 simplefb registered at guest {m_fb.group(1)}s")
else:
    print("FAIL: simplefb not found")

# 4. VDA mount
m_vda = re.search(r"\[\s*([0-9.]+)\] init: \[libfs_mgr\]Mounted /dev/block/vda as /system Success", uart)
if m_vda:
    print(f"PASS: /dev/block/vda mounted as /system Success at guest {m_vda.group(1)}s")
else:
    # check alternate vda mount string
    vda_lines = [l for l in uart.splitlines() if "Mounted /dev/block/vda" in l or "vda" in l and "mount" in l.lower()]
    print("VDA mount lines:", vda_lines[:5])

# 5. SensorsMultiHal and hals.conf
load_failures = len(re.findall(r"Failed to load subHal config file", uart))
print(f"Failed to load subHal config file errors: {load_failures}")

vintf_sensors = re.findall(r"Found android\.hardware\.sensors\.ISensors/default in device VINTF manifest", uart)
print(f"ISensors/default in VINTF manifest: {len(vintf_sensors)}")

sigabrt_count = len(re.findall(r"SIGABRT", uart))
print(f"SIGABRT count: {sigabrt_count}")

# 6. Dummy sensorservice registration
dummy_reg = re.findall(r"dummy_sensorservice.*registered|Starting dummy sensorservice daemon", uart)
print(f"dummy_sensorservice lines: {len(dummy_reg)}")

# 7. sensorservice avc denials
avc_sensors = [l for l in uart.splitlines() if "avc:  denied" in l and "sensorservice" in l]
print(f"sensorservice avc denials: {len(avc_sensors)}")

# 8. Watchdog kills
watchdog_kills = len(re.findall(r"Watchdog:.*KILLING", uart))
print(f"Watchdog kills: {watchdog_kills}")

# 9. Prime cache
prime_errors = len(re.findall(r"output buffer not gpu writeable", uart))
print(f"Prime cache errors: {prime_errors}")

# 10. system_server started
ss_starts = re.findall(r"Entered the Android system server!", uart)
print(f"system_server starts: {len(ss_starts)}")
