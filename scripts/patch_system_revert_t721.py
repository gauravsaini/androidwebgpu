#!/usr/bin/env python3
"""
Step 3c revert-image script for Track T721.
Restores the ORIGINAL /system/bin/surfaceflinger (SHA 259d39f5) into a copy
of the T719 scaffold system.img (SHA ac15a68a), removing the 4-byte scaffold
patch at 0x204060. The resulting image differs from the T719 image ONLY in
surfaceflinger; every neighbour (incl. installd.rc trigger) stays
byte-identical. Uses the T719 Linux debugfs procedure: copy-then-patch
(base never modified), rm-first-then-write, explicit metadata + xattr,
e2fsck validation.
"""
import hashlib
import json
import subprocess
from pathlib import Path

BASE_IMG = Path("/home/muse/t719-work/t719-system.img")
WORK_DIR = Path("/home/muse/t721-work")
TARGET_IMG = WORK_DIR / "t721-system-revert.img"
REVERT_BIN = Path("/home/muse/t719-work/surfaceflinger_orig.bin")
REPORT_PATH = WORK_DIR / "t721_revert_report.json"

EXPECTED_BASE_SHA = "ac15a68ac1fb95c51eb3476f76692dfdda7e413e3d9e52f8cf8312b02a2ad5dd"
EXPECTED_BASE_SIZE = 2059816960
EXPECTED_SCAFFOLD_BIN_SHA = "fadee423d50c8da59918cc568e932c5e6dfe11553817625cdc82b77c6bc59312"
EXPECTED_ORIG_BIN_SHA = "259d39f54ef6bbac9cc90991f913ef0bd8cdcde5e9cbbd6a828b36fc6298061b"
EXPECTED_BIN_SIZE = 8336320
EXPECTED_XATTR = "u:object_r:surfaceflinger_exec:s0"
PATCH_OFFSET = 0x204060
ORIG_BYTES = bytes.fromhex("c0030054")
NEW_BYTES = bytes.fromhex("06000014")

DEBUGFS = "/usr/sbin/debugfs"
E2FSCK = "/usr/sbin/e2fsck"

BIN_PATH = "/system/bin/surfaceflinger"
INSTALLD_RC = "/system/etc/init/installd.rc"
EXPECTED_INSTALLD_RC_SHA = "3ea3466b"
EXPECTED_INSTALLD_RC_SIZE = 4958

