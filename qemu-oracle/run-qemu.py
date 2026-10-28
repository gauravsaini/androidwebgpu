import socket, subprocess, sys, time, os, re

D = "/Users/Shared/wt-track631-qreg-fix/qemu-oracle"
MON = "/tmp/q631-mon"
OUT = os.path.join(D, "output.bin")
for p in (MON, OUT):
    try:
        os.unlink(p)
    except FileNotFoundError:
        pass

q = subprocess.Popen([
    "/opt/homebrew/bin/qemu-system-aarch64",
    "-machine", "virt", "-cpu", "cortex-a53", "-m", "256",
    "-nographic", "-serial", "none", "-display", "none",
    "-monitor", "unix:" + MON + ",server,nowait",
    "-kernel", os.path.join(D, "t631-q.elf"),
], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=D)
time.sleep(4)

def read_until_prompt(s, timeout=8):
    s.settimeout(timeout)
    buf = b""
    try:
        while b"(qemu)" not in buf[-64:]:
            chunk = s.recv(65536)
            if not chunk:
                break
            buf += chunk
    except socket.timeout:
        pass
    return buf

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(MON)
read_until_prompt(s)
import subprocess as _sp
_nm = _sp.check_output(["/opt/homebrew/bin/aarch64-elf-nm", os.path.join(D, "t631-q.elf")]).decode()
_out = [l.split()[0] for l in _nm.splitlines() if l.rstrip().endswith(" output")][0]
s.sendall(("pmemsave 0x" + _out + " 160 output.bin\n").encode())
resp = read_until_prompt(s)
# Strip ANSI escapes and show the tail (the result line).
clean = re.sub(rb"\x1b\[[0-9;]*[A-Za-z]", b"", resp)
print("pmemsave tail:", clean[-300:].decode(errors="replace"))
s.sendall(b"quit\n")
time.sleep(1)
s.close()
try:
    q.wait(timeout=10)
except subprocess.TimeoutExpired:
    q.kill()
print("output exists:", os.path.exists(OUT), "size:", os.path.getsize(OUT) if os.path.exists(OUT) else 0)
