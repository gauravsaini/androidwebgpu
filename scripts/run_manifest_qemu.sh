#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$repo_dir/MANIFEST.json"
serial_log="$repo_dir/qemu-manifest-boot.log"
launch_log="$repo_dir/qemu-manifest-launch.log"
rm -f "$launch_log"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'required command missing: %s\n' "$1" >&2
    exit 2
  }
}

need jq
need shasum

qemu="$(jq -er '.qemu_binary' "$manifest")"
kernel="$(jq -er '.kernel_path' "$manifest")"
dtb="$(jq -er '.dtb_path' "$manifest")"
initramfs="$(jq -er '.initramfs_path' "$manifest")"
cpu="$(jq -er '.cpu' "$manifest")"
ram_mb="$(jq -er '.ram_mb' "$manifest")"
bootargs="$(jq -er '.bootargs' "$manifest")"
machine="$(jq -er '.qemu_machine' "$manifest")"
gpu_device="$(jq -er '.qemu_gpu_device' "$manifest")"
accel="$(jq -er '.qemu_accel' "$manifest")"
expected_qemu_version="$(jq -er '.qemu_version' "$manifest")"

actual_qemu_version="$("$qemu" --version | head -n 1)"
printf '%s\n' "$actual_qemu_version" > "$launch_log"
if [[ "$actual_qemu_version" != "$expected_qemu_version" ]]; then
  printf 'QEMU version mismatch: expected %s, got %s\n' \
    "$expected_qemu_version" "$actual_qemu_version" >&2
  exit 2
fi

check_hash() {
  local path="$1"
  local expected="$2"
  local actual
  actual="$(shasum -a 256 "$path" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    printf 'SHA-256 mismatch for %s: expected %s, got %s\n' \
      "$path" "$expected" "$actual" >&2
    exit 2
  fi
}

check_hash "$kernel" "$(jq -er '.kernel_sha256' "$manifest")"
check_hash "$dtb" "$(jq -er '.dtb_sha256' "$manifest")"
check_hash "$initramfs" "$(jq -er '.initramfs_sha256' "$manifest")"

rm -f "$serial_log"
"$qemu" \
  -machine "$machine" \
  -cpu "$cpu" \
  -m "${ram_mb}M" \
  -accel "$accel" \
  -kernel "$kernel" \
  -dtb "$dtb" \
  -initrd "$initramfs" \
  -append "$bootargs" \
  -device "$gpu_device" \
  -display none \
  -serial "file:$serial_log" \
  -monitor none \
  -no-reboot \
  >> "$launch_log" 2>&1 &
qemu_pid=$!

stop_qemu() {
  if kill -0 "$qemu_pid" 2>/dev/null; then
    kill -TERM "$qemu_pid" 2>/dev/null || true
    wait "$qemu_pid" 2>/dev/null || true
  fi
}
trap stop_qemu EXIT

for ((second = 0; second < 600; second++)); do
  if [[ -s "$serial_log" ]] && grep -Fq 'Run /init as init process' "$serial_log"; then
    kill -TERM "$qemu_pid" 2>/dev/null || true
    wait "$qemu_pid" 2>/dev/null || true
    trap - EXIT
    printf 'Guest UART reached Run /init as init process after %s seconds.\n' "$second"
    exit 0
  fi
  if ! kill -0 "$qemu_pid" 2>/dev/null; then
    set +e
    wait "$qemu_pid"
    status=$?
    set -e
    printf 'QEMU exited before the guest init marker (status %s).\n' "$status" >&2
    exit 1
  fi
  sleep 1
done

printf 'Timeout waiting for the guest init marker.\n' >&2
exit 1
