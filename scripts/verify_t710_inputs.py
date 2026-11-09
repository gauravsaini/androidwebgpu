import hashlib
import os
import sys

INPUTS = {
    "findings-t709.md (in T709 worktree)": (
        "/Users/Shared/wt-track709-logcat-systemserver-cure/findings-t709.md",
        "be5c9e24fc0adf78391afc2a81841d83181f7e688bff5b7508987fc925cfa90b",
        13379
    ),
    "T709 flight-01 UART": (
        "/Users/Shared/wt-track709-logcat-systemserver-cure/flights/flight-01/uart.log",
        "97194592e2825aa7b7536f2edc73fd089bdd4fcfbcaee7fef52cd1967719abff",
        None
    ),
    "T709 flight-ab UART": (
        "/Users/Shared/wt-track709-logcat-systemserver-cure/flights/flight-ab/uart.log",
        "7da313bcbc5c3b491af1e29768cca7e22361c8f3b2d53d0061573d1999cde08f",
        None
    ),
    "T709 diagnostic smoke UART": (
        "/Users/Shared/wt-track709-logcat-systemserver-cure/flights/smoke-attempt-7/uart.log",
        "f7338aacbd35dcf9ed7745fa6da5405be3786b72afde076dc088d6fcc1e5335f",
        None
    ),
    "T709 validation smoke UART": (
        "/Users/Shared/wt-track709-logcat-systemserver-cure/flights/smoke/uart.log",
        "c480c5951952b648010d2be2b0e0c5cd881b8b794e4da781abe337b6150ae9b8",
        None
    ),
    "T709 fix initramfs": (
        "guest-inputs/t709-systemserver-fb0-fix.cpio.gz",
        "f164d33b8b48a8541de4c6e163be654647fca39a1a8a70b421b05a7114f562db",
        53465900
    ),
    "T709 modified.dtb": (
        "guest-inputs/t709-modified.dtb",
        "718691ecb661046785bae0b26a2d5cbd2ae90518ef591a810f7484ac03b9263e",
        2049
    ),
    "T708 vendor.img": (
        "guest-inputs/t708-vendor.img",
        "8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092",
        134217728
    ),
    "T708 rebuilt Composer ELF": (
        "guest-inputs/t708-android.hardware.graphics.composer@2.1-service",
        "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401",
        19942712
    ),
    "T706 patched system.img": (
        "guest-inputs/system.img",
        "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601",
        2059816960
    ),
    "T705 rebuilt Image": (
        "guest-inputs/t705-Image",
        "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859",
        20126208
    ),
    "extracted_kernel.config": (
        "guest-inputs/extracted_kernel.config",
        "d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8",
        133733
    ),
    "canonical Image": (
        "guest-inputs/canonical-Image",
        "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
        23073280
    )
}

def sha256sum(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    failed = False
    for name, (path, expected_sha, expected_size) in INPUTS.items():
        if not os.path.exists(path):
            print(f"FAILED: {name} not found at {path}")
            failed = True
            continue
        size = os.path.getsize(path)
        if expected_size is not None and size != expected_size:
            print(f"FAILED: {name} size mismatch: got {size}, expected {expected_size}")
            failed = True
            continue
        sha = sha256sum(path)
        if sha != expected_sha:
            print(f"FAILED: {name} SHA mismatch: got {sha}, expected {expected_sha}")
            failed = True
            continue
        print(f"PASSED: {name} ({size} B) -> {sha}")

    if failed:
        sys.exit(1)
    print("ALL_INPUTS_VERIFIED_OK")

if __name__ == "__main__":
    main()
