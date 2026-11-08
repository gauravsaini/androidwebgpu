#!/usr/bin/env python3
import hashlib
import os
import sys

GUEST_INPUTS_DIR = os.path.abspath("guest-inputs")
os.makedirs(GUEST_INPUTS_DIR, exist_ok=True)

EXPECTED = [
    {
        "name": "findings-t705.md",
        "source": "findings-t705.md",
        "dest": os.path.join(GUEST_INPUTS_DIR, "findings-t705.md"),
        "sha256": "d99361c71a23a41bff7f059794fc1ba6b06256dd61453ab61d2e6aacdd0313da",
        "size": 17345
    },
    {
        "name": "findings-t704.md",
        "source": "findings-t704.md",
        "dest": os.path.join(GUEST_INPUTS_DIR, "findings-t704.md"),
        "sha256": "c572f0dfe9f0dd02e164952d4402ddb27cf20a4c1fb7a4b24dd5fec6f3fb6d34",
        "size": 19065
    },
    {
        "name": "findings-t703.md",
        "source": "findings-t703.md",
        "dest": os.path.join(GUEST_INPUTS_DIR, "findings-t703.md"),
        "sha256": "6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583",
        "size": 22137
    },
    {
        "name": "findings-t702.md",
        "source": "findings-t702.md",
        "dest": os.path.join(GUEST_INPUTS_DIR, "findings-t702.md"),
        "sha256": "708128c022b3f81c092b8743af075086f492568194a3f5afb66a944134aadb34",
        "size": 21909
    },
    {
        "name": "t705-Image",
        "source": "/Users/Shared/wt-track705-virtio-blk-kernel-rebuild/guest-inputs/t705-Image",
        "dest": os.path.join(GUEST_INPUTS_DIR, "t705-Image"),
        "sha256": "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859",
        "size": 20126208
    },
    {
        "name": "t705-systemserver-fb0-fix.cpio.gz",
        "source": "/Users/Shared/wt-track705-virtio-blk-kernel-rebuild/guest-inputs/t705-systemserver-fb0-fix.cpio.gz",
        "dest": os.path.join(GUEST_INPUTS_DIR, "t705-systemserver-fb0-fix.cpio.gz"),
        "sha256": "583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965",
        "size": 53169644
    },
    {
        "name": "t705-modified.dtb",
        "source": "/Users/Shared/wt-track705-virtio-blk-kernel-rebuild/guest-inputs/t705-modified.dtb",
        "dest": os.path.join(GUEST_INPUTS_DIR, "t705-modified.dtb"),
        "sha256": "0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01",
        "size": 2049
    },
    {
        "name": "canonical-Image",
        "source": "/Users/Shared/wt-track705-virtio-blk-kernel-rebuild/guest-inputs/canonical-Image",
        "dest": os.path.join(GUEST_INPUTS_DIR, "canonical-Image"),
        "sha256": "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
        "size": 23073280
    },
    {
        "name": "clean-base-system.img",
        "source": "/Users/Shared/wt-track702-launcher3-crashloop-cure/evidence/originals/system.img",
        "dest": os.path.join(GUEST_INPUTS_DIR, "clean-base-system.img"),
        "sha256": "98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d",
        "size": 2059816960
    },
    {
        "name": "composer",
        "source": "/Users/Shared/wt-track705-virtio-blk-kernel-rebuild/guest-inputs/android.hardware.graphics.composer@2.1-service",
        "dest": os.path.join(GUEST_INPUTS_DIR, "android.hardware.graphics.composer@2.1-service"),
        "sha256": "ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132",
        "size": 19941040
    },
    {
        "name": "extracted_kernel.config",
        "source": "/Users/Shared/wt-track697-framecommit-routeb-agyx/guest-inputs/extracted_kernel.config",
        "dest": os.path.join(GUEST_INPUTS_DIR, "extracted_kernel.config"),
        "sha256": "d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8",
        "size": 133733
    }
]

def hash_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1024*1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    print("=== Step 1: Input Verification ===")
    all_ok = True
    for item in EXPECTED:
        name = item["name"]
        src = item["source"]
        dst = item["dest"]
        
        if not os.path.exists(dst):
            if os.path.exists(src):
                try:
                    os.link(src, dst)
                    print(f"Hardlinked {src} -> {dst}")
                except Exception as e:
                    print(f"Could not hardlink {src} -> {dst}: {e}")
                    # Try copying only if not 2GB
                    if item["size"] < 100 * 1024 * 1024:
                        import shutil
                        shutil.copy2(src, dst)
                    else:
                        print(f"FATAL: cannot link 2GB file: {e}")
                        sys.exit(1)
            else:
                print(f"ERROR: Missing source {src} for {name}")
                all_ok = False
                continue

        actual_size = os.path.getsize(dst)
        if actual_size != item["size"]:
            print(f"ERROR {name}: size mismatch (expected {item['size']}, got {actual_size})")
            all_ok = False
            continue

        actual_sha = hash_file(dst)
        if actual_sha != item["sha256"]:
            print(f"ERROR {name}: SHA mismatch (expected {item['sha256']}, got {actual_sha})")
            all_ok = False
            continue

        print(f"PASS: {name} (size {actual_size}, SHA {actual_sha[:12]}...)")

    # Prove block 145414 on clean-base-system.img
    clean_img = os.path.join(GUEST_INPUTS_DIR, "clean-base-system.img")
    with open(clean_img, "rb") as f:
        f.seek(145414 * 4096)
        block = f.read(4096)
    block_sha = hashlib.sha256(block).hexdigest()
    expected_block_sha = "4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea"
    if block_sha != expected_block_sha:
        print(f"ERROR: block 145414 SHA mismatch: {block_sha} != {expected_block_sha}")
        all_ok = False
    else:
        print(f"PASS: block 145414 clean fill proven (SHA {block_sha[:12]}...)")

    if all_ok:
        print("STEP1_INPUTS_VERIFIED_OK")
        sys.exit(0)
    else:
        print("STEP1_INPUTS_VERIFIED_FAILED")
        sys.exit(1)

if __name__ == "__main__":
    main()
