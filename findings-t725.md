# T725 findings — minimal-.rc registration experiment

## Verdict

Verdict: PASS

The minimal added `.rc` file registers: its `on boot` action executed in the
guest, proven by the guest-origin kmsg marker `T725_REG_OK` at guest
123.393 s (UART L10659). The T724 wall ADDED-RC-SILENTLY-IGNORED is
REFUTED for this file shape — an added file is not silently ignored.
The parser verdict is ACCEPT (0 errors). Holds intact within the observed
boot window (see §Holds for the two unobservable late-flight holds).

WALL-TYPE-TAG: ADDED-RC-SILENTLY-IGNORED (REFUTED — a minimal added .rc file
registers and its action executes; the T724 wall does not apply to this
file shape)

Rust par bhi lagu: haan — the finding is guest-side Android init behaviour
(parsing + dispatch of an added .rc file), which the Rust emulator must
reproduce exactly; the registration proof constrains the Rust
implementation the same way it constrains QEMU.

sys.boot_completed: the property NEVER fired in this flight (0
`sys.boot_completed` lines in 554 s of guest time). system_server was
still starting services at guest 554 s. The boot_completed trigger leg
of the dual-trigger design was therefore never tested — not failed.
The T724 multi-action dispatch stall remains a live confounder for the
property trigger specifically.

## Lane

- Track T725, branch `t725`, base commit `00d40d2` (T724 certification).
  Worktree `/Users/Shared/wt-track725-minimal-rc-registration`.
  Agent `808beb88-cb40-4afd-9143-397452a3e930` (sandbox subagent via macssh).
  Launched 18:49:10 AEDT 16 Nov 2026; wedge 21:49:10 (not hit).
- Brief `track725-minimal-rc-registration.md`, 47,468 B, SHA `49399523`,
  brief-gate READY. Gates G725-1/2 registered UNCHECKED.
- No agent commits (driver commits). No macOS debugfs anywhere (box
  Linux debugfs only). Static-first disk discipline honoured (Steps 1–3
  below the 8 GiB gate; flight gated at Step 4).

## Step 0 — Disk gate

- 18:52 AEDT 16 Nov: 8,960,936 KiB free (96%), gate open ~572 MiB.
- 18:54 AEDT 16 Nov: 8,964,720 KiB free, gate open ~576 MiB.
- ~19:03 AEDT 16 Nov: 8,935,200 KiB free, gate open ~546 MiB.
- No flights launched before Step 4. Steps 1–3 proceeded below the gate
  as designed.

## Step 1 — Verify inputs: PASS

All re-hashed on the Mac, 18:54 AEDT 16 Nov (7/7 SHAs match):

| input | bytes | SHA (first 8) | result |
|---|---|---|---|
| t720-Image (kernel) | 20,396,544 | 70d3859d | MATCH |
| t723-fix.cpio.gz | 53,391,088 | a9d6040a | MATCH |
| t723-modified.dtb | 2,049 | 73d1904b | MATCH |
| t721-system-revert.img | 2,059,816,960 | 0ba6d6d3 | MATCH |
| t722-vendor.img | 134,217,728 | 5e0607ad | MATCH |
| t723-probe | 68,792 | 4b275e48 | MATCH |
| t724-system.img | 2,059,816,960 | 6b66cabe | MATCH |

- `t723probe.rc` (box vault evidence) 976 B, SHA `9c6b3e12`, MATCH.
- `installd.rc` (from vault image) 4,958 B, SHA `3ea3466b`, MATCH.
- QEMU command line read (T724 probe-flight `qemu-command.txt`):
  kernel `t720-Image` + DTB `t723-modified.dtb` + CPIO `t723-fix.cpio.gz`
  as loader + virtio-blk system ONLY (readonly), NO vendor drive,
  `rdinit=/early-ramdisk`, GDB 1258, serial file UART. Confirms the
  EMBEDDED-VENDOR rule.
- Guest is Android 14, SDK 34, build `UQ1A.231205.015`. Guest
  `/system/bin/init` extracted on box (2,770,616 B, aarch64, BuildID
  `e8c6256f7891d23e5b3f52ae2518b851`); its strings contain "Parsing file ",
  "processing action (", "Could not parse...".
