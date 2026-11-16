#!/usr/bin/env python3
"""
Step 2 patch script for Track T721.
Patches the T718 vendor.img (base SHA bac3cdc1) in TWO files:
  1. /bin/hw/android.hardware.graphics.composer@2.1-service (19942712 B):
     orig SHA 8d62a642 -> patched SHA f7600821 (trampoline in repurposed
     sync_file_info + .bss extension; same size).
  2. /etc/init/android.hardware.graphics.composer@2.1-service.rc (259 B):
     appends an `on post-fs-data` trigger mounting debugfs at
     /sys/kernel/debug so the composer can open the sw_sync timeline.
     The service runs WITHOUT CAP_SYS_ADMIN, so init must do the mount.
Uses the T717/T718 Linux debugfs procedure: rm-first-then-write (vendor.img
has ZERO free inodes), explicit metadata + xattr, e2fsck validation.
The base image is never modified. The T718 shim (3daa5a49), the T716
SYS_RAWIO lines, and every neighbour file must stay byte-identical.
"""
import hashlib
import json
import subprocess
from pathlib import Path

BASE_IMG = Path("/home/muse/t718-work/t718-vendor.img")
WORK_DIR = Path("/home/muse/t721-work")
TARGET_IMG = WORK_DIR / "t721-vendor.img"
PATCHED_BIN = WORK_DIR / "composer-t721-patched.bin"
REPORT_PATH = WORK_DIR / "t721_patch_report.json"

EXPECTED_BASE_SHA = "bac3cdc112dc6c3257c016fb4f600c46f43b9372f3d2f2ebb9b7d79fd05daeca"
EXPECTED_ORIG_BIN_SHA = "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401"
EXPECTED_PATCHED_BIN_SHA = "f7600821894b923f5a014143ac6ea57dec6e1f2d448ec78f6d17d983159e2ea5"
EXPECTED_BIN_SIZE = 19942712
EXPECTED_BIN_INODE = "93"
EXPECTED_BIN_XATTR = "u:object_r:hal_graphics_composer_default_exec:s0"
EXPECTED_RC1_SHA = "47a9bd4a4415816db862f510cd71ee2c8286f9d98eff81315d87485142f966db"
EXPECTED_RC_SIZE = 259
EXPECTED_RC_INODE = "305"
EXPECTED_RC_XATTR = "u:object_r:vendor_configs_file:s0"
EXPECTED_RC2_SHA = "468f2a85090202608cd6da581f45988f3ee40519029514cad4a95899eec4cc82"
EXPECTED_SHIM_SHA = "3daa5a491e9d04ff56bbb7998be1eb91a1b3ed625d66b48f264a04b04ca70932"
EXPECTED_SHIM_SIZE = 1711816
EXPECTED_HIDL_ALLOC_SHA = "91d84fb9ddc80bb77ddae861498c8c96c91dcf8bde3a5bd28a68823df97fe552"

DEBUGFS = "/usr/sbin/debugfs"
E2FSCK = "/usr/sbin/e2fsck"

BIN_PATH = "/bin/hw/android.hardware.graphics.composer@2.1-service"
RC_PATH = "/etc/init/android.hardware.graphics.composer@2.1-service.rc"
SHIM_PATH = "/bin/hw/android.hardware.graphics.allocator-service.t628"
TRIGGER = b"\non post-fs-data\n    mount debugfs debugfs /sys/kernel/debug\n"

NEIGHBOURS = [
    "/etc/init/hw/init.virt.rc",
    "/etc/init/android.hardware.graphics.allocator-service.t628.rc",
    "/etc/init/hw/init.ranchu.rc",
    "/etc/vintf/manifest.xml",
    "/etc/init/android.hardware.graphics.allocator@2.0-service.rc",
    "/lib64/hw/android.hardware.graphics.mapper@2.0-impl.so",
    "/lib64/hw/mapper.default.so",
]


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()


