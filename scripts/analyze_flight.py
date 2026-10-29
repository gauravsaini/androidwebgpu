import re

log_path = "flights/flight-01/uart.log.numbered.txt"

with open(log_path, "r", encoding="utf-8", errors="replace") as f:
    lines = f.readlines()

print(f"Total lines in flight-01 log: {len(lines)}")

# 1. Check occurrences of SystemServiceRegistry
ssr_lines = []
for i, line in enumerate(lines, 1):
    if "SystemServiceRegistry" in line:
        ssr_lines.append((i, line.strip()))

print(f"Occurrences of SystemServiceRegistry: {len(ssr_lines)}")
for idx, l in ssr_lines:
    print(f"  {l}")

# Check NoClassDefFoundError
ncdf_lines = []
for i, line in enumerate(lines, 1):
    if "NoClassDefFoundError" in line:
        ncdf_lines.append((i, line.strip()))

print(f"Occurrences of NoClassDefFoundError: {len(ncdf_lines)}")
for idx, l in ncdf_lines:
    print(f"  {l}")

# 2. Check Zygote progress past preload / forkSystemServer
fork_lines = []
for i, line in enumerate(lines, 1):
    if any(k in line for k in ["forkSystemServer", "nativeForkSystemServer", "ZygoteFailure", "Not allowlisted", "tombstone"]):
        fork_lines.append((i, line.strip()))

print(f"\nLines related to Zygote fork / failure:")
for idx, l in fork_lines[:25]:
    print(f"  {l}")

# 3. Check SurfaceFlinger and bootanim
sf_lines = []
for i, line in enumerate(lines, 1):
    if any(k in line for k in ["primeShaderCache", "bootanim", "SurfaceFlinger::init", "AutoBackendTexture"]):
        sf_lines.append((i, line.strip()))

print(f"\nSurfaceFlinger & bootanim lines:")
for idx, l in sf_lines:
    print(f"  {l}")

# 4. Check system_server status
ss_lines = []
for i, line in enumerate(lines, 1):
    if "system_server" in line:
        ss_lines.append((i, line.strip()))

print(f"\nOccurrences of system_server: {len(ss_lines)}")
for idx, l in ss_lines[:20]:
    print(f"  {l}")