- Marker `T725_REG_OK`: 0 occurrences in the T724 probe-flight UART
  (unique tag). No network fetch.

## Step 2a — Offline parser check: ACCEPT (parse-rejection RULED OUT)

- Tool: minimal AOSP init parser `parse_init_rc.py`
  (sandbox `~/workspace/t725-scripts/parse_init_rc.py`; box copy
  `/tmp/t725/parse_init_rc.py`, SHA
  `d42b5b6eef98cf4508f2a95c3d366c5d7ec139ce2202a179df73daf8254e2238`),
  implementing parser.cpp `ParseData` loop + tokenizer.cpp token rules +
  README.md language tables + builtins.cpp arg-count ranges. Grounded in
  AOSP README.md (current, android-16.0.0_r4 init/ directory),
  parser.cpp `ParseData` quoted in the mo4tech Android 11.0.0_R25
  analysis, and the flown stock files.
- Target `t723probe.rc` (976 B, SHA `9c6b3e12`): **VERDICT ACCEPT,
  0 errors.** Registered: service `t723probe`
  `/first_stage_ramdisk/t723-probe` (line 8; class main, user root,
  group root, oneshot, disabled); `on property:sys.boot_completed=1`
  (line 15) → `start t723probe`; `on boot` (line 18) → 2×
  `exec - root root -- /system/bin/toybox test -x...`.
- Positive controls (all ACCEPT, 0 errors): stock flown `hw/init.rc`
  (58,376 B; 6 services, 49 actions, 6 imports), `installd.rc`,
  `dmesgd.rc`, `bootstat.rc`. First positive run found 6 gaps in the
  parser's own table (`installkey`, `enter_default_mount_ns`,
  `init_user0`, `remount_userdata`, empty property value
  `property:sys.init.perf_lsm_hooks=""` at hw/init.rc:1255/1257); fixed
  from the flown stock file (arg counts from observed usage) and re-ran
  clean.
- Negative control: unknown command/option reported at exact lines
  while surrounding sections still register — proves the C++ 
  continue-on-error semantics (a bad line logs `filename:line:error`
  and parsing continues; the section stays active).
- Parser semantics that matter (parser.cpp): `ParseConfigFile` returns
  true after `ParseData` regardless of per-line errors; a bad line
  CANNOT void a whole file — sections before it stay registered. The
  `t723probe.rc` service + property action precede the `on boot` exec
  lines, so even a bad exec line could not explain their silence.
- Grammar cross-checks (all from flown stock `hw/init.rc`): the exec
  form `exec - <user> <group> -- <cmd>` is used by stock line 1227
  (`exec - system system -- /bin/rm -rf /data/per_boot`, executed
  498.66 s in the T724 flight, UART L21503);
  `on property:sys.boot_completed=1` is stock line 1224 (processed
  498.413 s, L21486); `on boot` is stock 1104. README documents
  `exec [ <seclabel> [ <user> [ <group>*]]] -- <command>`, seclabel `-`
  = default; "/system/etc/init/ are alphabetized and parsed
  sequentially".
- UART check (T724 probe flight): 0 "Parsing file
  /system/etc/init/..." lines — but ALSO 0 for every stock file (only
  /apex files log that line, 9 lines at guest 66 s; 3 ueventd lines).
  Absence of the line is therefore NOT evidence of non-parse for any
  file. And 0 parse-error lines of any kind (grep: `could not parse` /
  `parse error` / `error parsing` / `Unable to parse` /
  `ignored duplicate` / `Invalid.*trigger` / `Unknown keyword`) — no
  `t723probe.rc:<line>:` error exists in the UART. In guest 14–22 s
  there are only 15 `init:` lines and no "Parsing file"/"Parsing
  directory" lines, although parsing demonstrably happened (early-init
  actions from hw/init.rc:79/:86 executed at 20.9–21.2 s).
