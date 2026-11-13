#!/usr/bin/env python3
import hashlib, os, sys, subprocess

files = {
    'findings-t711.md': ('/Users/Shared/wt-track711-selinux-domain-fix/findings-t711.md', 'bc18f0b926f3e2e38c373fd39dceb607e1877e234774abeb80b3aaf23b2be3c9', 15146),
    't711-flight-01-uart': ('/Users/Shared/wt-track711-selinux-domain-fix/flights/flight-01/uart.log', '6fc97ccc88c4991928934f38d94e0a5831044e79629e1a395327ba0650a5b4ad', None),
    't711-flight-ab-uart': ('/Users/Shared/wt-track711-selinux-domain-fix/flights/flight-ab/uart.log', '29154ab0e1c0bc0ee270e4d387eb10b7918dfb0b8298f16d6625b0ae590440be', None),
    't711-smoke-uart': ('/Users/Shared/wt-track711-selinux-domain-fix/flights/smoke/uart.log', 'd1bfd4f681a7ca177a38d97feb46f956b16833de66cba438a1bffcb611d6d755', None),
    't711-fix.cpio.gz': ('guest-inputs/t711-fix.cpio.gz', '025cac1c987aaf57bfc3356062e3245de49c550b549a24cc73ff2754f040321d', 53502663),
    't711-modified.dtb': ('guest-inputs/t711-modified.dtb', '2d6d0c11ef22cace73e0f95b5eb8552727c57c614cc07d0fc16ece4613eeb77b', 2049),
    't708-vendor.img': ('guest-inputs/t708-vendor.img', '8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092', 134217728),
    't708-composer': ('guest-inputs/t708-android.hardware.graphics.composer@2.1-service', '8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401', 19942712),
    'system.img': ('guest-inputs/system.img', '2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601', 2059816960),
    't705-Image': ('guest-inputs/t705-Image', '53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859', 20126208),
    'extracted_kernel.config': ('guest-inputs/extracted_kernel.config', 'd840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8', 133733),
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
        print(f'PASS: {name} (sha={actual_sha[:16]}..., sz={sz})')

# Verify lines for UARTs
line_checks = {
    't711-flight-01-uart': 51931,
    't711-flight-ab-uart': 574,
    't711-smoke-uart': 11051,
}
for name, exp_lines in line_checks.items():
    p = files[name][0]
    with open(p, 'rb') as f:
        lc = sum(1 for _ in f)
    if lc != exp_lines:
        print(f'FAIL LINE COUNT: {name} lines={lc} (exp {exp_lines})')
        all_ok = False
    else:
        print(f'PASS LINE COUNT: {name} ({lc} lines)')

# Verify box vault files and access
box_cmd = "sha256sum /mnt/sdb1/kernel-critical/lane-images/t711-selinux-domain-fix.cpio.gz /mnt/sdb1/kernel-critical/lane-images/t706-system.img"
res = subprocess.run([os.path.expanduser('~/workspace/bin/boxssh.sh'), box_cmd], capture_output=True, text=True)
if res.returncode != 0:
    print(f'FAIL: boxssh failed: {res.stderr}')
    all_ok = False
else:
    box_out = res.stdout
    print(f'BOX SHA OUTPUT:\n{box_out}')
    if '025cac1c987aaf57bfc3356062e3245de49c550b549a24cc73ff2754f040321d' not in box_out:
        print('FAIL: box copy of t711-selinux-domain-fix.cpio.gz hash mismatch')
        all_ok = False
    else:
        print('PASS: box copy of t711-selinux-domain-fix.cpio.gz verified')
    if '2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601' not in box_out:
        print('FAIL: box copy of t706-system.img hash mismatch')
        all_ok = False
    else:
        print('PASS: box copy of t706-system.img verified')

if all_ok:
    print('ALL_INPUTS_VERIFIED_OK')
    sys.exit(0)
else:
    print('INPUTS_VERIFICATION_FAILED')
    sys.exit(1)
