#!/usr/bin/env python3
"""Build and apply the Launcher3 DotRenderer NPE fix for Track T702.

Dual-layer fix in classes7.dex of Launcher3QuickStep.apk:
1. Layer 1 (Root cause in BitmapRenderer):
   - At 0x199a10: nop out if-nez v0, 0009 so createHardwareBitmap always
     falls through to createSoftwareBitmap, allocating standard software ARGB_8888
     bitmaps via software Skia Canvas instead of failing on headless SwiftShader
     (ThreadedRenderer: Error reading image: -30001!).
   - At 0x199a90: set USE_HARDWARE_BITMAP = false (const/4 v0, #0).
2. Layer 2 (DotRenderer NPE guard):
   - At 0x19b112: replace invoke-virtual {v2}, Bitmap.getHeight() + move-result v2
     with move v2, v0 (v0 contains the pill height passed to createPill) + nops,
     ensuring DotRenderer.<init> can never invoke getHeight() on a null reference.

Pipeline:
- Preserves original system.img in evidence/originals/system.img (with SHA-256).
- Clones system.img to guest-inputs/system-fixed.img.
- Extracts, patches classes7.dex, recomputes Adler32 + SHA-1.
- Repacks Launcher3QuickStep.apk, zipaligns to 4-byte boundaries, signs with v1+v2+v3.
- Injects new APK into system-fixed.img using debugfs, sets 0644, uid/gid 0,
  SELinux label u:object_r:system_file:s0.
- Verifies filesystem consistency with e2fsck -f -y.
- Atomically replaces guest-inputs/system.img.
- Extracts from repacked system.img and verifies with dexdump that the fix is in-image.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import time
import zlib
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
GUEST_INPUTS = WORKTREE / "guest-inputs"
SYSTEM_IMG = GUEST_INPUTS / "system.img"
ORIGINALS = WORKTREE / "evidence" / "originals"
SCRATCH = Path("/tmp/t702_patch")

JAVA_HOME = Path("/opt/homebrew/Cellar/openjdk@17/17.0.16/libexec/openjdk.jdk/Contents/Home")
BUILD_TOOLS = Path("/opt/homebrew/share/android-commandlinetools/build-tools/34.0.0")
DEBUGFS = Path("/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/debugfs")
E2FSCK = Path("/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/e2fsck")

EXPECTED_ORIGINAL_SHA256 = "98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d"
EXPECTED_ORIGINAL_SIZE = 2059816960


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(8 * 1024 * 1024)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def run_cmd(cmd: list[str], env: dict | None = None) -> str:
    print(f"[CMD] {' '.join(str(c) for c in cmd)}")
    res = subprocess.run(cmd, capture_output=True, text=True, env=env)
    if res.returncode != 0:
        print(f"[ERROR] returncode={res.returncode}")
        print(f"STDOUT:\n{res.stdout}")
        print(f"STDERR:\n{res.stderr}")
        raise RuntimeError(f"Command failed: {' '.join(str(c) for c in cmd)}")
    return res.stdout


def main():
    t0 = time.monotonic()
    print(f"=== Track T702 Fix Builder Started ===")

    # 1. Verify environment and tools
    assert SYSTEM_IMG.is_file(), f"system.img missing: {SYSTEM_IMG}"
    assert DEBUGFS.is_file(), f"debugfs missing: {DEBUGFS}"
    assert E2FSCK.is_file(), f"e2fsck missing: {E2FSCK}"
    assert JAVA_HOME.is_dir(), f"JAVA_HOME missing: {JAVA_HOME}"
    assert (BUILD_TOOLS / "dexdump").is_file(), "dexdump missing"
    assert (BUILD_TOOLS / "zipalign").is_file(), "zipalign missing"
    assert (BUILD_TOOLS / "apksigner").is_file(), "apksigner missing"

    env = os.environ.copy()
    env["JAVA_HOME"] = str(JAVA_HOME)
    env["PATH"] = f"{JAVA_HOME}/bin:{BUILD_TOOLS}:{env.get('PATH', '')}"

    # 2. Preserve original bytes in evidence/originals/
    ORIGINALS.mkdir(parents=True, exist_ok=True)
    orig_backup = ORIGINALS / "system.img"
    if not orig_backup.exists():
        print(f"Hardlinking {SYSTEM_IMG} to {orig_backup}...")
        os.link(SYSTEM_IMG, orig_backup)
    
    orig_sha = sha256_file(orig_backup)
    orig_size = orig_backup.stat().st_size
    print(f"Original system.img SHA-256: {orig_sha} ({orig_size} bytes)")
    assert orig_sha == EXPECTED_ORIGINAL_SHA256, f"Original SHA mismatch: {orig_sha}"
    assert orig_size == EXPECTED_ORIGINAL_SIZE, f"Original size mismatch: {orig_size}"

    # 3. Setup scratch directory
    if SCRATCH.exists():
        shutil.rmtree(SCRATCH)
    SCRATCH.mkdir(parents=True)

    # 4. Dump Launcher3QuickStep.apk from system.img
    orig_apk = SCRATCH / "Launcher3QuickStep_orig.apk"
    print(f"Dumping Launcher3QuickStep.apk from system.img...")
    run_cmd([str(DEBUGFS), "-R", f"dump system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk {orig_apk}", str(SYSTEM_IMG)])
    assert orig_apk.is_file(), "Failed to dump Launcher3QuickStep.apk"

    # Also backup original APK in evidence/originals
    orig_apk_backup = ORIGINALS / "Launcher3QuickStep.apk"
    if not orig_apk_backup.exists():
        shutil.copy2(orig_apk, orig_apk_backup)
    orig_apk_sha = sha256_file(orig_apk_backup)
    print(f"Original Launcher3QuickStep.apk SHA-256: {orig_apk_sha}")

    # 5. Extract classes7.dex
    unpacked_dir = SCRATCH / "unpacked"
    unpacked_dir.mkdir()
    print("Unpacking APK...")
    run_cmd(["unzip", "-q", str(orig_apk), "-d", str(unpacked_dir)])

    classes7 = unpacked_dir / "classes7.dex"
    assert classes7.is_file(), "classes7.dex missing from unpacked APK"
    classes7_data = bytearray(classes7.read_bytes())

    # 6. Verify target bytes before patching
    # Offset 0x199a10: createHardwareBitmap if-nez branch (39000700)
    assert classes7_data[0x199a10:0x199a14] == bytes.fromhex("39000700"), f"Offset 0x199a10 mismatch: {classes7_data[0x199a10:0x199a14].hex()}"
    # Offset 0x199a90: USE_HARDWARE_BITMAP init (1210)
    assert classes7_data[0x199a90:0x199a92] == bytes.fromhex("1210"), f"Offset 0x199a90 mismatch: {classes7_data[0x199a90:0x199a92].hex()}"
    # Offset 0x19b112: DotRenderer.<init> getHeight invoke (6e10150302000a02)
    assert classes7_data[0x19b112:0x19b11a] == bytes.fromhex("6e10150302000a02"), f"Offset 0x19b112 mismatch: {classes7_data[0x19b112:0x19b11a].hex()}"

    # 7. Apply Dual-Layer Patch
    print("Applying Dual-Layer Patch to classes7.dex...")
    # Patch 1: Nop out if-nez in createHardwareBitmap -> always call createSoftwareBitmap
    classes7_data[0x199a10:0x199a14] = bytes.fromhex("00000000")
    # Patch 2: Set USE_HARDWARE_BITMAP = false in <clinit>
    classes7_data[0x199a90:0x199a92] = bytes.fromhex("1200")
    # Patch 3: Replace getHeight() with move v2, v0 (v0 is pill height) + nops
    classes7_data[0x19b112:0x19b11a] = bytes.fromhex("0102000000000000")

    # 8. Recompute DEX signature (SHA-1) and checksum (Adler32)
    print("Recomputing DEX SHA-1 and Adler-32...")
    new_sig = hashlib.sha1(classes7_data[32:]).digest()
    classes7_data[12:32] = new_sig
    new_checksum = zlib.adler32(classes7_data[12:]) & 0xffffffff
    classes7_data[8:12] = struct.pack("<I", new_checksum)

    patched_classes7 = SCRATCH / "classes7_patched.dex"
    patched_classes7.write_bytes(classes7_data)
    classes7.write_bytes(classes7_data)

    # Verify patched dex with dexdump
    out_chk = run_cmd([str(BUILD_TOOLS / "dexdump"), "-c", str(patched_classes7)])
    assert "Checksum verified" in out_chk, "DEX checksum verification failed!"
    print("[PASS] classes7.dex patched and verified clean.")

    # 9. Repack APK
    repacked_apk = SCRATCH / "Launcher3QuickStep_repacked.apk"
    aligned_apk = SCRATCH / "Launcher3QuickStep_aligned.apk"
    signed_apk = SCRATCH / "Launcher3QuickStep_signed.apk"

    shutil.copy2(orig_apk, repacked_apk)
    print("Updating classes7.dex in APK...")
    # Update classes7.dex inside repacked_apk
    cwd = os.getcwd()
    os.chdir(unpacked_dir)
    try:
        run_cmd(["zip", "-u", str(repacked_apk), "classes7.dex"])
        # Remove old signatures
        run_cmd(["zip", "-d", str(repacked_apk), "META-INF/*.SF", "META-INF/*.RSA", "META-INF/*.MF"])
    finally:
        os.chdir(cwd)

    # 10. Zipalign APK
    print("Zipaligning APK (4-byte alignment)...")
    run_cmd([str(BUILD_TOOLS / "zipalign"), "-f", "-p", "4", str(repacked_apk), str(aligned_apk)])

    # 11. Sign APK with v1, v2, and v3 schemes using Android test keystore
    keystore_path = SCRATCH / "test.keystore"
    print("Generating signing key...")
    run_cmd([
        f"{JAVA_HOME}/bin/keytool", "-genkeypair", "-v",
        "-keystore", str(keystore_path),
        "-storepass", "android",
        "-alias", "androiddebugkey",
        "-keypass", "android",
        "-keyalg", "RSA",
        "-keysize", "2048",
        "-validity", "10000",
        "-dname", "CN=Android,O=Android,C=US",
    ], env=env)

    print("Signing APK with apksigner (v1+v2+v3)...")
    shutil.copy2(aligned_apk, signed_apk)
    run_cmd([
        str(BUILD_TOOLS / "apksigner"), "sign",
        "--ks", str(keystore_path),
        "--ks-pass", "pass:android",
        "--key-pass", "pass:android",
        "--v1-signing-enabled", "true",
        "--v2-signing-enabled", "true",
        "--v3-signing-enabled", "true",
        str(signed_apk),
    ], env=env)

    # Verify signature
    out_sig = run_cmd([str(BUILD_TOOLS / "apksigner"), "verify", "-v", str(signed_apk)], env=env)
    assert "Verifies" in out_sig, f"Signature verification failed: {out_sig}"
    print("[PASS] Repacked APK signed and verified with apksigner.")
    signed_apk_sha = sha256_file(signed_apk)
    signed_apk_size = signed_apk.stat().st_size
    print(f"New Launcher3QuickStep.apk: SHA-256 {signed_apk_sha} ({signed_apk_size} bytes)")

    # 12. Clone system.img to guest-inputs/system-fixed.img
    fixed_img = GUEST_INPUTS / "system-fixed.img"
    print(f"Cloning {SYSTEM_IMG} to {fixed_img} (breaking hardlink)...")
    shutil.copyfile(SYSTEM_IMG, fixed_img)

    # 13. Inject signed_apk into system-fixed.img using debugfs
    print("Injecting new APK into system-fixed.img via debugfs...")
    cmds = [
        f"rm system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk",
        f"write {signed_apk} system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk",
        f"set_inode_field system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk mode 0100644",
        f"set_inode_field system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk uid 0",
        f"set_inode_field system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk gid 0",
        f"ea_set system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk security.selinux u:object_r:system_file:s0",
    ]
    for c in cmds:
        run_cmd([str(DEBUGFS), "-w", "-R", c, str(fixed_img)])

    # 14. Run e2fsck on system-fixed.img
    print("Running e2fsck consistency check...")
    # e2fsck returns 0 or 1 (1 = errors corrected cleanly, typical for debugfs edits)
    res = subprocess.run([str(E2FSCK), "-f", "-y", str(fixed_img)], capture_output=True, text=True)
    print(f"e2fsck exit code: {res.returncode}")
    print(res.stdout)
    if res.returncode not in (0, 1):
        raise RuntimeError(f"e2fsck failed with exit code {res.returncode}: {res.stderr}")

    # 15. Atomically replace system.img
    print(f"Atomically replacing {SYSTEM_IMG}...")
    os.replace(fixed_img, SYSTEM_IMG)

    new_img_sha = sha256_file(SYSTEM_IMG)
    new_img_size = SYSTEM_IMG.stat().st_size
    print(f"New system.img SHA-256: {new_img_sha} ({new_img_size} bytes)")
    assert new_img_size == EXPECTED_ORIGINAL_SIZE, f"Image size changed: {new_img_size} != {EXPECTED_ORIGINAL_SIZE}"
    assert new_img_sha != orig_sha, "system.img SHA did not change!"

    # 16. Extract and verify preflight proof from newly repacked system.img
    print("Extracting proof APK from new system.img...")
    proof_apk = SCRATCH / "proof_extracted.apk"
    run_cmd([str(DEBUGFS), "-R", f"dump system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk {proof_apk}", str(SYSTEM_IMG)])
    assert proof_apk.is_file(), "Failed to dump proof APK from new system.img"
    proof_apk_sha = sha256_file(proof_apk)
    assert proof_apk_sha == signed_apk_sha, f"Proof APK SHA mismatch: {proof_apk_sha} != {signed_apk_sha}"

    proof_classes7 = SCRATCH / "proof_classes7.dex"
    classes7_bytes = subprocess.run(["unzip", "-p", str(proof_apk), "classes7.dex"], capture_output=True).stdout
    proof_classes7.write_bytes(classes7_bytes)

    # Prove patch is in classes7.dex
    assert classes7_bytes[0x199a10:0x199a14] == bytes.fromhex("00000000"), "Patch 1 missing in image!"
    assert classes7_bytes[0x199a90:0x199a92] == bytes.fromhex("1200"), "Patch 2 missing in image!"
    assert classes7_bytes[0x19b112:0x19b11a] == bytes.fromhex("0102000000000000"), "Patch 3 missing in image!"

    out_chk2 = run_cmd([str(BUILD_TOOLS / "dexdump"), "-c", str(proof_classes7)])
    assert "Checksum verified" in out_chk2, "Proof DEX checksum failed!"
    print("[PASS] Proof DEX checksum verified!")
    print("[PASS] Preflight proof 100% verified: Dual-Layer fix is physically inside system.img!")

    # Record fix artifact record
    record = {
        "track": "T702",
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "original_system_img_sha256": orig_sha,
        "original_system_img_bytes": orig_size,
        "new_system_img_sha256": new_img_sha,
        "new_system_img_bytes": new_img_size,
        "original_apk_sha256": orig_apk_sha,
        "new_apk_sha256": signed_apk_sha,
        "new_apk_bytes": signed_apk_size,
        "patched_dex": "classes7.dex",
        "patches": [
            {
                "target": "BitmapRenderer.createHardwareBitmap",
                "offset": "0x199a10",
                "old_hex": "39000700",
                "new_hex": "00000000",
                "description": "Nop out if-nez v0, 0009 branch to hardware Picture path; forces fallthrough to createSoftwareBitmap"
            },
            {
                "target": "BitmapRenderer.<clinit>",
                "offset": "0x199a90",
                "old_hex": "1210",
                "new_hex": "1200",
                "description": "Set USE_HARDWARE_BITMAP = false (const/4 v0, #0)"
            },
            {
                "target": "DotRenderer.<init>",
                "offset": "0x19b112",
                "old_hex": "6e10150302000a02",
                "new_hex": "0102000000000000",
                "description": "Replace invoke-virtual getHeight + move-result with move v2, v0 (pill height) + 3 nops"
            }
        ],
        "preflight_proof": {
            "verified_in_image": True,
            "apk_path_in_guest": "/system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk",
            "file_mode": "0100644",
            "uid": 0,
            "gid": 0,
            "selinux_context": "u:object_r:system_file:s0"
        }
    }
    (WORKTREE / "evidence" / "t702_fix_record.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"Fix record written to evidence/t702_fix_record.json")
    print(f"Elapsed time: {time.monotonic() - t0:.1f}s")


if __name__ == "__main__":
    main()