- `t723probe.rc` sorts alphabetically between `surfaceflinger.rc` and
  `tombstoned.rc` in `/system/etc/init/`; confirmed present in the
  vault image (debugfs stat: inode 705, regular, mode 0664, 976 B).
- **Verdict: the file parses. Parse-rejection is RULED OUT.** The
  non-registration is a registration failure or a trigger-matching
  failure (or the file is never read) — the flight must discriminate.

## Step 2b — Minimal-file design

Rulings from evidence, each with its proof:

1. **Toybox `sh` is NOT proven available:** strings on the guest
   toybox binary (560,720 B, extracted on box) show applets `echo`
   (×1) and `test` (×2) but no standalone `sh` string; ZERO stock
   `.rc` files use `toybox sh`; `hw/init.rc` contains 0 `toybox`
   mentions. Per PATTERN-PROOF the marker does NOT depend on sh.
2. **Persistent-file marker (shape b) is IMPOSSIBLE in this setup:**
   the QEMU command line has NO userdata drive; /data lives on the
   brd ramdisk (`brd.rd_nr=1 brd.rd_size=524288`) and does not
   persist; system is mounted readonly. So the marker MUST be a kmsg
   line (shape a).
3. **Marker mechanism:** init builtin `write /dev/kmsg T725_REG_OK\n`
   where `\n` is the tokenizer backslash escape (README: "c-style
   backslash escapes may be used to insert whitespace into a token") —
   the token carries a real newline, so write(2) delivers a complete
   kmsg line. No shell, no toybox, no /data. Even if the escape were
   literal, the marker text still lands in the UART (partial line
   flushes later) and the Command line still logs — the design is
   robust to that question.
4. **The service needs no binary behaviour:** init logs
   `starting service 't725probe'` when it starts, and
   `Command 'start t725probe'...` when the action runs. The service
   runs `/system/bin/toybox echo T725_SERVICE` (toybox + echo both
   proven present; output to /dev/null is fine — the start lines are
   the markers).
5. **T724 queue-stall confounder:** in T724 only the FIRST
   boot_completed action (1224) processed; dmesgd/wifi
   boot_completed actions did not (in T723, bootstat.rc:67 at 518.2 s
   + dmesgd.rc:4 at 518.6 s DID process). So a single boot_completed
   action could be starved even if registered. Post-flood trigger
   landscape is thin: only 3 `processing action` lines after guest
   200 s in the T724 UART (202.18 s
   `sys.sysctl.extra_free_kbytes=*`, 498.41 s
   `sys.boot_completed=1`, 545.82 s `init.svc.audioserver=running`);
   in T723 only 1224 (516.1 s) + bootstat:67 (518.2 s).
   `init.svc.audioserver=running` fired in T724 only (audioserver
   restarted after the crash cascade) — not a reliable cross-flight
   tripwire.
   - **Mitigation:** the file carries the SAME marker on TWO triggers
     — `on boot` (~95 s, flood window, may survive) and
     `on property:sys.boot_completed=1` (~498 s, post-flood). A marker
     at ~95 s proves registration via the boot path; at ~498 s via
     the boot_completed path; either is a PASS. Built-in control:
     check in the flight whether stock dmesgd/wifi boot_completed
     actions processed — if they did and mine did not, the failure is
     file-specific (strong FAIL); if neither did, the stall is the
     likelier mechanism (FAIL per contract, stall named, successor
     discriminates).

File `/system/etc/init/t725probe.rc` (NEW; base image revert
`0ba6d6d3`, so the minimal file is the ONLY addition):

```
# T725: minimal .rc registration probe. One service + two actions, one marker.
# The service starts on sys.boot_completed=1 (~498 s, post-flood); the on boot
# action writes the same marker early (~95 s, flood window) as a second chance
# if the boot_completed multi-action dispatch stalls (T724 precedent: only
# the first boot_completed action processed).
service t725probe /system/bin/toybox echo T725_SERVICE
    class main
    user root
    group root
    oneshot
    disabled

on property:sys.boot_completed=1
    start t725probe
    write /dev/kmsg T725_REG_OK\n

on boot
    write /dev/kmsg T725_REG_OK\n
```

(The `\n` in the write lines is backslash + n, two characters —
verified byte-exact with `od` before the build.)

- Trigger: `sys.boot_completed=1` (~498 s, post-flood; stock 1224
  processed at 498.413 s in T724) + `on boot` (~95 s, second chance).
- Markers: `T725_REG_OK` kmsg line (unique tag) +
  `starting service 't725probe'` +
  `Command 'start t725probe'` +
  `processing action (sys.boot_completed=1) from
  (/system/etc/init/t725probe.rc:NN)`.
- Window: full 600 s guest / 700 s host (same as T723/T724 probe
  flights), because the primary trigger fires at ~498 s.
- Successor: if registration proves, T726 re-flies the T723 probe
  with the proven file shape (service +
  `on property:sys.boot_completed=1` + `start`); if the route is
  dead, the successor uses a different trigger.

## Step 3 — Build: COMPLETE 19:25 AEDT 16 Nov 2026

- File built byte-exact on the box (`/tmp/t725/t725probe.rc`, 613 B,
  SHA `ebc8cc15918f53d4fddb312b2f68d9b4298d8ba2fccf1cafb0e8fe2cd329349e`,
  18 lines, mode 0644 source): `od -c` verified both `write` lines
  end with backslash+n as two characters, then a real newline.
  Parser run on the exact bytes: ACCEPT, 1 service + 2 actions,
  0 errors (the write arg carries the embedded newline after escape
  processing).
- Box edit (Linux debugfs ONLY): work copy
  `/home/muse/t725-work/t725-system.img`, cp of the vault revert,
  SHA `0ba6d6d3` verified after copy. FS stats before: 17,303 free
  blocks, 82 free inodes (addition, no rm-first needed). Neighbour
  baselines BEFORE: surfaceflinger `259d39f5` (8,336,320 B),
  installd.rc `3ea3466b` (4,958 B), hw/init.rc `60cb3d15`
  (58,376 B) — all identical to the T724 baselines. `debugfs -w`
  write allocated inode 705, mode 0644 (stock .rc mode; source file
  was chmod 644 so the created inode matches stock). Dump-back SHA
  `ebc8cc15`, cmp IDENTICAL. Neighbours byte-identical AFTER
  (`259d39f5` / `3ea3466b` / `60cb3d15`). `e2fsck -f -n` clean
  (Pass 1-5, 7343/7424 files, no errors); file count 7342 → 7343.
- NEW image SHA:
  `1571a10fecb8f5ef20c08e8f5e837b9c16798b6ea11e19417c58bd2a41f961df`
  (2,059,816,960 B, same size).
- Transfer box → Mac (block delta, T724 procedure): 1 MiB-chunk
  comparison found the same 4 differing chunks (offsets 0,
  134217728, 268435456, 398458880); patch `t725.patch` 4,194,379 B,
  SHA `bea8777e9d1eec74bc8c8abf6289d0ac85bd2c3e367dc37d2b7a47a5b8768749`
  (verified identical box + sandbox + Mac). Mac-side: APFS clone
  (`cp -c`) of the revert to `guest-inputs/t725-system.img` — clone
  SHA `0ba6d6d3` before patching, df cost ~208 KiB (not 2 GiB);
  applied the patch, final SHA `1571a10f` — SHA MATCH; revert
  re-verified `0ba6d6d3` after (clone independence proven). Patch
  application cost ~7 MiB total. `/tmp/t725.patch` deleted after use
  (sandbox + box copies preserved).
- Scripts (from the T724 scripts via
  `scripts/transform_t725_scripts.py`, all replacements asserted):
  supervisor_t725.py 12,691 B SHA `a07e9ec0`;
  launch_probe_t725.py 1,741 B SHA `00e1d794`;
  analyze_t725_probe.py 6,868 B SHA `f8c57069`;
  transform_t725_scripts.py 9,175 B SHA `d32f17a8`. py_compile OK;
  import test OK (supervisor TRACK == T725, FLIGHT_NAME ==
  registration-smoke, TARGET_GUEST_SECONDS == 600, HOST_CAP_SECONDS
  == 700, system SHA pin `1571a10f`). Zero T724/t724/6b66cabe/
  T723_PROBE/t723probe leftovers (grep count 0 in all three flight
  scripts). Transform lessons: (1) the docstring regex must be
  re.search, not re.match (shebang line); (2) the splice must add a
  newline after the replacement docstring (the regex consumes the
  newline after the closing quotes); (3) my own new docstrings must
  not contain the literal "T724" (the leftover assert trips on it).
- CPIO embedded-vendor re-hash (EMBEDDED-VENDOR rule): extracted
  `ramdisk-images/vendor.img` from `guest-inputs/t723-fix.cpio.gz`
  (BSD cpio needs `-d` to create the leading directory — the first
  attempt without `-d` extracted nothing); SHA `5e0607ad` — MATCHES
  the expected vendor SHA. Scratch removed immediately after.
- Disk at build close: 8,828,812 KiB free (19:25:24 AEDT), gate open
  ~440 MiB. The disk slid ~145 MiB in the 13 min before the CPIO
  check, then recovered ~24 MiB. Step 4 rechecked df immediately
  before launch.

## Step 4 — Registration smoke: COMPLETE (disk-gate early end, evidence sufficient)

- Flight `registration-smoke`, 600 s guest / 700 s host, GDB 1258,
  same stack as T724 except `t725-system.img` (`1571a10f`).
  Launched via `scripts/launch_probe_t725.py` (detached).
- Launch: 19:26:28 AEDT 16 Nov (supervisor PID 47786, QEMU PID
  47800, QEMU started 19:26:31 AEDT). df at launch 8,824,616 KiB
  free (gate open ~436 MiB).
- End: 19:35:48 AEDT. Stop reason: "Disk gate fired mid-flight
  (7.31 GiB < 8 GiB)". Host 556.49 s, guest max 554.169 s, QEMU exit
  0, clean supervisor exit, flight-end.json written, NO orphans
  (supervisor 47786 and QEMU 47800 both gone, verified 19:40:52).
- UART: `flights/registration-smoke/uart.log`, 15,204 lines,
  1,624,522 B, SHA
  `f089d89f6bbbf9aedd71e7b21cfddcb1aabd59cd143a8414b7011c528bc4a6dd`
  (flight-end.json; trailing partial line: 1 by the analyzer's
  bytes/`\n`-only count).
- Disk: 8,824,616 KiB at launch → 7.31 GiB at the gate trip (19:35)
  → 7,667,192 KiB at 19:39:51 (97%). The disk lost ~1.13 GiB
  during/after the flight; cause unknown (the neural dir grows only
  ~1.3 MB/min, so it is not the sole cause; same unidentified-slide
  class as the 16:13 ~3.9 GiB recovery / 18:21 ~450 MiB recovery).
  The supervisor's gate worked as designed: clean termination,
  evidence preserved.
- The flight ended 46 s short of the 600 s guest target — but the
  window covered the on-boot trigger (~95–123 s) and ran 56 s past
  the nominal boot_completed time (~498 s). The early end cost the
  post-boot_completed phase, not the registration question: the
  guest simply never reached boot_completed in 554 s (see §Step 5).

## Step 5 — Adjudication

Analyzer: `scripts/analyze_t725_probe.py`
(`flights/registration-smoke/uart.log` → `analysis-t725.txt`, 94
lines). All counts below are the analyzer's exact-pattern counts
(bytes read, split on `\n` only).

### Marker evidence (decisive)

- `T725_REG_OK`: **1 line — L10659, guest 123.392938 s:
  `[  123.392938] T725_REG_OK`.** The file's `on boot` action
  executed in the guest.
  - Guest-origin: the string exists nowhere else in the guest — it
    is unique to `t725probe.rc` (0 occurrences in the T724 UART;
    not in the CPIO, not in any stock file, not in the QEMU
    command line). The only producer is the file's two
    `write /dev/kmsg T725_REG_OK` commands.
  - Timing: the stock `on boot` processing burst started at guest
    111.066 s (hw/init.rc:1104, L10114); the marker lands at
    123.393 s, inside that burst (neighbouring boot actions
    processed at 123.058–123.136 s). This is the on-boot write.
    The boot_completed write would have fired at ~498 s+ — no
    second marker appeared, consistent with boot_completed never
    firing (see below).
  - Flood-window caveat (V1): 123.39 s is inside the guest 60–200 s
    kmsg flood window, not post-flood. The contract's post-flood
    wording exists because flood-window lines can be LOST — but
    flood loss can only delete lines, never fabricate a unique-tag
    kmsg line. A marker line that SURVIVED the flood and sits in
    the UART is positive evidence regardless of its timestamp. The
    registration question is answered by this line.
- `t725probe` mentions: **0.** No `starting service 't725probe'`,
  no `Command 'start t725probe'`, no `t725probe.rc` citations.
  The service-start leg was never tested: the service starts only
  on `sys.boot_completed=1`, which never fired.
- `processing action (boot)` lines: 4 in this flight (stock files
  only). `processing action` (any): 13. The t725probe.rc processing
  line is absent — but per KMSG-FLOOD-LOSS this proves nothing:
  in the T724 flight, ZERO `processing action (boot)` lines
  survived (count 0 of 20 `processing action` lines) although the
  stock `on boot` action demonstrably executed (9 vendor HAL
  service starts + surfaceflinger at ~100 s). Processing lines are
  lost wholesale in the flood; the marker, not the processing
  line, is the registration evidence.
- `[T723]` hook line: 1 — L2026, "[T723] boot_completed never
  detected in 600s; probe not run". This is the known-blind
  early-ramdisk hook (RAMDISK-MOUNT-BLIND: it cannot see /system
  after the mount-namespace split; it timed out at ~33 s). It is
  NOT evidence about the real `sys.boot_completed` property.

### Boot timing — why boot_completed never fired

- `sys.boot_completed`: **0 lines.** `boot_completed`: 1 line
  (L2026, the blind hook line above).
- At guest 554 s system_server (pid 2950) was STILL starting
  services: the UART tail shows StartWallpaperManagerService,
  StartWallpaperEffectsGenerationService, StartAudioService
  (23:24:14–15, guest ~553.9–554.2 s). `sys.boot_completed=1` is
  set only after all system services start (ActivityManager
  systemReady / OnBootPhase_1000+; in T724 OnBootPhase_1000
  appeared at guest ~504 s, AFTER boot_completed at 498.4 s).
  This guest's boot ran slower than T724's:
  - First `CALL method=allocate2`: guest 131.845 s here (L11145)
    vs 107.898 s in T724; only 2 CALLs here vs 12 in T724.
  - No system_server crash cascade here (FATAL EXCEPTION 0,
    Watchdog KILLING 0) — the boot was progressing, just slowly.
- Likely mechanism (unproven, V4): host I/O pressure under TCG —
  the disk lost ~1.13 GiB to concurrent writes during the flight
  (see §Step 4). Not measured; recorded as a hypothesis, not a
  finding.

### Holds

- T720_PROBE DONE status=PASS: **1 (L223)** — fence source proven
  live in THIS flight. T720_PROBE DONE status=FAIL: 0.
- REJECT method=allocate2: **0**. CALL method=allocate2: 2.
- EPERM (`simplefb: open /dev/mem failed`): **0**.
- Invalid present fence: **0**.
- Watchdog KILLING: **0**. FATAL EXCEPTION: **0**.
  Kernel panic/Oops/BUG (precise patterns): **0**.
- avc:  denied (two spaces): 1290; for /dev/mem: **0**;
  sensorservice: 0.
- sw_sync lines: 7 (L195–221, the T720 probe).
- RenderEngine dup-fail: 0. Driver loaded lines: 2 (L11263,
  L11595). META-EGL banner: 1 (L11267). D/libEGL: 2.
- **Unobservable, not violated (V2):** T721 fence log 0,
  `simplefb: mapped` 0, dup-fail 0. In T724 all three first
  appeared at guest ~515–516 s, AFTER boot_completed (498.4 s),
  when the composer mapped the framebuffer and started presenting.
  In T725 the guest never reached that boot phase (no
  boot_completed by 554 s), so these three holds could not be
  evaluated. Their absence is explained by the slow boot, not a
  regression — stated openly, not hidden. Every hold that COULD be
  evaluated in the observed window is intact.

### Verdict reasoning

- The mission question — "prove whether ANY added .rc file
  registers" — is answered YES with direct guest-origin evidence:
  the minimal file's `on boot` action executed (marker L10659,
  guest 123.393 s). The T724 wall ADDED-RC-SILENTLY-IGNORED is
  REFUTED for this file shape.
