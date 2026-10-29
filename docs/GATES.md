# Gates: Track T643 — rust-init-marker-silence: why the PID-1 marker stays silent after init-exec (Chain B, diagnosis + fix-or-diagnosis)

OWNS: dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, units/u12-orchestrator/**

Scope: Pin the silence mechanism why PID-1 marker stays silent after init-exec: (a) PC histogram EL0 vs EL1 after t=8.14s, (b) console-path verdict on "unable to open an initial console", (c) PC 0xffffff80085c95ec function and EL0 data abort disposition, (d) pin jiffies probe PA, (e) stuck-vs-slow verdict with steps/guest-second. Then either minimal engine fix proven by guest-consumed run or quantified stuck-vs-slow verdict with step budget.

- [x] G643-1: Silence mechanism pinned with five-point evidence (EL0 execution, console path, PC/dabt disposition, jiffies PA, stuck-vs-slow quantification)
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track643-rust-init-marker-silence.md').read(); assert '0xffffff80085c95ec' in f; assert 'find_next_bit' in f; assert 'unable to open an initial console' in f; assert 'jiffies' in f; assert 'Run /init as init process' in f; print('MECHANISM_PINNED_OK')"
  EXPECT: MECHANISM_PINNED_OK

- [x] G643-2: Findings artifact and outcome delivered with Verdict, WALL-TYPE-TAG: engine (type-2), required strings, and SHA-256 logged
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track643-rust-init-marker-silence.md').read(); assert ('Verdict: PASS' in f or 'Verdict: FAIL' in f); assert 'WALL-TYPE-TAG: engine (type-2)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'el0-fetch-fault-210158' in f; assert 'rust-init-marker-silent-300m' in f; assert 'PATHN-TRACK-B' in f; assert open('/Users/Shared/outcome-t643.txt').read(); print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