def run_debugfs(cmd: str, img: Path, write: bool = False) -> str:
    args = [DEBUGFS]
    if write:
        args.append("-w")
    args += ["-R", cmd, str(img)]
    res = subprocess.run(args, capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"debugfs failed: {cmd}\nSTDOUT: {res.stdout}\nSTDERR: {res.stderr}")
    return res.stdout


def dump_file(img: Path, guest_path: str, dest: Path) -> bytes:
    run_debugfs(f"dump {guest_path} {dest}", img)
    return dest.read_bytes()


def rm_write(img: Path, host_file: Path, guest_path: str, expected_inode: str,
             mode: str, uid: int, gid: int, xattr: str) -> str:
    run_debugfs(f"rm {guest_path}", img, write=True)
    out = run_debugfs(f"write {host_file} {guest_path}", img, write=True)
    new_inode = None
    for line in out.splitlines():
        if "Allocated inode:" in line:
            new_inode = line.split("Allocated inode:")[1].strip()
            break
    print(f"Allocated inode for {guest_path}: {new_inode}")
    assert new_inode, "Failed to get allocated inode"
    assert new_inode == expected_inode, f"inode not reused: {new_inode} != {expected_inode}"
    run_debugfs(f"set_inode_field <{new_inode}> mode {mode}", img, write=True)
    run_debugfs(f"set_inode_field <{new_inode}> uid {uid}", img, write=True)
    run_debugfs(f"set_inode_field <{new_inode}> gid {gid}", img, write=True)
    run_debugfs(f"ea_set <{new_inode}> security.selinux {xattr}", img, write=True)
    return new_inode


