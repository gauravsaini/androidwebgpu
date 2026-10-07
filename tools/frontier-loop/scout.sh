#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if command -v uv >/dev/null 2>&1; then
  exec uv run --no-project python "$ROOT/tools/frontier-loop/scout.py" "$@"
fi
exec python3 "$ROOT/tools/frontier-loop/scout.py" "$@"
