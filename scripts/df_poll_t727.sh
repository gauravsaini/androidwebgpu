#!/bin/bash
# T727 Step 4 disk poll: reads df, appends a poll line to progress-t727.txt,
# refreshes heartbeat.json, prints GATE_OPEN or GATE_CLOSED + avail KiB.
WT=/Users/Shared/wt-track727-toybox-discriminator
AVAIL=$(df -k /System/Volumes/Data | tail -1 | awk '{print $4}')
TS=$(TZ=Australia/Sydney date '+%Y-%m-%d %H:%M:%S AEDT')
UTC=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
echo "- Step 4 poll $TS: df avail $AVAIL KiB (flight threshold 9,769,216 KiB)." >> "$WT/progress-t727.txt"
printf '{\n  "ts": "%s",\n  "status": "RUNNING",\n  "stage": "step4-polling-disk",\n  "note": "T727 Step 4 polling disk: avail %s KiB at %s; gate opens at 9,769,216 KiB."\n}\n' "$UTC" "$AVAIL" "$TS" > "$WT/heartbeat.json"
if [ "$AVAIL" -ge 9769216 ]; then echo "GATE_OPEN $AVAIL"; else echo "GATE_CLOSED $AVAIL"; fi
