#!/usr/bin/env python3
"""
Verification Script for Track T706 Step 2 Patch Proofs (G706-2)
Validates:
1. system.img SHA-256 is 2de0533e... and size is 2,059,816,960 bytes
2. Block 145414 reads 4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea
3. Launcher3QuickStep.apk extracted from image matches 8a28e1db...
4. Patched opcodes in classes7.dex are verified (0x199a10=00000000, 0x199a90=1200, 0x19b112=0102000000000000)
5. oat/arm64 directory is absent
6. services.jar is byte-identical to original clean base (d4108052...)
7. e2fsck -f -y passes with returncode 0
"""

import hashlib
import json
import os
import subprocess
import sys
import zipfile
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track706-clean-gsi-hwc-hotplug")
SYSTEM_IMG = WORKTREE / "guest-inputs" / "system.img"
REPORT_FILE = WORKTREE / "evidence" / "patch_proof_verified.json"

EXPECTED_IMG_SHA256 = "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601"
EXPECTED_IMG_SIZE = 2059816960
EXPECTED_BLK145414_SHA256 = "4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea"
EXPECTED_APK_SHA256 = "8a28e1dbb416f1f6a3279082279dd9441db53cad10353892f8a32c08a85cd97c"
EXPECTED_SERVICES_SHA256 = "d41080524ff81b8693a453f4d5cfe8a119be433f3e375a0780683acc71f20d6e"

DEBUGFS = "/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/debugfs"
E2FSCK = "/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/e2fsck"

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

def main():
    print("=== T706 Mac Patch Proof Verification ===")
    assert SYSTEM_IMG.is_file(), f"system.img missing: {SYSTEM_IMG}"
    
    # 1. Image Size & SHA
    size = SYSTEM_IMG.stat().st_size
    print(f"system.img size: {size} bytes")
    assert size == EXPECTED_IMG_SIZE, f"Size mismatch: {size} vs {EXPECTED_IMG_SIZE}"
    
    sha = sha256_file(SYSTEM_IMG)
    print(f"system.img SHA-256: {sha}")
    assert sha == EXPECTED_IMG_SHA256, f"SHA mismatch: {sha} vs {EXPECTED_IMG_SHA256}"
    
    # 2. Block 145414
    blk_sha = check_block145414(SYSTEM_IMG)
    print(f"Block 145414 SHA-256: {blk_sha}")
    assert blk_sha == EXPECTED_BLK145414_SHA256, f"Block 145414 corrupted: {blk_sha}"
    
    # 3. e2fsck clean check (read-only -n to preserve authoritative SHA-256)
    print("Running e2fsck on image...")
    res = subprocess.run([E2FSCK, "-f", "-n", str(SYSTEM_IMG)], capture_output=True, text=True)
    print(f"e2fsck returncode: {res.returncode}")
    assert res.returncode == 0, f"e2fsck reported issues: {res.stdout}"
    
    # 4. Dump APK
    tmp_apk = WORKTREE / "evidence" / "verified_extracted_apk.apk"
    subprocess.run([DEBUGFS, "-R", f"dump system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk {tmp_apk}", str(SYSTEM_IMG)], check=True, capture_output=True)
    extracted_apk_sha = sha256_file(tmp_apk)
    print(f"Extracted APK SHA-256: {extracted_apk_sha}")
    assert extracted_apk_sha == EXPECTED_APK_SHA256, f"Extracted APK SHA mismatch: {extracted_apk_sha}"
    
    # 5. Verify patched opcodes
    with zipfile.ZipFile(tmp_apk, "r") as z:
        dex = z.read("classes7.dex")
        p1 = dex[0x199a10:0x199a14].hex()
        p2 = dex[0x199a90:0x199a92].hex()
        p3 = dex[0x19b112:0x19b11a].hex()
        print(f"classes7.dex opcodes: 0x199a10={p1}, 0x199a90={p2}, 0x19b112={p3}")
        assert p1 == "00000000", f"p1 mismatch: {p1}"
        assert p2 == "1200", f"p2 mismatch: {p2}"
        assert p3 == "0102000000000000", f"p3 mismatch: {p3}"
    print("[PASS] classes7.dex opcodes verified exact.")
    
    # 6. Verify oat absent
    ls_out = subprocess.run([DEBUGFS, "-R", "ls -l system/system_ext/priv-app/Launcher3QuickStep", str(SYSTEM_IMG)], capture_output=True, text=True, check=True).stdout
    assert "oat" not in ls_out, f"oat directory present:\n{ls_out}"
    print("[PASS] oat directory confirmed absent.")
    
    # 7. Dump services.jar
    tmp_services = WORKTREE / "evidence" / "verified_extracted_services.jar"
    subprocess.run([DEBUGFS, "-R", f"dump system/framework/services.jar {tmp_services}", str(SYSTEM_IMG)], check=True, capture_output=True)
    extracted_services_sha = sha256_file(tmp_services)
    print(f"Extracted services.jar SHA-256: {extracted_services_sha}")
    assert extracted_services_sha == EXPECTED_SERVICES_SHA256, f"services.jar mismatch: {extracted_services_sha}"
    print("[PASS] services.jar byte-identical to original clean base.")
    
    proofs = {
        "verdict": "PASS",
        "system_img_sha256": sha,
        "system_img_size": size,
        "block_145414_sha256": blk_sha,
        "block_145414_clean": True,
        "apk_sha256": extracted_apk_sha,
        "opcodes_verified": True,
        "oat_dir_absent": True,
        "services_jar_sha256": extracted_services_sha,
        "e2fsck_exit_code": 0
    }
    with open(REPORT_FILE, "w") as f:
        json.dump(proofs, f, indent=2)
    print(f"[ALL GATES PASSED] Proofs saved to {REPORT_FILE}")

if __name__ == "__main__":
    main()