NEIGHBOURS = [
    "/system/lib64/libgui.so",
    "/system/lib64/libui.so",
    "/system/lib64/libsync.so",
    "/system/lib64/android.hardware.graphics.allocator-V2-ndk.so",
    "/system/bin/dumpsys",
    "/system/bin/am",
    "/system/bin/cmd",
    "/system/bin/app_process64",
    INSTALLD_RC,
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


def main():
    print("=== T721 System Revert Image Patch Starting ===")
    WORK_DIR.mkdir(parents=True, exist_ok=True)
    assert BASE_IMG.is_file(), f"Base image not found: {BASE_IMG}"
    assert REVERT_BIN.is_file(), f"Revert binary not found: {REVERT_BIN}"

    print("[1] Verifying base image SHA + size...")
    base_sha = sha256_file(BASE_IMG)
    assert base_sha == EXPECTED_BASE_SHA, f"Base SHA mismatch: {base_sha}"
    assert BASE_IMG.stat().st_size == EXPECTED_BASE_SIZE
    print("[PASS] base is the T719 scaffold image.")

    print("[2] Dumping scaffold binary from base + verifying revert binary...")
    scaffold_bin = dump_file(BASE_IMG, BIN_PATH, WORK_DIR / "surfaceflinger_scaffold_dump.bin")
    assert len(scaffold_bin) == EXPECTED_BIN_SIZE
    assert hashlib.sha256(scaffold_bin).hexdigest() == EXPECTED_SCAFFOLD_BIN_SHA, "scaffold SHA mismatch"
    assert bytes(scaffold_bin[PATCH_OFFSET:PATCH_OFFSET + 4]) == NEW_BYTES, "scaffold bytes absent?!"
    revert = REVERT_BIN.read_bytes()
    assert len(revert) == EXPECTED_BIN_SIZE, f"revert size {len(revert)}"
    assert hashlib.sha256(revert).hexdigest() == EXPECTED_ORIG_BIN_SHA, "revert binary SHA mismatch"
    assert bytes(revert[PATCH_OFFSET:PATCH_OFFSET + 4]) == ORIG_BYTES, "revert bytes wrong?!"
    diffs = [i for i in range(len(scaffold_bin)) if scaffold_bin[i] != revert[i]]
    print(f"Revert-vs-scaffold diffs: {[hex(d) for d in diffs]}")
    assert diffs == [PATCH_OFFSET, PATCH_OFFSET + 1, PATCH_OFFSET + 3], "diff confinement failed"
    print("[PASS] revert binary is the original SF; only the 4 scaffold bytes differ.")

    neighbour_shas = {}
    for gp in NEIGHBOURS:
        b = dump_file(BASE_IMG, gp, WORK_DIR / "nb_base.tmp")
        neighbour_shas[gp] = hashlib.sha256(b).hexdigest()
        print(f"Neighbour {gp}: {len(b)} B SHA {neighbour_shas[gp][:16]}...")
    installd = dump_file(BASE_IMG, INSTALLD_RC, WORK_DIR / "installd_rc_orig.rc")
    assert len(installd) == EXPECTED_INSTALLD_RC_SIZE, f"installd.rc size {len(installd)}"
    assert hashlib.sha256(installd).hexdigest().startswith(EXPECTED_INSTALLD_RC_SHA), "installd.rc SHA mismatch"
    assert b"start installd" in installd, "installd trigger line missing"
    print("[PASS] installd.rc verified (trigger preserved).")

    stat_orig = run_debugfs(f"stat {BIN_PATH}", BASE_IMG)
    print(stat_orig)
    assert "Mode:  0755" in stat_orig and "User:     0" in stat_orig and "Group:  2000" in stat_orig
    ea_orig = run_debugfs(f"ea_get {BIN_PATH} security.selinux", BASE_IMG)
    assert EXPECTED_XATTR in ea_orig, f"xattr mismatch: {ea_orig!r}"
    print("[PASS] scaffold binary metadata + xattr confirmed.")

    print(f"[3] Copying base image to {TARGET_IMG}...")
    subprocess.run(["cp", str(BASE_IMG), str(TARGET_IMG)], check=True)
    assert TARGET_IMG.stat().st_size == BASE_IMG.stat().st_size

    print(f"[4] rm-first-then-write for {BIN_PATH}...")
    run_debugfs(f"rm {BIN_PATH}", TARGET_IMG, write=True)
    out = run_debugfs(f"write {REVERT_BIN} {BIN_PATH}", TARGET_IMG, write=True)
    print(out)
    new_inode = None
    for line in out.splitlines():
        if "Allocated inode:" in line:
            new_inode = line.split("Allocated inode:")[1].strip()
            break
    print(f"Allocated inode: {new_inode} (reuse not required, system.img has free inodes)")
    assert new_inode, "Failed to get allocated inode"
    run_debugfs(f"set_inode_field <{new_inode}> mode 0100755", TARGET_IMG, write=True)
    run_debugfs(f"set_inode_field <{new_inode}> uid 0", TARGET_IMG, write=True)
    run_debugfs(f"set_inode_field <{new_inode}> gid 2000", TARGET_IMG, write=True)
    run_debugfs(f"ea_set <{new_inode}> security.selinux {EXPECTED_XATTR}", TARGET_IMG, write=True)

    print("[5] Running e2fsck -f -y (bitmap repair pass)...")
    res1 = subprocess.run([E2FSCK, "-f", "-y", str(TARGET_IMG)], capture_output=True, text=True)
    print(f"e2fsck -f -y code: {res1.returncode}")
    print(res1.stdout[-1500:])

    print("[6] Running e2fsck -f -n (must be 0 cleanly)...")
    res2 = subprocess.run([E2FSCK, "-f", "-n", str(TARGET_IMG)], capture_output=True, text=True)
    print(f"e2fsck -f -n code: {res2.returncode}")
    if res2.returncode != 0:
        print("e2fsck -f -n nonzero; running one more -f -y then -f -n...")
        subprocess.run([E2FSCK, "-f", "-y", str(TARGET_IMG)], capture_output=True, text=True)
        res2 = subprocess.run([E2FSCK, "-f", "-n", str(TARGET_IMG)], capture_output=True, text=True)
        print(f"e2fsck -f -n retry code: {res2.returncode}")
    assert res2.returncode == 0, f"e2fsck clean check failed: {res2.returncode}\n{res2.stdout}"
    print(res2.stdout[-1200:])

    print("[7] Dumping reverted binary and byte-comparing...")
    dumped = dump_file(TARGET_IMG, BIN_PATH, WORK_DIR / "surfaceflinger_revert_dumped.bin")
    assert dumped == revert, "Dumped binary differs from revert binary"
    print("[PASS] Dumped binary byte-identical to original SF.")

    print("[8] Verifying metadata + xattr on reverted binary...")
    stat_out = run_debugfs(f"stat {BIN_PATH}", TARGET_IMG)
    assert "Mode:  0755" in stat_out, f"mode mismatch:\n{stat_out}"
    assert "User:     0" in stat_out, "uid mismatch"
    assert "Group:  2000" in stat_out, "gid mismatch"
    assert f"Size: {EXPECTED_BIN_SIZE}" in stat_out, "size mismatch"
    ea_out = run_debugfs(f"ea_get {BIN_PATH} security.selinux", TARGET_IMG)
    assert EXPECTED_XATTR in ea_out, f"xattr mismatch: {ea_out!r}"
    print("[PASS] Reverted binary: mode/uid/gid/size/xattr correct.")

    print("[9] Verifying neighbour files byte-identical to base...")
    for gp in NEIGHBOURS:
        new_bytes = dump_file(TARGET_IMG, gp, WORK_DIR / "nb_new.tmp")
        assert hashlib.sha256(new_bytes).hexdigest() == neighbour_shas[gp], f"Neighbour changed: {gp}"
        print(f"[PASS] {gp} unchanged ({len(new_bytes)} B).")

    print("[10] Verifying base image untouched...")
    assert sha256_file(BASE_IMG) == EXPECTED_BASE_SHA, "Base image was modified!"

    target_sha = sha256_file(TARGET_IMG)
    target_size = TARGET_IMG.stat().st_size
    print(f"Revert system.img SHA-256: {target_sha} ({target_size} bytes)")
    assert target_size == EXPECTED_BASE_SIZE, f"Image size changed: {target_size}"

    report = {
        "verdict": "PASS",
        "base_img_sha256": EXPECTED_BASE_SHA,
        "target_img_path": str(TARGET_IMG),
        "target_img_sha256": target_sha,
        "target_img_size": target_size,
        "scaffold_bin_sha256": EXPECTED_SCAFFOLD_BIN_SHA,
        "revert_bin_sha256": EXPECTED_ORIG_BIN_SHA,
        "bin_size": EXPECTED_BIN_SIZE,
        "bin_inode_new": new_inode,
        "xattr": EXPECTED_XATTR,
        "patch_offset": hex(PATCH_OFFSET),
        "orig_bytes": ORIG_BYTES.hex(),
        "scaffold_bytes": NEW_BYTES.hex(),
        "neighbours_verified": NEIGHBOURS,
        "e2fsck_n_exit_code": res2.returncode,
    }
    with open(REPORT_PATH, "w") as f:
        json.dump(report, f, indent=2)
    print(f"[SUCCESS] T721 revert image completed and verified. Report: {REPORT_PATH}")


if __name__ == "__main__":
    main()
