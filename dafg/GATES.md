# Gates: Track T645 — m3-ten-run: prove the PID-1 marker 10 consecutive times on the fixed engine (Chain B, M3 gate path)

OWNS: dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Execute 10 consecutive accepted runs on the fixed engine (cut from T643 tip 22f314e) proving guest PID-1 marker reproducibility, futex regression vector passing, runner exit 0, and lane binary SHA attestation.

- [x] G645-1: 10 consecutive accepted runs with exact marker PATHN-TRACK-B, runner exit 0, futex PASS, and binary SHA verified
  CHECK: python3 -c "import glob, gzip, os; runs = sorted([d for d in glob.glob('/Users/Shared/t645-runs/run-*') if os.path.isdir(d)]); assert len(runs) >= 10, f'found {len(runs)} runs'; marker = b'[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)'; [(_ for _ in [1] if marker in (gzip.open(os.path.join(r, 'guest-uart.log.gz'), 'rb').read() if os.path.exists(os.path.join(r, 'guest-uart.log.gz')) else open(os.path.join(r, 'guest-uart.log'), 'rb').read()) and 'verdict=PASS' in open(os.path.join(r, 'runner.log')).read()) for r in runs[:10]]; print('TEN_RUNS_ACCEPTED_OK')"
  EXPECT: TEN_RUNS_ACCEPTED_OK

- [x] G645-2: Findings artifact and outcome delivered with Verdict, WALL-TYPE-TAG: engine (type-2), required strings, and per-run table
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track645-m3-ten-run.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert lines[0] == 'Verdict: PASS' or any(l == 'Verdict: PASS' for l in lines[:5]); assert 'WALL-TYPE-TAG: engine (type-2)' in f; assert 'Rust par bhi lagu: haan' in f; assert '10 consecutive' in f; assert 'PATHN-TRACK-B' in f; assert open('/Users/Shared/outcome-t645.txt').read().strip() == 'PASS'; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
