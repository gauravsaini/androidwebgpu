#!/bin/sh
# Unlazy CHECK oracle: build one contracts crate.
# Prints the success marker ONLY when cargo exits 0.
set -u
crate="$1"
if cargo build -p "$crate" > /tmp/unlazy-build.log 2>&1; then
  echo "contracts build passed"
else
  tail -30 /tmp/unlazy-build.log
  exit 1
fi