def main():
    print("=== T721 Linux Vendor Image Patch Starting ===")
    WORK_DIR.mkdir(parents=True, exist_ok=True)
    assert BASE_IMG.is_file(), f"Base image not found: {BASE_IMG}"
    assert PATCHED_BIN.is_file(), f"Patched binary not found: {PATCHED_BIN}"

    print("[1] Verifying base image + patched binary SHAs...")
    base_sha = sha256_file(BASE_IMG)
    assert base_sha == EXPECTED_BASE_SHA, f"Base SHA mismatch: {base_sha}"
    patched_sha = sha256_file(PATCHED_BIN)
    assert patched_sha == EXPECTED_PATCHED_BIN_SHA, f"Patched SHA mismatch: {patched_sha}"
    assert PATCHED_BIN.stat().st_size == EXPECTED_BIN_SIZE
    print("[PASS] base + patched binary SHAs match.")

    print("[2] Dumping originals from base image...")
    orig_bin = dump_file(BASE_IMG, BIN_PATH, WORK_DIR / "composer_orig_dump.bin")
    assert len(orig_bin) == EXPECTED_BIN_SIZE
    assert hashlib.sha256(orig_bin).hexdigest() == EXPECTED_ORIG_BIN_SHA, "orig ELF SHA mismatch"
    rc1_orig = dump_file(BASE_IMG, RC_PATH, WORK_DIR / "composer_rc_orig.rc")
    assert len(rc1_orig) == EXPECTED_RC_SIZE, f"rc size {len(rc1_orig)}"
    assert hashlib.sha256(rc1_orig).hexdigest() == EXPECTED_RC1_SHA, "RC1 SHA mismatch"
    assert rc1_orig.count(b"capabilities SYS_NICE SYS_RAWIO\n") == 1, "RC1 must carry SYS_RAWIO"
    assert b"debugfs" not in rc1_orig, "RC1 already mentions debugfs?!"
    rc2_orig = dump_file(BASE_IMG, "/etc/init/hw/init.virt.rc", WORK_DIR / "init_virt_orig.rc")
    assert hashlib.sha256(rc2_orig).hexdigest() == EXPECTED_RC2_SHA, "RC2 SHA mismatch"
    shim_orig = dump_file(BASE_IMG, SHIM_PATH, WORK_DIR / "shim_orig.bin")
    assert len(shim_orig) == EXPECTED_SHIM_SIZE
    assert hashlib.sha256(shim_orig).hexdigest() == EXPECTED_SHIM_SHA, "shim SHA mismatch"
    stat_bin = run_debugfs(f"stat {BIN_PATH}", BASE_IMG)
    assert "Mode:  0755" in stat_bin and "User:     0" in stat_bin and "Group:  2000" in stat_bin
    ea_bin = run_debugfs(f"ea_get {BIN_PATH} security.selinux", BASE_IMG)
    assert EXPECTED_BIN_XATTR in ea_bin, f"ELF xattr mismatch: {ea_bin!r}"
    stat_rc = run_debugfs(f"stat {RC_PATH}", BASE_IMG)
    assert "Mode:  0644" in stat_rc and "User:     0" in stat_rc and "Group:     0" in stat_rc
    ea_rc = run_debugfs(f"ea_get {RC_PATH} security.selinux", BASE_IMG)
    assert EXPECTED_RC_XATTR in ea_rc, f"RC xattr mismatch: {ea_rc!r}"
    print("[PASS] originals + metadata + xattrs confirmed.")

    print("[3] Building new .rc content...")
    rc_new = rc1_orig
    if not rc_new.endswith(b"\n"):
        rc_new += b"\n"
    rc_new += TRIGGER
    assert rc_new.startswith(rc1_orig), "new .rc must extend the original"
    assert rc_new.count(b"mount debugfs debugfs /sys/kernel/debug\n") == 1
    rc_new_path = WORK_DIR / "composer_rc_new.rc"
    rc_new_path.write_bytes(rc_new)
    rc_new_sha = hashlib.sha256(rc_new).hexdigest()
    print(f"new .rc: {len(rc_new)} B SHA {rc_new_sha}")
    print(rc_new.decode())

    print(f"[4] Copying {BASE_IMG} to {TARGET_IMG}...")
    subprocess.run(["cp", str(BASE_IMG), str(TARGET_IMG)], check=True)
    assert TARGET_IMG.stat().st_size == BASE_IMG.stat().st_size

    print(f"[5] rm-first-then-write for {BIN_PATH}...")
    rm_write(TARGET_IMG, PATCHED_BIN, BIN_PATH, EXPECTED_BIN_INODE, "0100755", 0, 2000,
             EXPECTED_BIN_XATTR)
    print(f"[6] rm-first-then-write for {RC_PATH}...")
    rm_write(TARGET_IMG, rc_new_path, RC_PATH, EXPECTED_RC_INODE, "0100644", 0, 0,
             EXPECTED_RC_XATTR)

    print("[7] Running e2fsck -f -y (bitmap repair pass)...")
    res1 = subprocess.run([E2FSCK, "-f", "-y", str(TARGET_IMG)], capture_output=True, text=True)
    print(f"e2fsck -f -y code: {res1.returncode}")
    print(res1.stdout[-1500:])

    print("[8] Running e2fsck -f -n (must be 0 cleanly)...")
    res2 = subprocess.run([E2FSCK, "-f", "-n", str(TARGET_IMG)], capture_output=True, text=True)
    print(f"e2fsck -f -n code: {res2.returncode}")
    if res2.returncode != 0:
        print("e2fsck -f -n nonzero; running one more -f -y then -f -n...")
        subprocess.run([E2FSCK, "-f", "-y", str(TARGET_IMG)], capture_output=True, text=True)
        res2 = subprocess.run([E2FSCK, "-f", "-n", str(TARGET_IMG)], capture_output=True, text=True)
        print(f"e2fsck -f -n retry code: {res2.returncode}")
    assert res2.returncode == 0, f"e2fsck clean check failed: {res2.returncode}\n{res2.stdout}"
    print(res2.stdout[-1200:])

    print("[9] Dumping patched files and byte-comparing...")
    dumped_bin = dump_file(TARGET_IMG, BIN_PATH, WORK_DIR / "composer_dumped.bin")
    assert dumped_bin == PATCHED_BIN.read_bytes(), "Dumped ELF differs from patched binary"
    dumped_rc = dump_file(TARGET_IMG, RC_PATH, WORK_DIR / "composer_rc_dumped.rc")
    assert dumped_rc == rc_new, "Dumped .rc differs from new .rc"
    print("[PASS] Dumped ELF + .rc byte-identical to staged files.")

    print("[10] Verifying metadata + xattr on patched files...")
    stat_bin2 = run_debugfs(f"stat {BIN_PATH}", TARGET_IMG)
    assert "Mode:  0755" in stat_bin2 and "User:     0" in stat_bin2 and "Group:  2000" in stat_bin2
    assert f"Size: {EXPECTED_BIN_SIZE}" in stat_bin2
    assert EXPECTED_BIN_XATTR in run_debugfs(f"ea_get {BIN_PATH} security.selinux", TARGET_IMG)
    stat_rc2 = run_debugfs(f"stat {RC_PATH}", TARGET_IMG)
    assert "Mode:  0644" in stat_rc2 and "User:     0" in stat_rc2 and "Group:     0" in stat_rc2
    assert f"Size: {len(rc_new)}" in stat_rc2
    assert EXPECTED_RC_XATTR in run_debugfs(f"ea_get {RC_PATH} security.selinux", TARGET_IMG)
    print("[PASS] metadata + xattrs correct on both patched files.")

    print("[11] Verifying neighbour files byte-identical to base...")
    for gp in NEIGHBOURS:
        base_bytes = dump_file(BASE_IMG, gp, WORK_DIR / "nb_base.tmp")
        new_bytes = dump_file(TARGET_IMG, gp, WORK_DIR / "nb_new.tmp")
        assert base_bytes == new_bytes, f"Neighbour changed: {gp}"
        print(f"[PASS] {gp} unchanged ({len(base_bytes)} B).")

    print("[12] Verifying shim + HIDL allocator byte-identical...")
    shim_new = dump_file(TARGET_IMG, SHIM_PATH, WORK_DIR / "shim_new.bin")
    assert hashlib.sha256(shim_new).hexdigest() == EXPECTED_SHIM_SHA, "shim changed!"
    hidl_new = dump_file(TARGET_IMG, "/bin/hw/android.hardware.graphics.allocator@2.0-service",
                         WORK_DIR / "hidl_alloc_new.bin")
    assert hashlib.sha256(hidl_new).hexdigest() == EXPECTED_HIDL_ALLOC_SHA, "HIDL allocator changed!"
    print("[PASS] shim (3daa5a49) + HIDL allocator byte-identical.")

    print("[13] Verifying base image untouched...")
    assert sha256_file(BASE_IMG) == EXPECTED_BASE_SHA, "Base image was modified!"

    target_sha = sha256_file(TARGET_IMG)
    target_size = TARGET_IMG.stat().st_size
    print(f"Patched vendor.img SHA-256: {target_sha} ({target_size} bytes)")
    assert target_size == 134217728, f"Image size changed: {target_size}"

    report = {
        "verdict": "PASS",
        "base_img_sha256": EXPECTED_BASE_SHA,
        "target_img_path": str(TARGET_IMG),
        "target_img_sha256": target_sha,
        "target_img_size": target_size,
        "orig_bin_sha256": EXPECTED_ORIG_BIN_SHA,
        "patched_bin_sha256": EXPECTED_PATCHED_BIN_SHA,
        "bin_size": EXPECTED_BIN_SIZE,
        "bin_inode": EXPECTED_BIN_INODE,
        "bin_xattr": EXPECTED_BIN_XATTR,
        "rc_orig_sha256": EXPECTED_RC1_SHA,
        "rc_new_sha256": rc_new_sha,
        "rc_new_size": len(rc_new),
        "rc_inode": EXPECTED_RC_INODE,
        "rc_xattr": EXPECTED_RC_XATTR,
        "shim_sha256": EXPECTED_SHIM_SHA,
        "hidl_alloc_sha256": EXPECTED_HIDL_ALLOC_SHA,
        "neighbours_verified": NEIGHBOURS,
        "e2fsck_n_exit_code": res2.returncode,
    }
    with open(REPORT_PATH, "w") as f:
        json.dump(report, f, indent=2)
    print(f"[SUCCESS] T721 vendor patch completed and verified. Report: {REPORT_PATH}")


if __name__ == "__main__":
    main()