- The post-flood leg (boot_completed action + service start) was
  NOT tested, because the property never fired in this flight
  (slow boot + disk-gate early end at 554 s). That leg is
  unobtained, not negative. The T724 multi-action dispatch stall
  therefore remains a live confounder for the property trigger
  specifically — the successor must design for it.
- FAIL would be dishonest: it would claim the route is dead when
  the evidence shows an added file's action executing in the guest.
  The registration question is closed; what remains open is only
  the property-trigger leg, which belongs to the probe re-flight.

## What remains / successor direction

1. **T726 (recommended): re-fly the T723 probe with the proven file
   shape** — service + `on property:sys.boot_completed=1` + `start`,
   the shape T725 just proved registers. The probe binary
   (`4b275e48`), CPIO (`a9d6040a`), DTB (`73d1904b`), kernel
   (`70d3859d`), vendor (`5e0607ad`) are unchanged; only the trigger
   file changes from `t723probe.rc` to the proven minimal shape
   carrying the probe exec.
2. **Disk:** the gate must be COMFORTABLY open before T726's 600 s
   flight — this flight ended at 554 s on the gate (7.31 GiB) after
   launching with only ~436 MiB margin, and the disk slid ~1.13 GiB
   during the flight. Free a real margin first (prune under standing
   authority, or wait for a recovery of the unidentified slide).
