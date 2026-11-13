#!/usr/bin/env python3
import re
from pathlib import Path

log_path = Path("flights/flight-01/uart.log")
if not log_path.exists():
    print("uart.log does not exist yet")
    exit(0)

text = log_path.read_text(encoding="latin1")
lines = text.splitlines()
print(f"=== FLIGHT-01 STATUS: {len(lines)} lines ===")

# Latest guest timestamp
m_time = re.findall(r"^\[\s*([0-9]+\.[0-9]+)\]", text, re.MULTILINE)
if m_time:
    print(f"Guest latest timestamp: {m_time[-1]}s")

# Key milestones
milestones = [
    ("Freeing unused kernel memory", r"Freeing unused kernel memory"),
    ("Init first stage", r"init: init first stage started!"),
    ("simplefb registered", r"fb0: simplefb registered!"),
    ("VDA mounted /system", r"__mount\(source=/dev/block/vda,target=/system"),
    ("hals.conf loaded", r"SensorsMultiHal"),
    ("Failed to load subHal config", r"Failed to load subHal config file"),
    ("ISensors registered", r"Found android\.hardware\.sensors\.ISensors/default"),
    ("dummy_sensorservice registered", r"dummy_sensorservice.*registered"),
    ("system_server started", r"Entered the Android system server!"),
    ("StartSensorService", r"StartSensorService"),
    ("SensorService onBootPhase", r"SensorService.*onBootPhase"),
    ("Watchdog kills", r"Watchdog:.*KILLING"),
    ("SurfaceFlinger power mode 2", r"Finished setting power mode 2 on display 0"),
    ("queueBuffer", r"queueBuffer"),
    ("presentDisplay", r"presentDisplay"),
    ("validateDisplay", r"validateDisplay"),
    ("sys.boot_completed=1", r"sys\.boot_completed=1"),
    ("Launcher3", r"com\.android\.launcher3"),
]

for name, pattern in milestones:
    matches = re.findall(pattern, text)
    print(f"{name}: {len(matches)}")
