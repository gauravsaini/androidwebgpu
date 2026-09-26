#!/bin/sh
# Unlazy CHECK oracle: clippy with warnings denied on one crate.
# Prints the success marker ONLY when clippy exits 0.
set -u
crate="$1"
if cargo clippy -p "$crate" --all-targets -- -D warnings > /tmp/unlazy-clippy.log 2>&1; then
  echo "contracts clippy passed"
else
  tail -40 /tmp/unlazy-clippy.log
  exit 1
fi
