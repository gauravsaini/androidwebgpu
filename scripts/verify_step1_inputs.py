#!/usr/bin/env python3
import hashlib, os, sys

inputs = [
    ('findings-t706.md', '/Users/Shared/wt-track706-clean-gsi-hwc-hotplug/findings-t706.md', 'cb6ebb2d1f07e1c6d733dec183bcd508194f0481985671a8809a9d001804e4e8', 17266),
    ('findings-t705.md', '/Users/Shared/wt-track705-virtio-blk-kernel-rebuild/findings-t705.md', 'd99361c71a23a41bff7f059794fc1ba6b06256dd61453ab61d2e6aacdd0313da', 17345),
    ('T706 flight-01 UART', '/Users/Shared/wt-track706-clean-gsi-hwc-hotplug/flights/flight-01/uart.log', '3e8121d08c8e1f2176eb3d1e5274137f7019220baefb264c2730abc0367ca1d0', 5735995),
    ('T706 flight-ab UART', '/Users/Shared/wt-track706-clean-gsi-hwc-hotplug/flights/flight-ab/uart.log', '6088d1dcde55d75e0ad70c077fd7bcd92ce62157309fc3127c583b83564b6dcf', 46858),
    ('T706 patched system.img', 'guest-inputs/system.img', '2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601', 2059816960),
    ('T705 rebuilt Image', 'guest-inputs/t705-Image', '53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859', 20126208),
    ('T705 fix initramfs', 'guest-inputs/t705-systemserver-fb0-fix.cpio.gz', '583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965', 53169644),
    ('T705 modified.dtb', 'guest-inputs/t705-modified.dtb', '0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01', 2049),
    ('Composer ELF', 'guest-inputs/android.hardware.graphics.composer@2.1-service', 'ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132', 19941040),
    ('extracted_kernel.config', 'guest-inputs/extracted_kernel.config', 'd840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8', 133733),
]

all_ok = True
for name, path, expected_sha, expected_size in inputs:
    if not os.path.exists(path):
        print(f'MISSING: {name} at {path}')
        all_ok = False
        continue
    size = os.path.getsize(path)
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        while chunk := f.read(1024*1024):
            h.update(chunk)
    actual_sha = h.hexdigest()
    sha_match = (actual_sha == expected_sha)
    size_match = (size == expected_size)
    print(f'{name}: size={size} (exp {expected_size}, match={size_match}) sha={actual_sha} (exp {expected_sha}, match={sha_match})')
    if not (sha_match and size_match):
        all_ok = False

if all_ok:
    print('ALL_INPUTS_VERIFIED_PASS')
else:
    print('INPUTS_VERIFIED_FAIL')
    sys.exit(1)
