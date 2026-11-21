# T742 DEATH NOTE — first failing link

## Verdict: FAIL

## First failing link

system_server pid 1960 died by SIGKILL at guest ~1230.1 s (service-flight-20, L24670) and no UART line names its killer.

## Wall

SYSTEM_SERVER-SIGKILL-KILLER-UNIDENTIFIED (guest, NEW).

## Evidence

- Full 1500 s flight ran (service-flight-20): guest 1516.766 s, host 1519.31 s, QEMU exit 0. Boot completed (first firing L20510, guest 1167.317 s). Dup-fail regression holds (0 in 349.4 s presenting, rate 0.000, below both baselines).
- system_server died by SIGKILL twice: pid 705 at guest ~841 s (killer NAMED — Watchdog, L11158, main thread blocked 111 s, media.c2/camera waits, the T740 flight-15 pattern) and pid 1960 at guest ~1230.1 s (killer UNIDENTIFIED). Third system_server pid 3285 (guest ~1240 s) alive at flight end.
- Killer-exhaustion for 1960: Watchdog KILLING lines 1 total (pid 705 only); LMK kill-decision lines 0 (all 10 lowmemorykiller lines are startup or conn established/dropped); 'Low on memory' 0, onTrimMemory 0, meminfo 0; init 'Sending signal 9' 27 lines, 0 naming system_server (the zygote kill at guest 1215.263 s demonstrably did not kill 1960 — it logged afterwards); ActivityManager 'Killing ... system_server' 0 (am_kill victims only apps 2588/2620, killed BY 1960); kill-verb grep ("Killing ", whole UART) 2 lines, both AM killing apps. Flood 2.6 lines/s around the death — no flood loss.
- Watchdog KILLING hold (requires 0) also fails: 1 (the 705 kill). All other holds met.
- Flight-19 is VOID (cut on the disk gate at guest 261.054 s). Flight-20 is the evidence flight.
- Analyzer: analyze_t742_probe.py SHA 1ce9f532; regression vs flight-18 reproduced byte-identically by this recovery lane (diff 0). Independent recount reproduces every count. First-boot correction: true first firing L20510 guest 1167.317 s (analyzer reports 1167.684 s — its regex misses the concatenated line).

## Successor direction

T743 must identify the 1960 killer with evidence the UART does not carry: mine the 631 tombstone lines for a watchdog dump covering 1960; capture a dedicated kmsg stream around the death window; add a periodic meminfo dump to test RAM pressure directly (meminfo 0 here — unmeasured, not disproven). Parallel thread: the watchdog kill of 705 (and T740's 788) shares the media.c2/camera lazy-HAL stall root cause — removing that stall tests whether the watchdog kills stop. Do NOT re-diagnose dispatch (T736 PASS) or the dup path (T741 PASS). Do NOT rebuild the kernel or change the vendor image, CPIO, or DTB.

## Files

- findings-t742.md SHA 55cecf6a (18,851 B) — /Users/Shared/wt-track742-sigkill-source/findings-t742.md
- outcome-t742.txt SHA 4f8e9e45 (5 B, "FAIL") — same worktree
- analysis-t742.txt SHA 81881151 (451 lines) — same worktree
- service-flight-20/uart.log SHA 28a47389 (29,547 lines / 3,832,505 B)
- service-flight-19/uart.log SHA f6416a8d (6,475 lines / 870,178 B) — VOID
