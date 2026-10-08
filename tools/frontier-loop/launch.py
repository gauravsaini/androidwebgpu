#!/usr/bin/env python3
"""Create an isolated worktree and launch one detached frontier Codex agent."""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

from classify import load_and_validate


SHARED = Path(os.environ.get("FRONTIER_SHARED_ROOT", "/Users/Shared"))
REGISTRY = Path(os.environ.get("FRONTIER_REGISTRY_PATH", SHARED / "codex-track-REGISTRY.md"))
LOG_DIR = Path(os.environ.get("FRONTIER_LOG_DIR", SHARED / "codex-logs"))
CODEX = Path(os.environ.get("FRONTIER_CODEX", "/Users/ektasaini/.local/bin/codex"))
ORACLES = {
    "unmapped_mmio": "/Users/Shared/codex-logs/track66-qemu-mmio.trace",
    "unknown_insn": "aarch64 objdump + capstone on the Image",
    "sysreg": "TBD (qemu_sysreg_dump has not been captured)",
    "fault_divergence": "QEMU run to the same step; compare the first divergence",
    "spin": "kernel symbols + poll-target analysis",
}
TEMPLATES = {
    "unmapped_mmio": "device.md",
    "unknown_insn": "decode.md",
    "sysreg": "sysreg.md",
    "fault_divergence": "divergence.md",
    "spin": "spin.md",
}


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


def next_track_id(contents: str) -> int:
    numbers = [int(match) for match in re.findall(r"^\|\s*(\d+)\s*\|", contents, re.MULTILINE)]
    return max([71, *numbers]) + 1


def update_registry(track_id: int, row: str, replacement: str | None = None) -> None:
    REGISTRY.parent.mkdir(parents=True, exist_ok=True)
    with REGISTRY.open("a+", encoding="utf-8") as stream:
        fcntl.flock(stream.fileno(), fcntl.LOCK_EX)
        stream.seek(0)
        lines = stream.readlines()
        found = False
        for index, line in enumerate(lines):
            if re.match(rf"\|\s*{track_id}\s*\|", line):
                found = True
                if replacement is not None:
                    lines[index] = replacement
        if not found:
            lines.append(row)
        stream.seek(0)
        stream.truncate()
        stream.writelines(lines)
        stream.flush()
        os.fsync(stream.fileno())
        fcntl.flock(stream.fileno(), fcntl.LOCK_UN)


def parse_classification(value: str) -> dict[str, str]:
    candidate = Path(value)
    try:
        decoded = json.loads(candidate.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        decoded = {"category": value}
    if isinstance(decoded, dict) and isinstance(decoded.get("classification"), dict):
        decoded = decoded["classification"]
    if not isinstance(decoded, dict) or not isinstance(decoded.get("category"), str):
        raise ValueError("category must be a category name or JSON containing classification.category")
    return decoded


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("boot_report")
    parser.add_argument("category", help="category string or classify.py JSON output file")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args(argv)
    track_id = None
    worktree = None
    registry_row = None

    try:
        report = load_and_validate(args.boot_report)
        classification = parse_classification(args.category)
        category = classification["category"]
        if category not in TEMPLATES:
            raise ValueError(f"category {category!r} has no auto-fix brief; send it to the human integrator")
        if not (Path(__file__).resolve().parent / ".lock").exists():
            raise ValueError("launch.py must be called by the driver while its .lock is held")
        if not CODEX.is_file() or not os.access(CODEX, os.X_OK):
            raise RuntimeError(f"Codex executable is missing or not executable: {CODEX}")
        nohup = shutil.which("nohup")
        if nohup is None:
            raise RuntimeError("nohup executable was not found")

        repo = args.repo_root.resolve()
        base_commit = git(repo, "rev-parse", "HEAD")
        now = datetime.now(ZoneInfo("Australia/Melbourne"))
        stamp = now.strftime("%Y%m%d-%H%M%S")
        LOG_DIR.mkdir(parents=True, exist_ok=True)
        with REGISTRY.open("r", encoding="utf-8") as stream:
            fcntl.flock(stream.fileno(), fcntl.LOCK_SH)
            registry_text = stream.read()
            fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
        track_id = next_track_id(registry_text)
        worktree = SHARED / f"wt-track{track_id}-frontier-{category}"
        suffix = 1
        while worktree.exists():
            suffix += 1
            worktree = SHARED / f"wt-track{track_id}-frontier-{category}-{stamp}-{suffix}"
        findings = SHARED / f"codex-track{track_id}-frontier-{category}.md"
        log_path = LOG_DIR / f"track{track_id}-frontier-{category}.log"
        prompt_path = LOG_DIR / f"track{track_id}-frontier-{category}.prompt.md"

        subprocess.run(
            ["git", "-C", str(repo), "worktree", "add", "--detach", str(worktree), base_commit],
            check=True,
            stdout=sys.stderr,
        )
        row = (
            f"| {track_id} | frontier-{category} | {worktree} | {findings} | running | "
            f"{now.strftime('%Y-%m-%d %H:%M')} |\n"
        )
        registry_row = row
        update_registry(track_id, registry_row)

        template = (Path(__file__).resolve().parent / "briefs" / TEMPLATES[category]).read_text(encoding="utf-8")
        replacements = {
            "{{base_commit}}": base_commit,
            "{{halt_json}}": json.dumps(report["halt"], indent=2, sort_keys=True),
            "{{oracle_path}}": ORACLES[category],
            "{{worktree}}": str(worktree),
            "{{findings_path}}": str(findings),
            "{{log_path}}": str(log_path),
        }
        prompt = template
        for token, replacement in replacements.items():
            prompt = prompt.replace(token, replacement)
        prompt = prompt.replace("/Users/Shared/codex-track-REGISTRY.md", str(REGISTRY))
        prompt += (
            "\n\n## Driver handoff\n"
            f"Write the final findings to `{findings}`. The driver waits for this file. "
            "Start it with `VERDICT: PASS`, `VERDICT: FAIL`, or `VERDICT: BLOCKED` and "
            "include the exact `FIX_COMMIT: <full commit sha>` line when a passing fix is committed.\n"
        )
        prompt_path.write_text(prompt, encoding="utf-8")

        with log_path.open("ab", buffering=0) as log:
            process = subprocess.Popen(
                [
                    nohup,
                    str(CODEX),
                    "exec",
                    "--skip-git-repo-check",
                    "-m",
                    "gpt-6-luna",
                    "-c",
                    'model_reasoning_effort="max"',
                    "-s",
                    "workspace-write",
                    "--add-dir",
                    str(SHARED),
                    prompt,
                ],
                cwd=worktree,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
                close_fds=True,
            )
        print(
            json.dumps(
                {
                    "track_id": track_id,
                    "pid": process.pid,
                    "base_commit": base_commit,
                    "worktree": str(worktree),
                    "findings_path": str(findings),
                    "log_path": str(log_path),
                    "prompt_path": str(prompt_path),
                },
                sort_keys=True,
            )
        )
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        if track_id is not None and registry_row is not None:
            update_registry(
                track_id,
                registry_row,
                registry_row.replace("| running |", "| dead (launch failed) |"),
            )
        if worktree is not None and worktree.exists():
            subprocess.run(
                ["git", "-C", str(args.repo_root.resolve()), "worktree", "remove", "--force", str(worktree)],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        print(f"frontier launch failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
