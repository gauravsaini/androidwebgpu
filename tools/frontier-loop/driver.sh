#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(git -C "$SCRIPT_DIR/../.." rev-parse --show-toplevel)
LOCK="$SCRIPT_DIR/.lock"
RUN_DIR=${FRONTIER_RUN_DIR:-/Users/Shared/codex-logs/frontier-loop}
EXCEPTION_FILE=${FRONTIER_EXCEPTION_FILE:-/Users/Shared/codex-track71-exceptions.md}
PYTHON_BIN=$(command -v python3)
ACTIVE_AGENT_PID=""

if [[ -e "$LOCK" ]]; then
  printf 'frontier loop refused: lock exists at %s\n' "$LOCK" >&2
  exit 1
fi
if ! mkdir "$LOCK" 2>/dev/null; then
  printf 'frontier loop refused: another driver owns %s\n' "$LOCK" >&2
  exit 1
fi
printf '%s\n' "$$" > "$LOCK/pid"

terminate_agent() {
  local pid=$1
  local attempt
  kill -TERM "$pid" 2>/dev/null || return 0
  for ((attempt = 0; attempt < 5; attempt++)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  kill -KILL "$pid" 2>/dev/null || true
}

cleanup() {
  if [[ -n "$ACTIVE_AGENT_PID" ]] && kill -0 "$ACTIVE_AGENT_PID" 2>/dev/null; then
    terminate_agent "$ACTIVE_AGENT_PID"
  fi
  rm -f "$LOCK/pid"
  rmdir "$LOCK" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

python_run() {
  if command -v uv >/dev/null 2>&1; then
    uv run --offline --no-project --python "$PYTHON_BIN" python "$@"
  else
    "$PYTHON_BIN" "$@"
  fi
}

run_kernel() {
  # The adapter fixes the guest inputs and forwards T69/T70 CLI flags unchanged.
  if [[ -n ${FRONTIER_RUN_KERNEL:-} ]]; then
    "$FRONTIER_RUN_KERNEL" "$@"
  else
    command run_kernel "$@"
  fi
}

run_kernel_in() {
  local directory=$1
  shift
  (cd "$directory" && run_kernel "$@")
}

file_signature() {
  local file=$1
  if [[ ! -f "$file" ]]; then
    printf 'absent'
    return
  fi
  local modified size digest
  modified=$(stat -f '%m' "$file" 2>/dev/null || stat -c '%Y' "$file" 2>/dev/null || printf '0')
  size=$(wc -c < "$file" | tr -d '[:space:]')
  digest=$(shasum -a 256 "$file" | awk '{print $1}')
  printf '%s:%s:%s' "$modified" "$size" "$digest"
}

record_exception() {
  local reason=$1
  local report=$2
  local details=${3:-'{}'}
  {
    printf '\n## %s — %s\n\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')" "$reason"
    printf -- '- Base revision: `%s`\n' "$(git -C "$REPO_ROOT" rev-parse HEAD)"
    printf -- '- Report: `%s`\n' "$report"
    printf -- '- Details: `%s`\n' "$details"
  } >> "$EXCEPTION_FILE"
}

wait_for_findings() {
  local metadata=$1
  local findings_path pid last_signature signature now last_progress
  findings_path=$(jq -r '.findings_path' <<< "$metadata")
  pid=$(jq -r '.pid' <<< "$metadata")
  last_signature=$(file_signature "$findings_path")
  last_progress=$(date +%s)

  while true; do
    signature=$(file_signature "$findings_path")
    if [[ "$signature" != "$last_signature" ]]; then
      last_signature=$signature
      last_progress=$(date +%s)
    fi

    if [[ -s "$findings_path" ]] && grep -Eq '^VERDICT:[[:space:]]*(PASS|FAIL|BLOCKED)' "$findings_path"; then
      if ! kill -0 "$pid" 2>/dev/null; then
        return 0
      fi
    fi

    now=$(date +%s)
    if (( now - last_progress >= 600 )); then
      printf 'frontier agent %s made no findings progress for 10 minutes; terminating pid %s\n' "$findings_path" "$pid" >&2
      terminate_agent "$pid"
      return 1
    fi
    sleep 10
  done
}

merge_fix_to_integ() {
  local fix_commit=$1
  git -C "$REPO_ROOT" merge --no-ff "$fix_commit" -m "frontier-loop: integrate $fix_commit"
}

save_snapshot_at() {
  local halt_step=$1
  local fix_commit=$2
  local snapshot_step snapshot_path
  if (( halt_step > 1000000 )); then
    snapshot_step=$((halt_step - 1000000))
  else
    snapshot_step=0
  fi
  snapshot_path="$RUN_DIR/snapshot-${fix_commit:0:12}-step-${snapshot_step}.bin"
  run_kernel_in "$REPO_ROOT" --save-snapshot "$snapshot_path" --snapshot-at-step "$snapshot_step" >&2
  if [[ ! -f "$snapshot_path" ]]; then
    printf 'run_kernel completed but did not create snapshot: %s\n' "$snapshot_path" >&2
    return 1
  fi
  printf '%s' "$snapshot_path"
}

mkdir -p "$RUN_DIR"
REV=$(git -C "$REPO_ROOT" rev-parse HEAD)
SNAP=""
printf 'frontier loop started at %s (lock %s)\n' "$REV" "$LOCK"

while true; do
  ITERATION_DIR=$(mktemp -d "$RUN_DIR/iteration-XXXXXX")
  REPORT="$ITERATION_DIR/report.json"
  CLASS="$ITERATION_DIR/classification.json"

  if [[ -n "$SNAP" ]]; then
    run_kernel_in "$REPO_ROOT" --load-snapshot "$SNAP" --boot-report-json "$REPORT"
  else
    run_kernel_in "$REPO_ROOT" --boot-report-json "$REPORT"
  fi

  if jq -e '.uart.init_marker == true' "$REPORT" >/dev/null; then
    printf 'BADA GOAL REACHED\n'
    break
  fi

  REPORT_REV=$(jq -r '.binary.git_rev' "$REPORT")
  if [[ "$REPORT_REV" != "$REV" ]]; then
    record_exception 'boot report binary revision differs from integration HEAD' "$REPORT" "head=$REV report_git_rev=$REPORT_REV"
    break
  fi

  if ! python_run "$SCRIPT_DIR/classify.py" "$REPORT" > "$CLASS"; then
    record_exception 'classification failed' "$REPORT" "$(cat "$CLASS" 2>/dev/null || printf '{}')"
    break
  fi
  CATEGORY=$(jq -r '.category' "$CLASS")
  if [[ "$CATEGORY" == 'unknown' ]]; then
    record_exception 'human classification required' "$REPORT" "$(cat "$CLASS")"
    break
  fi

  if ! AGENT=$(python_run "$SCRIPT_DIR/launch.py" "$REPORT" "$CLASS" --repo-root "$REPO_ROOT"); then
    record_exception 'agent launch failed' "$REPORT" "$(cat "$CLASS")"
    break
  fi
  ACTIVE_AGENT_PID=$(jq -r '.pid' <<< "$AGENT")
  FINDINGS=$(jq -r '.findings_path' <<< "$AGENT")
  AGENT_WORKTREE=$(jq -r '.worktree' <<< "$AGENT")
  printf 'launched one frontier agent: %s (pid %s)\n' "$FINDINGS" "$ACTIVE_AGENT_PID"

  if ! wait_for_findings "$AGENT"; then
    record_exception 'agent findings timed out' "$REPORT" "findings=$FINDINGS log=$(jq -r '.log_path' <<< "$AGENT")"
    break
  fi
  ACTIVE_AGENT_PID=""

  VERDICT=$(sed -nE 's/^VERDICT:[[:space:]]*([^[:space:]]+).*$/\1/p' "$FINDINGS" | tail -n 1)
  if [[ "$VERDICT" != 'PASS' ]]; then
    record_exception 'agent did not produce a passing fix' "$REPORT" "findings=$FINDINGS verdict=${VERDICT:-missing}"
    break
  fi
  FIXREV=$(sed -nE 's/^FIX_COMMIT:[[:space:]]*([[:xdigit:]]{40})[[:space:]]*$/\1/p' "$FINDINGS" | tail -n 1)
  if [[ -z "$FIXREV" ]]; then
    record_exception 'agent findings omit full fix commit' "$REPORT" "findings=$FINDINGS"
    break
  fi

  REPORT2="$ITERATION_DIR/report2.json"
  if [[ -n "$SNAP" ]]; then
    run_kernel_in "$AGENT_WORKTREE" --load-snapshot "$SNAP" --boot-report-json "$REPORT2"
  else
    run_kernel_in "$AGENT_WORKTREE" --boot-report-json "$REPORT2"
  fi

  if GATE_RESULT=$(python_run "$SCRIPT_DIR/gate.py" "$REPORT" "$REPORT2" "$FIXREV" --repo-root "$REPO_ROOT"); then
    printf 'merge gate: %s\n' "$GATE_RESULT"
  else
    record_exception 'merge gate failed' "$REPORT2" "$GATE_RESULT"
    printf 'merge gate failed; human integrator must review %s\n' "$EXCEPTION_FILE" >&2
    break
  fi

  if ! merge_fix_to_integ "$FIXREV"; then
    record_exception 'fix merge failed' "$REPORT2" "fix=$FIXREV"
    break
  fi
  REV=$(git -C "$REPO_ROOT" rev-parse HEAD)
  printf 'integrated %s; new integration HEAD is %s\n' "$FIXREV" "$REV"

  if ! SNAP=$(save_snapshot_at "$(jq -r '.run.halt_step' "$REPORT2")" "$FIXREV"); then
    record_exception 'snapshot creation failed' "$REPORT2" "fix=$FIXREV"
    break
  fi
done
