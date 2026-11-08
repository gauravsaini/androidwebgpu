#!/usr/bin/env python3
"""
Step 2 Patching Script for Track T706
Patches system.img on Linux safely without clobbering shared_blocks (block 145414 in services.jar).
"""

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

BASE_IMG = Path("/mnt/sdb1/kernel-critical/guest-inputs/system.img")
TARGET_IMG = Path("/mnt/sdb1/kernel-critical/lane-images/t706-system.img")
PATCHED_APK = Path("/mnt/sdb1/kernel-critical/lane-images/patched_Launcher3QuickStep.apk")
REPORT_PATH = Path("/mnt/sdb1/kernel-critical/lane-images/t706_patch_report.json")

EXPECTED_BASE_SHA = "98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d"
EXPECTED_BLK145414_SHA = "4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea"
EXPECTED_APK_SHA = "8a28e1dbb416f1f6a3279082279dd9441db53cad10353892f8a32c08a85cd97c"

DEBUGFS = "/usr/sbin/debugfs"
E2FSCK = "/usr/sbin/e2fsck"

def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def check_block145414(p: Path) -> str:
    with open(p, "rb") as f:
        f.seek(145414 * 4096)
        data = f.read(4096)
    return hashlib.sha256(data).hexdigest()

def run_debugfs(cmd: str, img: Path) -> str:
    res = subprocess.run([DEBUGFS, "-w", "-R", cmd, str(img)], capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"debugfs failed: {cmd}\nSTDOUT: {res.stdout}\nSTDERR: {res.stderr}")
    return res.stdout

def run_debugfs_ro(cmd: str, img: Path) -> str:
    res = subprocess.run([DEBUGFS, "-R", cmd, str(img)], capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"debugfs failed: {cmd}\nSTDOUT: {res.stdout}\nSTDERR: {res.stderr}")
    return res.stdout

