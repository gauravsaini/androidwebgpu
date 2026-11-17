# TIMELINE — Track T728: observable-payload service

- 01:25:32 AEDT 18 Nov 2026 — Lane launched (wedge 04:25:32). Brief c0bb3d26, worktree wt-track728-observable-payload, branch t728, base a395f88.
- 01:25:47 AEDT — Step 0: df 9,452,928 KiB — gate CLOSED (shortfall ~309 MiB). Static-first: Steps 1–3 proceed.
- 01:28 AEDT — Step 1 PASS: all input SHAs MATCH (kernel 70d3859d, dtb 73d1904b, CPIO a9d6040a, system 8e9914e3, vendor 5e0607ad, t723-probe 4b275e48, patch cfe7d1ee). Box work-copy + revert verified.
- 01:35 AEDT — Step 2 PASS: helper built (19db5d31, static aarch64), parser ACCEPT 0 errors, markers unique (0 in T724–T727 UARTs). No qemu-aarch64 in the fleet — static verification is the host proof, flight is the runtime proof.
- 01:45 AEDT — Step 3 PASS: image built on box via Linux debugfs (9b8ef7ff; neighbours byte-identical, e2fsck clean, 7344/7424 files; bad-type inode caught + fixed via sif, dump-backs identical); patch 7ecbe959 (8 chunks); Mac APFS-clone apply, Mac image SHA 9b8ef7ff MATCHES. Analyzer sanity reproduces driver counts exactly.
- 04:25:32 AEDT — Wedge reached, gate CLOSED (final poll 04:24:10, 9,282,240 KiB). Clean WAIT-DISK exit: 32 df readings, gate never opened, no flight. Steps 0–3 PASS preserved; recovery pending driver relaunch.
- 07:32:51 AEDT — Wedge reached, gate CLOSED (final poll 07:27:46, 7,738,300 KiB). Clean WAIT-DISK exit, recovery 1: 34 polls (32-65), gate never opened, no flight. Steps 0-3 PASS preserved; recovery pending driver relaunch.
- 07:37:08 AEDT — Recovery 2 launched (wedge 10:37:08). SHAs re-verified MATCH (image 9b8ef7ff, patch 7ecbe959, rc f61b9c0f); polling continues from 66; gate closed (~2.0 GiB short).
- 08:31:40 AEDT — GATE OPEN at poll 77 (df 9,871,284 KiB; +2,027,132 KiB in 5 min). Poll loop stopped for flight launch.
- 08:34:20 AEDT — service-flight LAUNCHED (supervisor PID 32874, QEMU pid 32884; launch df 9,799,544 KiB). Guest 600 s / host 700 s, ends ~08:46 AEDT.
- 08:44:21 AEDT — service-flight ENDED (guest 600.263 s, host 601.02 s, QEMU exit 0; UART 44fdd2ad, 23,946 lines).
- 08:45 AEDT — Step 5 adjudicated. Verdict: FAIL. Trigger fired (boot_completed first guest 563.938 s), marker x2, service NEVER STARTED — init refused: /system/bin/t728probe unlabeled, no domain transition. New wall SERVICE-SELINUX-LABEL-REFUSED (guest). Findings + outcome + death note in worktree root.
