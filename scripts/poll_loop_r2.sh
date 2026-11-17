#!/bin/bash
# T728 recovery-2 Step 4 poll loop: polls every 5 min until wedge 10:37:08 AEDT 18 Nov 2026.
# One-shot df_poll_t728.sh per iteration; verifies each append lands; stops
# immediately on GATE_OPEN (exit 0) or at the wedge (exit 10, WAIT-DISK).
# Detached on the Mac so sandbox runtime restarts cannot kill it (recovery-1
# lost two loops to restarts). Wedge stop is a lexicographic time comparison.
WT=/Users/Shared/wt-track728-observable-payload
WEDGE_STR="2026-11-18 10:37:08"
LOG="$WT/poll-loop-recovery2.log"
N=66
echo "$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S AEDT') LOOP-START recovery-2: next poll 67, wedge 10:37:08 AEDT" >> "$LOG"
while true; do
  NOWSTR=$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S')
  if [ "$NOWSTR" \>= "$WEDGE_STR" ]; then
    echo "$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S AEDT') WEDGE-REACHED: gate closed, loop exiting (WAIT-DISK)" >> "$LOG"
    exit 10
  fi
  N=$((N+1))
  BEFORE_MTIME=$(stat -f %m "$WT/progress-t728.txt")
  OUT=$(bash "$WT/scripts/df_poll_t728.sh")
  AFTER_MTIME=$(stat -f %m "$WT/progress-t728.txt")
  if [ "$AFTER_MTIME" -le "$BEFORE_MTIME" ]; then
    echo "$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S AEDT') POLL $N APPEND-FAIL: progress mtime did not advance ($OUT)" >> "$LOG"
  else
    echo "$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S AEDT') POLL $N OK: $OUT" >> "$LOG"
  fi
  case "$OUT" in
    GATE_OPEN*) echo "$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S AEDT') GATE_OPEN at poll $N - loop stopping for flight launch" >> "$LOG"; exit 0;;
  esac
  sleep 300
done
