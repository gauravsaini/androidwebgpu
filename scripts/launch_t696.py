#!/usr/bin/env python3
"""Detach launcher for T696: new session, stdin /dev/null, stdout/stderr to a log, exec supervisor."""
import os
import sys

worktree = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
log_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(worktree, "supervisor_t696.log")
os.chdir(worktree)
os.setsid()
fd_in = os.open('/dev/null', os.O_RDONLY)
os.dup2(fd_in, 0)
fd_out = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
os.dup2(fd_out, 1)
os.dup2(fd_out, 2)
os.execvp(sys.executable, [sys.executable, 'scripts/supervisor_t696.py'])
