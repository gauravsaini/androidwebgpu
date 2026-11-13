#!/usr/bin/env python3
import hashlib, os, sys

files = {
    'findings-t710.md': ('/Users/Shared/wt-track710-sensorservice-wall-cure/findings-t710.md', 'e91f4b44f696dfc3e378551f8da76f89dd30af47451a5ba75e81bb63eefcd21e', 13817),
    't710-flight-01-uart': ('/Users/Shared/wt-track710-sensorservice-wall-cure/flights/flight-01/uart.log', 'e02f22e7007f66cd9f9cdadd5e9e47e2bef621256531742c6e224819f15ca9b9', 7937639),
    't710-flight-ab-uart': ('/Users/Shared/wt-track710-sensorservice-wall-cure/flights/flight-ab/uart.log', '923025b8e180ac74bf12c298501f31cb109e6918bc83b1db6f1b203652a137c5', 47791),
    't710-smoke-uart': ('/Users/Shared/wt-track710-sensorservice-wall-cure/flights/smoke/uart.log', '3143c2fb7806df69384426490901f1250058fb75b817eefb71d53db0d342ecfc', 1105935),
    't710-fix.cpio.gz': ('guest-inputs/t710-fix.cpio.gz', 'e839cb18cd60068d804bb0a93fc69b6c31ebe5ebbcd93cdd0bf30bb088066665', 53501694),
    't710-modified.dtb': ('guest-inputs/t710-modified.dtb', '1ef7939b7058305b857da4bd1f449493552bfb12c0a8e8d8d0cb808dfb9c2627', 2049),
    't708-vendor.img': ('guest-inputs/t708-vendor.img', '8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092', 134217728),
    't708-composer': ('guest-inputs/t708-android.hardware.graphics.composer@2.1-service', '8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401', 19942712),
    'system.img': ('guest-inputs/system.img', '2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601', 2059816960),
    't705-Image': ('guest-inputs/t705-Image', '53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859', 20126208),
    'extracted_kernel.config': ('guest-inputs/extracted_kernel.config', 'd840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8', 133733),
    'canonical-Image': ('guest-inputs/canonical-Image', '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04', 23073280)
}

all_ok = True
for name, (path, expected_sha, expected_sz) in files.items():
    if not os.path.exists(path):
        print(f'MISSING: {name} at {path}')
        all_ok = False
        continue
    sz = os.path.getsize(path)
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        while chunk := f.read(1048576):
            h.update(chunk)
    actual_sha = h.hexdigest()
    sha_match = actual_sha == expected_sha or actual_sha.startswith(expected_sha)
    sz_match = (expected_sz is None) or (sz == expected_sz)
    if not (sha_match and sz_match):
        print(f'FAIL: {name} sz={sz} (exp {expected_sz}) sha={actual_sha} (exp {expected_sha})')
        all_ok = False
    else:
        print(f'PASS: {name}')

if all_ok:
    print('INPUTS_VERIFIED_OK')
    sys.exit(0)
else:
    print('INPUTS_VERIFICATION_FAILED')
    sys.exit(1)
