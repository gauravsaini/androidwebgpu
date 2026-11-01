#!/bin/bash
set -euo pipefail

cd /Users/Shared/wt-track645-m3-ten-run

# Disk hygiene check before launch
AVAIL_KB=$(df -k /System/Volumes/Data | tail -1 | awk '{print $4}')
if [ "$AVAIL_KB" -lt 2097152 ]; then
  echo "Disk space below 2GiB ($AVAIL_KB KB). Aborting."
  exit 1
fi

exec python3 scripts/supervisor-t645.py
