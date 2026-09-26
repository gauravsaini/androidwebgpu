#!/bin/sh
# Unlazy CHECK oracle: run check-unit.sh <phase> over every crate of a wave.
# usage: check-wave.sh <1|2> <build|test|clippy>
set -u
wave="$1"
phase="$2"
case "$wave" in
  1) crates="u1-decode u2-ir-lift u3-wasm-jit u11-snapshot u10-analyzer u14-metrics" ;;
  2) crates="u4-mmu u5-gic-timer u6-virtio-transport u7-gpu-device u8-gpu-host" ;;
  *) echo "unknown wave: $wave"; exit 2 ;;
esac
for c in $crates; do
  if ! sh "$(dirname "$0")/check-unit.sh" "$c" "$phase"; then
    echo "wave $wave $phase FAILED at $c"
    exit 1
  fi
done
echo "wave $wave $phase passed"