def main():
    print("=== T706 Linux System Image Patch Starting ===")
    
    assert BASE_IMG.is_file(), f"Base image not found: {BASE_IMG}"
    assert PATCHED_APK.is_file(), f"Patched APK not found: {PATCHED_APK}"
    
    # 1. Verify clean base
    print("[1] Verifying clean base block 145414...")
    base_blk = check_block145414(BASE_IMG)
    print(f"Base block 145414 SHA: {base_blk}")
    assert base_blk == EXPECTED_BLK145414_SHA, f"Base block 145414 corrupted: {base_blk}"
    
    print("[2] Verifying patched APK SHA...")
    apk_sha = sha256_file(PATCHED_APK)
    print(f"Patched APK SHA: {apk_sha}")
    assert apk_sha == EXPECTED_APK_SHA, f"APK SHA mismatch: {apk_sha}"
    
    # 2. Copy base to target
    print(f"[3] Copying {BASE_IMG} to {TARGET_IMG}...")
    subprocess.run(["cp", "--reflink=auto", str(BASE_IMG), str(TARGET_IMG)], check=True)
    
    # 3. Apply patch with extent-safe ordering:
    # First write new APK as Launcher3QuickStep_new.apk while old APK is still present!
    # This guarantees naive block allocator allocates fresh blocks and DOES NOT reuse shared block 145414!
    print("[4] Writing new APK as Launcher3QuickStep_new.apk...")
    out = run_debugfs(f"write {PATCHED_APK} system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep_new.apk", TARGET_IMG)
    # Extract allocated inode
    new_inode = None
    for line in out.splitlines():
        if "Allocated inode:" in line:
            new_inode = line.split("Allocated inode:")[1].strip()
            break
    print(f"New inode allocated: {new_inode}")
    assert new_inode, "Failed to get allocated inode"
    
    # Set metadata on new inode
    print("[5] Setting permissions and SELinux label...")
    run_debugfs(f"set_inode_field <{new_inode}> mode 0100644", TARGET_IMG)
    run_debugfs(f"set_inode_field <{new_inode}> uid 0", TARGET_IMG)
    run_debugfs(f"set_inode_field <{new_inode}> gid 0", TARGET_IMG)
    run_debugfs(f"ea_set <{new_inode}> security.selinux u:object_r:system_file:s0", TARGET_IMG)
    
    # Unlink old APK
    print("[6] Replacing Launcher3QuickStep.apk link...")
    run_debugfs("rm system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk", TARGET_IMG)
    run_debugfs(f"link <{new_inode}> system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk", TARGET_IMG)
    run_debugfs("unlink system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep_new.apk", TARGET_IMG)
    
    # Remove oat directory (T703 de-shadow)
    print("[7] Deleting oat/arm64 AOT shadow directory...")
    run_debugfs("rm system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.odex", TARGET_IMG)
    run_debugfs("rm system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.vdex", TARGET_IMG)
    run_debugfs("rmdir system/system_ext/priv-app/Launcher3QuickStep/oat/arm64", TARGET_IMG)
    run_debugfs("rmdir system/system_ext/priv-app/Launcher3QuickStep/oat", TARGET_IMG)
    
    # 4. Consistency checks with e2fsck
    print("[8] Running e2fsck Pass 1 (repairs bitmaps)...")
    res1 = subprocess.run([E2FSCK, "-f", "-y", str(TARGET_IMG)], capture_output=True, text=True)
    print(f"e2fsck Pass 1 code: {res1.returncode}")
    print(res1.stdout)
    
    print("[9] Running e2fsck Pass 2 (must be 0 cleanly)...")
    res2 = subprocess.run([E2FSCK, "-f", "-y", str(TARGET_IMG)], capture_output=True, text=True)
    print(f"e2fsck Pass 2 code: {res2.returncode}")
    assert res2.returncode == 0, f"e2fsck failed clean check: {res2.returncode}\n{res2.stdout}"
    
    # 5. Proofs
    print("[10] Verifying block 145414 in patched image...")
    new_blk = check_block145414(TARGET_IMG)
    print(f"Patched image block 145414 SHA: {new_blk}")
    assert new_blk == EXPECTED_BLK145414_SHA, f"CRITICAL: block 145414 corrupted! Got {new_blk}"
    
    print("[11] Dumping and verifying APK from patched image...")
    tmp_dump_apk = Path("/tmp/t706_extracted_apk.apk")
    run_debugfs_ro(f"dump system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk {tmp_dump_apk}", TARGET_IMG)
    dump_apk_sha = sha256_file(tmp_dump_apk)
    print(f"Extracted APK SHA: {dump_apk_sha}")
    assert dump_apk_sha == EXPECTED_APK_SHA, f"Extracted APK SHA mismatch: {dump_apk_sha}"
    tmp_dump_apk.unlink()
    
    print("[12] Verifying oat directory is absent...")
    ls_out = run_debugfs_ro("ls -l system/system_ext/priv-app/Launcher3QuickStep", TARGET_IMG)
    assert "oat" not in ls_out, f"oat directory still present:\n{ls_out}"
    print("[PASS] oat directory confirmed absent.")
    
    print("[13] Verifying services.jar intact...")
    tmp_dump_services = Path("/tmp/t706_extracted_services.jar")
    run_debugfs_ro(f"dump system/framework/services.jar {tmp_dump_services}", TARGET_IMG)
    services_sha = sha256_file(tmp_dump_services)
    print(f"Extracted services.jar SHA: {services_sha}")
    # Compare with services.jar from base
    tmp_base_services = Path("/tmp/t706_base_services.jar")
    run_debugfs_ro(f"dump system/framework/services.jar {tmp_base_services}", BASE_IMG)
    base_services_sha = sha256_file(tmp_base_services)
    assert services_sha == base_services_sha, f"services.jar mismatch: {services_sha} vs {base_services_sha}"
    tmp_dump_services.unlink()
    tmp_base_services.unlink()
    print("[PASS] services.jar confirmed byte-identical to clean base.")
    
    target_sha = sha256_file(TARGET_IMG)
    target_size = TARGET_IMG.stat().st_size
    print(f"Patched system.img SHA-256: {target_sha} ({target_size} bytes)")
    
    report = {
        "verdict": "PASS",
        "target_img_path": str(TARGET_IMG),
        "target_img_sha256": target_sha,
        "target_img_size": target_size,
        "block_145414_sha256": new_blk,
        "block_145414_expected": EXPECTED_BLK145414_SHA,
        "patched_apk_sha256": dump_apk_sha,
        "services_jar_sha256": services_sha,
        "oat_dir_absent": True,
        "e2fsck_exit_code": res2.returncode,
    }
    with open(REPORT_PATH, "w") as f:
        json.dump(report, f, indent=2)
    print(f"[SUCCESS] Step 2 patch completed and verified. Report saved to {REPORT_PATH}")

if __name__ == "__main__":
    main()
