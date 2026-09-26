#!/bin/sh
# Unlazy CHECK oracle for unit crates: build | test | clippy.
# Prints "<crate> <phase> passed" ONLY on success. For test, requires >=1 test run.
set -u
crate="$1"
phase="$2"
filter="${3:-}"
log="/tmp/unlazy-unit.log"
case "$phase" in
  build) cmd="cargo build -p $crate" ;;
  test) cmd="cargo test -p $crate $filter" ;;
  clippy) cmd="cargo clippy -p $crate --all-targets -- -D warnings" ;;
  fmt) cmd="cargo fmt -p $crate -- --check" ;;
  *) echo "unknown phase: $phase"; exit 2 ;;
esac
if sh -c "$cmd" > "$log" 2>&1; then
  if [ "$phase" = "test" ]; then
    if grep -q "test result: ok" "$log" && ! grep -q "0 passed" "$log"; then
      echo "$crate test passed"
    else
      echo "no tests ran or not ok:"
      tail -20 "$log"
      exit 1
    fi
  else
    echo "$crate $phase passed"
  fi
else
  tail -30 "$log"
  exit 1
fi