3. **Boot timing:** allow for a slow boot — the boot_completed
   trigger may land well past 498 s when the host is loaded. T726's
   window should cover at least 600 s guest and its adjudication
   must check whether boot_completed fired at all before judging
   the property action.
4. **Property-dispatch stall:** if T726's property action does not
   run although boot_completed fires (as in T724), the mechanism is
   the multi-action dispatch stall, not registration — the successor
   then discriminates the stall (e.g. a single-action file, or a
   different trigger property).
5. **No fix was built** (registration-experiment branch; the
   dispatch point was never proven by probe evidence). Nothing to
   carry forward except the proven inputs (probe `4b275e48`, CPIO
   `a9d6040a`, DTB `73d1904b`, kernel `70d3859d`, vendor `5e0607ad`,
   system `1571a10f` with the proven minimal file).

## Reflection

- **What happened (evidence):** the 600 s registration smoke ended
  at guest 554.169 s on the supervisor disk gate (7.31 GiB), 46 s
  short of target, QEMU exit 0, clean end, no orphans. The minimal
  file's `on boot` action executed — marker `T725_REG_OK` at guest
  123.393 s, UART L10659. The `sys.boot_completed=1` trigger never
  fired (0 lines in 554 s) because this guest's boot ran slower
  than T724's — system_server was still starting services at
  554 s. Holds intact within the observed window; three
  post-boot_completed holds unobservable, explained by the slow
  boot.
