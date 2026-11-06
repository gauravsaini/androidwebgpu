# T682 — Composer 2.1 flight: install the bionic vendor ELF, prove a frame commits (Mac)

Verdict: FAIL

- Date: 2026-11-07 (driver certification; lane launched 2026-11-04)
- Worktree: /Users/Shared/wt-track682-composer-2-1-flight, branch t682, from af112eab
- Runtime: Mac opencode 2.1.2 (codex attempt 2026-11-04 16:19 died instantly on usage limit; log preserved as track682-codex.log.wall-20261104-1619; opencode fallback launched same turn per standing fallback)
- Reconstructed by reviewer: this findings file was reconstructed by the driver/reviewer on 2026-11-07 because the agent's own write never landed (agent wedged with zero output). Per RECONSTRUCTED-TAG this reconstruction is itself logged as a process defect.

## What happened

The lane was launched 2026-11-04 ~16:21 AEDT as PID 93981. Its log
/Users/Shared/codex-logs/track682-opencode.log grew to 1,592,922 B by
16:23:48 on 2026-11-04 and then froze permanently. The agent probed ext4
tooling in its first minutes (mke2fs found at /opt/homebrew/bin) and then
produced no further observable output for ~62 hours.

Driver verification (repeated 2026-11-04 through 2026-11-07, final fresh
check 2026-11-07 ~06:46 AEDT, host EktaAir.local):

- PID 93981 alive, SN state, PPID 1, elapsed 02-14:24:59, CPU time
  25:56.70 and slowly advancing — alive outside, dead inside.
- Only child: PID 93983, opencode serve --stdio --port 0 (runtime
  plumbing, no work subprocess).
- Worktree contained zero agent-written files: no findings, no outcome,
  no death note, no fix initramfs, no UART log.
- No QEMU process was ever positively verified during the lane's life.
- Log frozen at 1,592,922 B, mtime Nov 4 16:23:48 2026, at every check.

This is the wedged-lane pattern, distinct from the T680/T681 box-opencode
teardown linger (those lanes filed complete terminal artifacts before
lingering; T682 filed nothing).

## Steps

### Step 0 — Mac disk gate

Not recorded by the agent. At kill time the driver recorded
/System/Volumes/Data at 21 Gi avail / 90% used, above the 8 Gi gate.
Disk was not the wall for this lane: the agent stalled on 2026-11-04
when disk was also above the gate at launch.

### Step 1 — Verify all inputs

Not completed by the agent — the agent stalled before any Step 1 output
was filed. No input verification record exists in the worktree. The
driver-staged guest inputs remain in guest-inputs/ untouched (Image,
minimal-virt-fixed.dtb, system.img, the T681 ELF, base-afb2991f.cpio.gz).
No SHA mismatch is claimed; the step was simply never reported.

### Step 2 — Build the replacement vendor image

Not reached in any filed output. No replacement vendor image exists in
the worktree.

### Step 3 — Build exactly ONE fix initramfs

Not done. No fix initramfs was built. The base remains afb2991f.

### Step 4 — Fly 1200s under QEMU 11.1.2

Not done. No flight ran. No UART log exists for T682.

### Step 5 — Adjudicate frame commit, boot, and regressions

- Frame commit into fb0: NOT proven. No guest-origin frame evidence
  exists because no flight ran. The frame / fb0 question is untested
  by this lane, not failed by the guest.
- sys.boot_completed: not observed (explicit: no flight, no guest run,
  so sys.boot_completed was NOT observed in T682).
- vendor.hwc-2-1 service start, ELF load, sepolicy /dev/mem, simplefb
  stride, and LIBC_R linker risks: none adjudicated — no guest run.
- Regressions T664–T681: not re-run; no image was built, so no
  regression was introduced by this lane.

## First failing link

Agent execution itself: the opencode agent wedged ~2 minutes after
launch, before filing any step output. The first failing link is the
lane/agent layer, not input, vendor-image-build, install, flight,
display, or regression. The mission itself (vendor.img CPIO-member
replacement + flight) remains unexecuted and is carried to T683.

## Disposition

User decision 2026-11-07 06:41 AEDT: Option 1 — kill now + T683 launch.
Driver killed PID 93981 with TERM at 06:45:42 AEDT on 2026-11-07; the
process and its opencode serve child (93983) were verified gone at
06:45:45 AEDT; the log grew by 121 B of terminal teardown sequences
only. A standing timeout rule was locked the same day: a wedged lane
(log frozen >2x expected step time AND elapsed > worst-case estimate)
is fresh-verified once, killed, certified honest FAIL, and its
successor launched the same turn — autonomously, with no options
presentation to the user.

WALL-TYPE-TAG: guest type-1
Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface and vendor.img install route are required regardless of implementation language; the wedged-agent failure itself is a lane-layer defect, recorded here under the mission's type-1 tag because the mission's wall chain is guest type-1.
