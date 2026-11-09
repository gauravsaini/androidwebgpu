#!/usr/bin/env python3
import hashlib
import os
import sys

def verify():
    files = {
        'findings-t707.md': ('findings-t707.md', '48ed44b135f66749ed9f3aa2f4a4fdb1c9b5555f7aa6e375a2f0a9c7af6c58ff', 19925),
        'T707 flight-01 UART': ('/Users/Shared/wt-track707-composer-hotplug-dispatch/flights/flight-01/uart.log', '13699cbebe1149b246409a4bd56e6228ebf4eb59983bc25ea9e0b06cfc72f9c9', 5571703),
        'T707 flight-ab UART': ('/Users/Shared/wt-track707-composer-hotplug-dispatch/flights/flight-ab/uart.log', '51431be7b8f14cea1cde193171a102bda6ff5378c62fa6f4deab64ad76bb57bb', 46938),
        't707-systemserver-fb0-fix.cpio.gz': ('guest-inputs/t707-systemserver-fb0-fix.cpio.gz', '201bcd058b65533ec7ad904bc79ee5b062c51855046b0e787cc81835508f0f94', 53172105),
        't707-modified.dtb': ('guest-inputs/t707-modified.dtb', '3d8341f4ea6c49f4f4d4eeb8593c3270eedbcc27ae948b21d6e521e200994ba9', 2049),
        't707-vendor.img': ('guest-inputs/t707-vendor.img', '983131c300997dd7041e588d1c2fe00ea0d27dbffb3862eb8b37b8b1324b7ad2', 134217728),
        't707 Composer ELF': ('guest-inputs/t707-android.hardware.graphics.composer@2.1-service', '781dac3bae61652d424a5f6622ac72d0801fcdcdeab0565445f28668b0eebed5', 19943144),
        'system.img': ('guest-inputs/system.img', '2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601', 2059816960),
        't705-Image': ('guest-inputs/t705-Image', '53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859', 20126208),
        'extracted_kernel.config': ('guest-inputs/extracted_kernel.config', 'd840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8', 133733),
        'canonical-Image': ('guest-inputs/canonical-Image', '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04', 23073280),
    }

    for name, (path, exp_sha, exp_sz) in files.items():
        if not os.path.exists(path):
            print(f"MISSING: {name} at {path}")
            sys.exit(1)
        sz = os.path.getsize(path)
        h = hashlib.sha256()
        with open(path, 'rb') as f:
            while chunk := f.read(1024 * 1024):
                h.update(chunk)
        act_sha = h.hexdigest()
        if sz != exp_sz:
            print(f"SIZE MISMATCH: {name} {sz} vs {exp_sz}")
            sys.exit(1)
        if act_sha != exp_sha:
            print(f"SHA MISMATCH: {name} {act_sha} vs {exp_sha}")
            sys.exit(1)

    print("INPUTS_VERIFIED_OK")

if __name__ == "__main__":
    verify()