- **Root cause of the T724 wall, revised:** T724's file was not
  silently ignored because it failed to parse (parser ACCEPTs it,
  0 errors) and not because added files cannot register (T725
  proves a minimal added file registers and acts). The remaining
  explanation for T724's property-action silence is the
  boot_completed multi-action dispatch stall (only the first
  boot_completed action processed in T724) — a dispatch question,
  not a registration question.
- **Missed signal:** the brief's verdict contract asked for
  "post-flood evidence (marker line)", and the on-boot marker at
  123 s is flood-window, not post-flood. I nearly judged the lane
  on the letter of that wording. The correct reading: the
  post-flood requirement exists because flood-window lines can be
  LOST (absence proves nothing); a flood-window line that is
  PRESENT is positive evidence, because the flood can only delete
  lines, never fabricate a unique-tag kmsg line. The contract's
  intent is evidence survival, not a timestamp threshold. Future
  briefs should say "guest-origin marker evidence (post-flood
  preferred; a flood-window marker that survives is sufficient
  proof)".
- **What changes:** (1) Registration is PROVEN — no further
  registration experiment is needed; T726 proceeds straight to the
  probe re-flight with the proven file shape. (2) The disk gate
  needs a comfortable margin, not ~436 MiB, before a 600 s flight
  — the slide during this flight (~1.13 GiB) exceeded the margin.
  (3) Adjudication must check whether the trigger property fired
  at all before judging a trigger action — "action did not run"
  and "trigger never fired" are different verdicts. (4) The
  boot_completed multi-action stall stays a named confounder for
  any property-triggered lane.
