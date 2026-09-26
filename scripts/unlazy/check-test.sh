#!/bin/sh
# Unlazy CHECK oracle: run contracts crate tests (optional name filter).
# Prints the success marker ONLY when cargo test exits 0.
set -u
crate="$1"
filter="${2:-}"
if cargo test -p "$crate" $filter > /tmp/unlazy-test.log 2>&1; then
  echo "contracts tests passed"
else
  tail -40 /tmp/unlazy-test.log
  exit 1
fi
