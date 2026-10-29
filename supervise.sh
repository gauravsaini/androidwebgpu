#!/bin/bash
set -euo pipefail

WORKTREE="/Users/Shared/wt-track643-rust-init-marker-silence"
FLIGHT_DIR="/Users/Shared/t643-flight"
GUEST_INPUTS="/Users/Shared/t643-guest-inputs"
HEARTBEAT="${WORKTREE}/heartbeat.json"

cd "${WORKTREE}"

# Disk hygiene check
AVAIL_KB=$(df -k /System/Volumes/Data | tail -1 | awk '{print $4}')
if [ "$AVAIL_KB" -lt 2097152 ]; then
  echo "Disk space below 2GiB ($AVAIL_KB KB). Throttling abort."
  exit 1
fi

export T632_DEBUG_FETCH=1

"${WORKTREE}/target/release/run_kernel" \
  --kernel "${GUEST_INPUTS}/Image" \
  --dtb "${GUEST_INPUTS}/minimal-virt-fixed.dtb" \
  --initrd "${GUEST_INPUTS}/initramfs.cpio" \
  --max-steps 300000000 \
  --console-output "${FLIGHT_DIR}/guest-uart.log" \
  > "${FLIGHT_DIR}/runner.log" 2>&1 &

RPID=$!
echo "runner pid: $RPID" > "${FLIGHT_DIR}/supervise.log"

while kill -0 "$RPID" 2>/dev/null; do
  printf '{"ts":"%s","stage":"boot","note":"run_kernel alive (pid %d), see t643-flight/runner.log"}\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$RPID" > "${HEARTBEAT}"
  sleep 60
done

wait "$RPID" || RUNNER_EXIT=$?
RUNNER_EXIT=${RUNNER_EXIT:-0}

printf '{"ts":"%s","stage":"boot-done","note":"runner exited %d, see t643-flight/runner.log"}\n' \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$RUNNER_EXIT" > "${HEARTBEAT}"

if [ -f "${FLIGHT_DIR}/guest-uart.log" ]; then
  cp "${FLIGHT_DIR}/guest-uart.log" "${FLIGHT_DIR}/guest-uart.log.bak"
  gzip -f "${FLIGHT_DIR}/guest-uart.log" 2>/dev/null || true
  mv "${FLIGHT_DIR}/guest-uart.log.bak" "${FLIGHT_DIR}/guest-uart.log"
fi

echo "supervisor done, runner exit: $RUNNER_EXIT" >> "${FLIGHT_DIR}/supervise.log"