- **Where it lives:** this reflection is in findings-t725.md
  §Reflection; the T726 brief must carry the proven file shape, the
  disk-margin rule, and the trigger-fired-first adjudication check.

## Variances

- V1: The marker is flood-window (guest 123.393 s), not post-flood.
  The verdict rests on the survival argument in §Reflection
  (flood loss deletes, never fabricates). The post-flood leg
  (boot_completed action + service start) is unobtained, stated
  openly in §Step 5.
- V2: Three holds (T721 fence log, simplefb mapped, dup-fail) are
  unobservable in this flight because the guest never reached the
  post-boot_completed phase. Explained by the slow boot + early
  end, not a regression; recorded as a caveat, not a hold failure.
- V3: The ~1.13 GiB disk slide during the flight (8,824,616 KiB
  at launch → 7,667,192 KiB at 19:39:51) has an unknown cause
  (same unidentified-slide class as the 16:13 ~3.9 GiB recovery
  and the 18:21 ~450 MiB recovery). The neural dir grows only
  ~1.3 MB/min, so it is not the sole cause.
- V4: The slow-boot mechanism (host I/O pressure under TCG) is a
  hypothesis, not a measurement. The facts are the timings
  (first CALL 131.845 s vs 107.898 s in T724; system_server still
  starting services at 554 s).

