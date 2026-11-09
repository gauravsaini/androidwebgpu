#!/usr/bin/env python3
import hashlib
import os
import subprocess
import sys

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest(), os.path.getsize(path)

def verify():
    # 1. Verify Composer ELF
    elf_path = "guest-inputs/t708-android.hardware.graphics.composer@2.1-service"
    elf_sha, elf_sz = sha256_file(elf_path)
    assert elf_sha == "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401", f"ELF SHA mismatch: {elf_sha}"
    assert elf_sz == 19942712, f"ELF size mismatch: {elf_sz}"
    with open(elf_path, "rb") as f:
        content = f.read()
    assert b"ComposerImpl::createClient: client reconnect requested, replacing previous client" in content, "Reconnect string missing"
    assert b"ComposerImpl::createClient: created client %p (reconnect supported)" in content, "Reconnect created string missing"

    # 2. Verify vendor.img
    ven_path = "guest-inputs/t708-vendor.img"
    ven_sha, ven_sz = sha256_file(ven_path)
    assert ven_sha == "8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092", f"Vendor SHA mismatch: {ven_sha}"
    assert ven_sz == 134217728, f"Vendor size mismatch: {ven_sz}"

    # 3. Verify fix initramfs
    cpio_path = "guest-inputs/t708-systemserver-fb0-fix.cpio.gz"
    cpio_sha, cpio_sz = sha256_file(cpio_path)
    assert cpio_sha == "49d395d3e195cdbe2686c61a3b53b32a48ed16e81b592a67e71f1b8236fe1ad9", f"CPIO SHA mismatch: {cpio_sha}"
    assert cpio_sz == 53172530, f"CPIO size mismatch: {cpio_sz}"

    # 4. Verify modified DTB
    dtb_path = "guest-inputs/t708-modified.dtb"
    dtb_sha, dtb_sz = sha256_file(dtb_path)
    assert dtb_sha == "9a85c074df4a6c0a84fec65f793cb1b63a73c94ba135d247ea5bf936539fb85c", f"DTB SHA mismatch: {dtb_sha}"
    assert dtb_sz == 2049, f"DTB size mismatch: {dtb_sz}"

    # 5. Verify Route B Image and system.img
    img_sha, img_sz = sha256_file("guest-inputs/t705-Image")
    assert img_sha == "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859"
    sys_sha, sys_sz = sha256_file("guest-inputs/system.img")
    assert sys_sha == "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601"

    print("PREFLIGHT_PROOFS_OK")

if __name__ == "__main__":
    verify()
