#!/bin/bash
set -euo pipefail

echo "=== Verifying Input SHAs ==="

check_file() {
    local path="$1"
    local expected="$2"
    local size="$3"
    
    if [ ! -f "$path" ]; then
        echo "FAIL: File missing: $path"
        exit 1
    fi
    
    local actual_sha
    actual_sha=$(shasum -a 256 "$path" | awk '{print $1}')
    local actual_size
    actual_size=$(wc -c < "$path" | tr -d ' ')
    
    if [ "$actual_sha" != "$expected" ]; then
        echo "FAIL: SHA mismatch for $path"
        echo "  Expected: $expected"
        echo "  Actual:   $actual_sha"
        exit 1
    fi
    
    if [ -n "$size" ] && [ "$actual_size" -ne "$size" ]; then
        echo "FAIL: Size mismatch for $path"
        echo "  Expected: $size"
        echo "  Actual:   $actual_size"
        exit 1
    fi
    
    echo "OK: $path (SHA: $actual_sha, Size: $actual_size)"
}

check_lines() {
    local path="$1"
    local expected_lines="$2"
    local actual_lines
    actual_lines=$(wc -l < "$path" | tr -d ' ')
    if [ "$actual_lines" -ne "$expected_lines" ]; then
        echo "FAIL: Line count mismatch for $path"
        echo "  Expected: $expected_lines"
        echo "  Actual:   $actual_lines"
        exit 1
    fi
    echo "OK lines: $path has $actual_lines lines"
}

# 1. findings-t712.md
check_file "/Users/Shared/wt-track712-hals-conf-fix/findings-t712.md" "b72e4655d6fbbd70aabe94a8d8696b928a539e0c3cbe3c11b243880be9c74e85" 16086

# 2. death-note-t712.md
check_file "/Users/Shared/wt-track712-hals-conf-fix/death-note-t712.md" "1d3038bec5238bde7ab45adacf5223da71220d60ccf792f309a77e4b1f902034" 8347

# 3. T712 flight-01 UART (51,719 lines)
check_file "/Users/Shared/wt-track712-hals-conf-fix/flights/flight-01/uart.log" "140f36f69eab75f48a3391e6f6beb37fb14f182163adb342e7902308b7c31125" ""
check_lines "/Users/Shared/wt-track712-hals-conf-fix/flights/flight-01/uart.log" 51719

# 4. T712 flight-ab UART (573 lines)
check_file "/Users/Shared/wt-track712-hals-conf-fix/flights/flight-ab/uart.log" "fc7f2a2cdae4e86d046daf90053275b613be7373ac3069fb6b63191ad327a45e" ""
check_lines "/Users/Shared/wt-track712-hals-conf-fix/flights/flight-ab/uart.log" 573

# 5. T712 smoke UART (10,369 lines)
check_file "/Users/Shared/wt-track712-hals-conf-fix/flights/smoke/uart.log" "51b074214542e40df338745b0289ef2a7a8ac07c90a62f874236f87770f94964" ""
check_lines "/Users/Shared/wt-track712-hals-conf-fix/flights/smoke/uart.log" 10369

# 6. T712 fix initramfs
check_file "guest-inputs/t712-fix.cpio.gz" "297d0a447cd56cbc8b985b11956e95563d5965640abae32985ae1ee26f559155" 53502698

# 7. T712 modified.dtb
check_file "guest-inputs/t712-modified.dtb" "58f5acb3b250f64fb7426cb090cce1728a153281dfdb40d8a9b6863a949e41d8" 2049

# 8. T712 vendor.img
check_file "guest-inputs/t712-vendor.img" "095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf" 134217728

# 9. T708 rebuilt Composer ELF
check_file "guest-inputs/t708-android.hardware.graphics.composer@2.1-service" "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401" 19942712

# 10. T706 patched system.img
check_file "guest-inputs/system.img" "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601" 2059816960

# 11. T705 rebuilt Image
check_file "guest-inputs/t705-Image" "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859" 20126208

# 12. extracted_kernel.config
check_file "guest-inputs/extracted_kernel.config" "d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8" 133733

echo "=== ALL INPUTS VERIFIED SUCCESSFULLY ==="