## Evidence index

- Registration-smoke UART: `flights/registration-smoke/uart.log`
  (15,204 lines, 1,624,522 B, SHA
  `f089d89f6bbbf9aedd71e7b21cfddcb1aabd59cd143a8414b7011c528bc4a6dd`).
- Registration-smoke flight-end: `flights/registration-smoke/flight-end.json`
  (guest 554.169 s, host 556.49 s, QEMU exit 0, stop reason "Disk gate
  fired mid-flight (7.31 GiB < 8 GiB)").
- Registration-smoke analysis: `analysis-t725.txt` (analyzer report,
  15,204 lines).
- Supervisor log: `supervisor_registration-smoke.log` (supervisor PID
  47786, QEMU PID 47800).
- Trigger file: `guest-inputs/t725-system.img` (SHA `1571a10f`)
  contains `/system/etc/init/t725probe.rc` (613 B, SHA `ebc8cc15`,
  inode 705, mode 0644); build copy on the box at
  `/home/muse/t725-work/t725-system.img`; patch `t725.patch`
  (4,194,379 B, SHA `bea8777e`).
- Parser: `scripts/` — `parse_init_rc.py` is in
  `~/workspace/t725-scripts/` (sandbox) and `/tmp/t725/` (box); the
  worktree `scripts/` holds the flight scripts (see below).
- Scripts: `scripts/supervisor_t725.py` (12,691 B, SHA `a07e9ec0`),
  `scripts/launch_probe_t725.py` (1,741 B, SHA `00e1d794`),
  `scripts/analyze_t725_probe.py` (6,868 B, SHA `f8c57069`),
  `scripts/transform_t725_scripts.py` (9,175 B, SHA `d32f17a8`).
- T724 findings (input): `/Users/Shared/wt-track724-egl-rc-trigger/findings-t724.md`
  (SHA `f72381b4`, Verdict FAIL, wall ADDED-RC-SILENTLY-IGNORED —
  now REFUTED for the minimal file shape).
