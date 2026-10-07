#!/usr/bin/env python3
"""Check focused tests, a pre-fix regression failure, and frontier progress."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Sequence

from classify import load_and_validate


def evaluate_gate(
    old_report: dict[str, Any],
    new_report: dict[str, Any],
    fix_commit: str,
    focused_tests_green: bool,
    regression_vector_verified: bool,
) -> dict[str, Any]:
    """Evaluate the three required merge conditions without running commands."""
    reasons: list[str] = []
    checks = {
        "focused_tests_green": bool(focused_tests_green),
        "regression_vector_verified": bool(regression_vector_verified),
        "frontier_advanced": False,
        "binary_matches_fix": new_report.get("binary", {}).get("git_rev") == fix_commit,
    }
    old_step = old_report.get("run", {}).get("halt_step")
    new_step = new_report.get("run", {}).get("halt_step")
    checks["frontier_advanced"] = (
        isinstance(old_step, int)
        and not isinstance(old_step, bool)
        and isinstance(new_step, int)
        and not isinstance(new_step, bool)
        and new_step > old_step
    )

    if not checks["focused_tests_green"]:
        reasons.append("focused_tests_failed")
    if not checks["regression_vector_verified"]:
        reasons.append("regression_vector_missing_or_pre_fix_passed")
    if not checks["frontier_advanced"]:
        reasons.append("frontier_did_not_advance")
    if not checks["binary_matches_fix"]:
        reasons.append("new_binary_revision_does_not_match_fix_commit")

    return {
        "status": "PASS" if not reasons else "FAIL",
        "reason": reasons,
        "checks": checks,
        "old_halt_step": old_step,
        "new_halt_step": new_step,
        "fix_commit": fix_commit,
        "new_binary_git_rev": new_report.get("binary", {}).get("git_rev"),
    }


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


def changed_files(repo: Path, fix_commit: str) -> list[str]:
    output = git(
        repo,
        "diff-tree",
        "--root",
        "--no-commit-id",
        "--name-only",
        "--diff-filter=AMR",
        "-r",
        fix_commit,
    ).stdout
    return [line for line in output.splitlines() if line]


def changed_files_between(repo: Path, parent: str, fix_commit: str) -> list[str]:
    output = git(repo, "diff", "--name-only", parent, fix_commit).stdout
    return [line for line in output.splitlines() if line]


def is_test_file(path: str) -> bool:
    parts = Path(path).parts
    candidate = Path(path)
    name = candidate.name
    if candidate.suffix not in {".rs", ".py", ".js", ".mjs", ".cjs"}:
        return False
    in_test_dir = any(part.lower() in {"test", "tests", "testing"} for part in parts[:-1])
    test_name = name.startswith("test_") or name.endswith("_test.py") or name.endswith(".test.js") or name.endswith(".spec.js")
    return in_test_dir or test_name


def test_files_for_commit(repo: Path, fix_commit: str) -> list[str]:
    return [path for path in changed_files(repo, fix_commit) if is_test_file(path)]


def test_commands(repo: Path, test_files: Sequence[str]) -> list[tuple[list[str], str]]:
    commands: list[tuple[list[str], str]] = []
    for test_path in test_files:
        path = Path(test_path)
        if path.suffix == ".rs":
            crate_manifest = None
            for parent in (repo / path).parents:
                candidate = parent / "Cargo.toml"
                if candidate.is_file():
                    crate_manifest = candidate
                    break
            if crate_manifest is None:
                command = ["cargo", "test", "--workspace", "--test", path.stem]
            elif path.parent.name == "tests":
                command = [
                    "cargo",
                    "test",
                    "--manifest-path",
                    str(crate_manifest),
                    "--test",
                    path.stem,
                ]
            else:
                command = ["cargo", "test", "--manifest-path", str(crate_manifest)]
        elif path.suffix == ".py":
            command = [sys.executable, str(repo / path)]
        elif path.suffix in {".js", ".mjs", ".cjs"}:
            command = ["node", str(repo / path)]
        else:
            command = ["make", "test"]
        commands.append((command, test_path))
    return commands


def run_commands(repo: Path, commands: Sequence[tuple[list[str], str]]) -> tuple[bool, list[dict[str, Any]]]:
    evidence: list[dict[str, Any]] = []
    all_green = bool(commands)
    for command, test_path in commands:
        result = subprocess.run(
            command,
            cwd=repo,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        evidence.append(
            {
                "test_file": test_path,
                "command": command,
                "exit_code": result.returncode,
                "output_tail": result.stdout[-4000:],
            }
        )
        all_green = all_green and result.returncode == 0
    return all_green, evidence


def test_names_from_source(source: str, path: str) -> list[str]:
    if path.endswith(".py"):
        return re.findall(r"^\s*def\s+(test\w+)\s*\(", source, flags=re.MULTILINE)
    if path.endswith(".rs"):
        return re.findall(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", source)
    return []


def is_test_failure(output: str, test_names: Sequence[str]) -> bool:
    markers = ("test result: FAILED", "FAILED (failures=", "FAILED (errors=", "FAIL:", "AssertionError")
    if not any(marker in output for marker in markers):
        return False
    return not test_names or any(re.search(rf"\b{re.escape(name)}\b", output) for name in test_names)


def worktree_add(repo: Path, destination: Path, revision: str) -> None:
    git(repo, "worktree", "add", "--detach", str(destination), revision)


def worktree_remove(repo: Path, destination: Path) -> None:
    git(repo, "worktree", "remove", "--force", str(destination), check=False)


def git_blob(repo: Path, revision: str, path: str) -> bytes | None:
    result = subprocess.run(
        ["git", "-C", str(repo), "show", f"{revision}:{path}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return result.stdout if result.returncode == 0 else None


def clean_gate_stash(repo: Path, stash_oid: str | None) -> None:
    if stash_oid is None:
        return
    entries = git(repo, "stash", "list", "--format=%gd %H", check=False).stdout.splitlines()
    for entry in entries:
        reference, oid = entry.split(maxsplit=1)
        if oid == stash_oid:
            git(repo, "stash", "drop", reference, check=False)
            return


def run_git_tests(repo: Path, fix_commit: str, paths: Sequence[str]) -> tuple[bool, bool, list[dict[str, Any]], list[dict[str, Any]]]:
    """Run tests at the fix, then stash its implementation to test the old tree."""
    if not paths:
        return False, False, [], []

    parent_line = git(repo, "rev-list", "--parents", "-n", "1", fix_commit).stdout.strip().split()
    if len(parent_line) < 2:
        return False, False, [], [{"error": "fix commit has no parent to use as the pre-fix tree"}]
    parent = parent_line[1]
    fixed_ok = False
    regression_verified = False
    fixed_evidence: list[dict[str, Any]] = []
    pre_fix_evidence: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="frontier-gate-") as temp_name:
        temporary = Path(temp_name)
        fixed_tree = temporary / "fixed"
        pre_fix_tree = temporary / "pre-fix"
        gate_stash_oid = None
        try:
            worktree_add(repo, fixed_tree, fix_commit)
            fixed_commands = test_commands(fixed_tree, paths)
            fixed_ok, fixed_evidence = run_commands(fixed_tree, fixed_commands)

            worktree_add(repo, pre_fix_tree, parent)
            pre_fix_names: dict[str, list[str]] = {}
            test_paths = set(paths)
            for path in changed_files_between(repo, parent, fix_commit):
                content = git_blob(repo, fix_commit, path)
                destination = pre_fix_tree / path
                if content is None:
                    if destination.exists():
                        destination.unlink()
                    continue
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(content)
                if path in test_paths:
                    pre_fix_names[path] = test_names_from_source(content.decode("utf-8", errors="replace"), path)

            git(pre_fix_tree, "add", "--", *paths)
            old_stash = git(pre_fix_tree, "rev-parse", "--verify", "--quiet", "refs/stash", check=False).stdout.strip()
            stash_result = git(
                pre_fix_tree,
                "stash",
                "push",
                "--keep-index",
                "--include-untracked",
                "-m",
                f"frontier-gate-pre-fix-{os.getpid()}",
                check=False,
            )
            current_stash = git(
                pre_fix_tree, "rev-parse", "--verify", "--quiet", "refs/stash", check=False
            ).stdout.strip()
            if current_stash and current_stash != old_stash:
                gate_stash_oid = current_stash
            if stash_result.returncode != 0 or gate_stash_oid is None:
                pre_fix_evidence.append(
                    {
                        "error": "could not stash the implementation while keeping regression tests staged",
                        "stash_output": stash_result.stdout,
                    }
                )
                return fixed_ok, False, fixed_evidence, pre_fix_evidence

            pre_fix_commands = test_commands(pre_fix_tree, paths)
            failures: list[bool] = []
            for command, test_path in pre_fix_commands:
                result = subprocess.run(
                    command,
                    cwd=pre_fix_tree,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    check=False,
                )
                failed_as_test = result.returncode != 0 and is_test_failure(
                    result.stdout, pre_fix_names.get(test_path, [])
                )
                failures.append(failed_as_test)
                pre_fix_evidence.append(
                    {
                        "test_file": test_path,
                        "command": command,
                        "verification_method": "git stash push --keep-index --include-untracked",
                        "exit_code": result.returncode,
                        "failed_as_regression": failed_as_test,
                        "output_tail": result.stdout[-4000:],
                    }
                )
            regression_verified = any(failures)
        except (OSError, subprocess.CalledProcessError) as exc:
            pre_fix_evidence.append({"error": str(exc)})
        finally:
            clean_gate_stash(repo, gate_stash_oid)
            worktree_remove(repo, pre_fix_tree)
            worktree_remove(repo, fixed_tree)

    return fixed_ok, regression_verified, fixed_evidence, pre_fix_evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("old_report")
    parser.add_argument("new_report")
    parser.add_argument("fix_commit")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args(argv)
    repo = args.repo_root.resolve()

    try:
        old_report = load_and_validate(args.old_report)
        new_report = load_and_validate(args.new_report)
        full_fix_commit = git(repo, "rev-parse", args.fix_commit).stdout.strip()
        paths = test_files_for_commit(repo, full_fix_commit)
        focused_ok, regression_ok, focused_evidence, regression_evidence = run_git_tests(
            repo, full_fix_commit, paths
        )
        result = evaluate_gate(old_report, new_report, full_fix_commit, focused_ok, regression_ok)
        result["regression_test_files"] = paths
        result["focused_test_evidence"] = focused_evidence
        result["pre_fix_test_evidence"] = regression_evidence
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        result = {
            "status": "FAIL",
            "reason": ["gate_input_or_execution_error"],
            "details": str(exc),
            "checks": {
                "focused_tests_green": False,
                "regression_vector_verified": False,
                "frontier_advanced": False,
                "binary_matches_fix": False,
            },
        }
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
