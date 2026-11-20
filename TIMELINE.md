# T731 TIMELINE

- 20:43 AEDT 19 Nov 2026 — lane launched (agent spawn). Step 0: df avail 74,431,080 KiB, gate open.
- 20:44 AEDT — Step 1 PASS: all 7 input SHAs match T730.
- 20:45 AEDT — parser positive control: old rc ACCEPT (1 service, 2 actions, 0 imports). New rc written (711 B, 053189d0), parser ACCEPT.
- 20:48 AEDT — box build started (t731-system.img from T729 image, debugfs rm+write new rc).
- 20:49 AEDT — first write attempt FAILED (debugfs write refuses existing file); image unchanged (still 568af6ac). Fix-up: rm + write.
- 20:50 AEDT — rc replaced (dump-back 053189d0), binary label intact, neighbours identical, e2fsck clean. Mode caught at 0640 bad-type, fixed via sif 0100644 -> regular 0644.
- 20:51 AEDT — new image SHA 15c79aad. Patch computed: 259807d8, 3 chunks, 3,145,728 B.
- 20:53 AEDT — Mac-side apply: 3 chunks, t731-system.img SHA 15c79aad, matches box. Marker uniqueness verified (0 in T724-T727 UARTs).
- 20:55 AEDT — Step 2 PASS, Step 3 PASS (scripts adapted, import-test OK, analyzer sanity reproduces T730 driver counts exactly).
- 21:11 AEDT — flight ended on host cap (801.72 s), guest 641.31 s, target 700 s missed.
- 21:14 AEDT — Step 5 done: Verdict FAIL, findings + outcome + death note written. Wrap-up only per user-ordered 22 h Mac pause (21:10 AEDT); no certification, no successor launch.
