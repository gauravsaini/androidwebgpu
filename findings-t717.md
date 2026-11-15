# T717 findings — 0x0 buffer-allocation wall: T628 AIDL shim parser fix

WALL-TYPE-TAG: guest
Rust par bhi lagu: haan

Verdict: FAIL

## 1. Mission and outcome summary

Mission (brief track717-alloc-zero.md, SHA fb823116): diagnose the 0x0
buffer-allocation wall — which display attribute the T681 simplefb composer
backend refuses in getDisplayAttribute, plus the size-propagation trace —
and apply the smallest fix yielding a nonzero allocation, then fly for
nonzero allocation + queueBuffer > 0 + first present dispatch. Base: the
T716 stack. Constraints honoured: SYS_RAWIO fix not regressed, installd
trigger untouched, no agent process on the Mac (sandbox subagent driving
via macssh only), no macOS debugfs image patching, no commits.

Outcome: the brief's premise was overturned (the composer attribute
refusal is tolerated; the wall was the T628 AIDL shim's descriptor
parser). Four parser versions were built and smoked. V4 PROVED the
parser wall broken: smoke #4 has REJECT 0 with the shim exercised and
the allocate2 call logged with the correct parsed values. But allocation
still fails: the shim's write_result reply path fails the transaction
with -129 (EX_TRANSACTION_FAILED), with no crash and no diagnostic. The
smoke criterion was therefore not met, no flight-01 was flown, and the
mission criteria (nonzero allocation, queueBuffer > 0, first present
dispatch) remain open. The exact first failing link and the precise next
fix are in section 9. All T716 holds stand.

## 2. Step 2 diagnosis — the brief's premise is overturned

### 2.1 The composer attribute refusal is tolerated, not the wall

T716 flight-01 UART L12061:

    DisplayDeviceInfo{"Built-in Screen": 1280 x 720, modeId 1, ... fps=60.0,
    density 160, 160.0 x 160.0 dpi}

SurfaceFlinger learned WIDTH/HEIGHT/VSYNC_PERIOD/DPI despite one attribute
failure per system_server lifetime (8 total, first L10878).

The composer ELF (8d62a642, 19,942,712 B, debug_info present, not stripped)
names source /home/muse/t681-link/t681-service.cpp, which exists on the
box. `getDisplayAttribute` (0x58b14, source lines 239-271): display != 0
gives BAD_DISPLAY (2); config != 0 gives BAD_CONFIG (1); switch on
WIDTH/HEIGHT/VSYNC_PERIOD/DPI_X/DPI_Y, default gives UNSUPPORTED (8). The
value table at file offset 104836 is [1280, 720, 16666667, 160000, 160000,
1851864910] — the 6th value is garbage (ASCII "plan", past the 5-entry
table). The failing query is therefore attribute 0 or >= 6. Other
tolerated failures in the same UART: `getDisplayConnectionType failed` 8x,
`getSupportedContentTypes failed` 44x, HDR-conversion-capability failure 8x.
The display forms correctly from attributes 1-5; the attribute failure is
in the same tolerated class. Inference, recorded as such.

### 2.2 The allocator rejects everything with EINVAL at real sizes

T716 flight-01 UART has 71 `GraphicBufferAllocator: Failed to allocate`
lines, ALL error 22 (EINVAL), at real sizes: `1280 x 720` format 1 usage
b33/1b00, `1280 x 720` format 22 usage 900, `220 x 4` format 1 usage 303,
`1280 x 156` format 22 usage 900. Load-bearing pair:

    L10890 Failed to allocate (1280 x 720) layerCount 1 format 1 usage 1b00: 22
    L10891 BufferQueueProducer: [FramebufferSurface] allocateBuffers: failed
           to allocate buffer (0 x 0, format 1, usage 0x300)

The 0x0 is the queue's logged parameters; the allocator was asked for
1280x720 and refused. Zero allocator lines carry 0x0 sizes — the allocator
fails on every nonzero request too, so it is an independent wall, not a
consequence of the 0x0 queue size.

Producer 0x0 failures: 42 total — 34 x format 22 usage 0 (BLAST consumers:
FallbackHome/Taskbar/QuickstepLauncher) + 8 x format 1 usage 0x300
(FramebufferSurface). The other 29 GBA failures are direct allocations
(RenderEngine/SF internals) with no producer line.

### 2.3 The HIDL SimpleFB allocator is healthy but never called

Service `android.hardware.graphics.allocator@2.0-service` ("SimpleFB
Graphics Allocator 2.0", 834,032 B, SHA 91d84fb9), pid 2416: starts L9561,
initializes L9562, `Successfully mapped /dev/fb0 (16777216 bytes...)`
L9563, registers with hwservicemanager L9677, "successfully registered"
L9679. Exactly 10 lines from pid 2416 in the entire 2400 s flight, all
within the first second of startup; zero `allocate()` log lines; no
death/restart lines. Alive but completely idle — no allocation request
ever reaches it. HIDL-only (no AIDL symbols). `SimpleFBAllocator::allocate`
at 0x34504; `allocateBufferInternal` at 0x34188; `initialize` at 0x34038.
Its source is gone (no match for "SimpleFB Graphics Allocator" on the box).

`init.virt.rc` stanza: `service vendor.allocator-2-0`, interface
`android.hardware.graphics.allocator@2.0::IAllocator default`, class hal
animation, user root, group root graphics drmrpc, capabilities SYS_NICE,
seclabel `hal_graphics_allocator_default`, stdio_to_kmsg; started on
early-boot and boot. The VINTF manifest declares BOTH the HIDL allocator
2.0 (hwbinder) AND the AIDL allocator v2, plus the HIDL composer 2.1.

### 2.4 The T628 AIDL shim is the wall — 71/71 REJECTs

Binary `/vendor/bin/hw/android.hardware.graphics.allocator-service.t628`
(1,711,816 B, SHA 745d19d0): NDK r29, debug_info, not stripped, source
file `allocator_service_t628.cpp` (source NOT on the box). Raw NDK binder
service: `main` 0x111e0, `on_transact` 0x11428, `write_result` 0x11a58,
`read_size_prefixed_descriptor` 0x11f3c, `guest_log_method` 0x1198c. ZERO
HIDL client symbols — it never forwards to the HIDL allocator. Runs as pid
2522; logs ONLY to /dev/kmsg (`<6>T628_ALLOCATOR` prefix, 18-byte header).

UART counts (T716 flight-01): 106 `T628_ALLOCATOR` lines = 71 `REJECT
method=allocate2 malformed-or-unsupported-descriptor` + 34 `CALL
method=getIMapperLibrarySuffix` + 1 `REGISTERED
name=android.hardware.graphics.allocator.IAllocator/default version=2`
(L10410). 71 REJECTs == 71 GBA EINVALs, 1:1. First REJECT at L10406
([104.655s]), before the first attribute failure (L10878) and first GBA
failure line (L10890). `T637_MAPPER` 34 lines (mapper suffix queries
succeed).

Transaction codes (from `on_transact` disassembly): 1 = `allocate` v1
(dead path, hardcoded 1024x768 format 1, never called); 2 = `allocate2`
(parser, then count < 17, then `write_result`; reject path writes the
59-byte REJECT string + AStatus error 22); 3 = descriptor-to-bool
diagnostic (dead path); 4 = `getIMapperLibrarySuffix` (returns "default");
0xFFFFFFFE = interface version 2; 0xFFFFFFFF = interface hash.

Starter: `/etc/init/android.hardware.graphics.allocator-service.t628.rc`
(275 B, inode 732): `service vendor.allocator-aidl ...`, `interface aidl
android.hardware.graphics.allocator.IAllocator/default`, class hal, user
system, group graphics drmrpc, capabilities SYS_NICE, `disabled`,
`oneshot` — lazy start via servicemanager. Replacing the binary at the
same path is sufficient; no .rc change needed.

### 2.5 The parser bug, exactly (disassembly of 0x11f3c-0x120f0)

`read_size_prefixed_descriptor(parcel, *out1, *out2, *out3, *out4)`:

1. Reads the int32 size prefix; sanity check passes only for 4 <= size <=
   0x100000, plus an int32-overflow guard (both fine).
2. Calls `AParcel_readByteArray` at the field position — interprets `width`
   as a byte count. For a real descriptor this fails (parcel too short) or
   over-consumes; the following position check (`consumed >= size`) can
   never pass for the real format. This read is simply wrong for the AIDL
   parcelable encoding.
3. Then reads 4xint32 + 2xint64. The real AIDL `AllocationDescriptor` is
   [size][width:int32][height:int32][format:int32][usage:int64] (size 20)
   — 3xint32 + 1xint64. The parser's field model matches no real format.
4. Skips with `setDataPosition(start + size)` — off by 4 (correct is
   `start + 4 + size`, since the size field excludes its own 4 bytes).
5. Validates out1 in [1,4096], out2 in [1,4096], out3 in [1,64], out4 in
   {1,2,4,5} via bitmask `0x36 >> out4`. In the real layout the 4th int32
   position holds usage-lo, which fails the <= 5 check anyway; and format
   22 is rejected by the out4 check.

Net: the parser returns false for every descriptor the framework sends.
Verified by reasoning over all observed inputs (1280x720, 220x4, 0x0 all
fail at the byte-array read or a later read/validation).

CORRECTION (recorded after smokes #1 and #2, from the disassembly of the
guest's own allocator-V2-ndk.so — see section 3.2): points 2, 3 and 4 of
the diagnosis above were WRONG. The real parcelable is
`BufferDescriptorInfo`, whose wire layout is [size][128-byte array]
[4xint32][2xint64][parcelable-array] — so the original parser's byte-array
read and its 4xint32+2xint64 field model MATCHED the real layout, and its
`start + size` skip was correct under the include-header size convention.
The original parser's ONLY fatal bug was reading the null flag (1) as the
size and failing its `4 <= size` check, before ever reaching the field
reads. The Step 2 text above is preserved as written; this correction
supersedes its mechanism claims.

### 2.6 The success path (write_result) is real and complete

`write_result` (0x11a58): count <= 16 check; `pixels = out2 x out1`;
validation via bitmask `0x1b` (out4 in {1,2,4,5}); bpp table at file
offset 0x1a70: [4, 4, 0, 2, 4] (index 0 fmt 1 RGBA_8888 = 4, index 1 fmt 2
RGBX_8888 = 4, index 2 fmt 3 = 0/invalid, index 3 fmt 4 RGB_565 = 2,
index 4 fmt 5 BGRA_8888 = 4) — so out4 IS the PixelFormat value;
`syscall(279, "t628-gralloc", 1)` = `memfd_create`;
`ftruncate(fd, bpp x pixels)`; builds `vector<NativeHandle>`; serializes
via imported `AllocationResult::writeToParcel`. Only the parser is broken;
everything downstream already works and must stay byte-identical.

Format 22 = HAL_PIXEL_FORMAT_IMPLEMENTATION_DEFINED (BLAST consumers).
Standard gralloc behaviour: map it to RGBA_8888 (code 1, bpp 4). The fix
maps 22 to 1 and accepts {1,2,4,5} as their own codes; all other formats
still reject.

## 3. The fix — binary patch of the parser in place (T708 ELF route)

Decision: patch ONLY the parser function in the existing binary, in place.
A source rewrite was rejected: the shim source is gone, AIDL NDK headers
would have to be generated (aidl compiler exists at /usr/bin/aidl and .aidl
snapshots exist under /home/muse/t680-toolchain/aosp/hwinterfaces/, but the
link/header path is risky), and rewriting `write_result` would risk the
exact NativeHandle layout the T637 mapper expects. The binary patch
preserves `write_result`, all transaction handlers, the hash, and all
strings byte-identically. (An earlier progress note mentioning a clean-room
rewrite captured a superseded plan; the binary patch is what was built.)

Footprint: [vaddr 0x11f3c, 0x120f0) = 436 bytes. `.text` has Address
0x110e0 but file offset 0x00d0e0 — file offset = vaddr - 0x4000, so the
splice point is file offset 0xDF3C (verified by asserting the original
first/last instruction bytes). `.rodata` Address == Off (0x19e0) — do not
assume that holds for `.text`.

PLT addresses (verified from disassembly + PLT dump):
`AParcel_getDataPosition` 0x34aa0, `AParcel_readInt32` 0x34a30,
`AParcel_readInt64` 0x34ab0, `AParcel_setDataPosition` 0x34ac0.

New parser (parser.s, 4,700 B): parses
[size][width][height][format][usage:int64]; requires 20 <= size <=
0x100000; start >= 0 and `end = start+4+size` fits int32 (via `lsr x2, x0,
#31` check, no literal pool); reads width to *out1, height to *out2,
format to *out3, usage to scratch (discarded); `setDataPosition(end)`;
width/height in [1,4096]; format 22 to code 1, {1,2,4,5} to themselves,
else fail; stores code to *out4; returns 1/0. Same signature and return
convention as the original. No BTI/PAC prologue (matches the original
local-function convention; called via direct `bl`).

Build (build_patch.sh): `aarch64-linux-gnu-as` + `ld -Ttext=0x11f3c -e
read_size_prefixed_descriptor` + `objcopy --only-section=.text` to
parser.bin, 336 B (<= 436). Disassembly reviewed instruction by
instruction: all 7 `bl` targets resolve to the exact PLT addresses; `uxtw`
assembled to `mov w` (correct zero-extend); validation constants correct
(20, 0x100000, 4095, 22, 1/2/4/5).

Splice (splice_patch.py): asserts original first instruction bytes
(`ff4302d1` = `sub sp, sp, #0x90`) and last footprint instruction
(`dfffff17` = `b` at 0x120ec) before writing; NOP-pads the remainder;
asserts all diffs are inside the footprint. The assertion caught a
transcription error on the first run (`ff4312d1` written, actual
`ff4302d1`) before any write — fixed, re-transferred, splice succeeded.

Fixed binary: SHA 697fde0135498de171f6f41244e654f2cd593b0755ac679768660b5e95da4b90,
1,711,816 B (same size), 375 bytes differ, all inside the footprint.
Verified: ELF parses; patched parser disassembles correctly at 0x11f3c;
NOP pad after `ret` at 0x1208c; next function (`__throw_length_error` at
0x120f0) intact; `write_result` memfd site intact; `on_transact` /
`write_result` region disassembly identical apart from the objdump filename
header line.

Original binary metadata (box debugfs): inode 731, Mode 0755, uid 0, gid
2000, size 1,711,816, extents (0-417):24410-24827, xattr
`security.selinux` = `u:object_r:hal_graphics_allocator_default_exec:s0`
(49 bytes incl. NUL).

### 3.1 Parser v2 (the flown fix) — v1 failed smoke #1, root-caused, replaced

Smoke #1 (v1 parser) still REJECTed with the exact T716 signature (section
5). Re-diagnosis from aidl-generated NDK code (box `aidl`, minimal
parcelable with the same field shape) plus the `on_transact` disassembly
proved the wire format the v1 parser had wrong:

    [null-flag:int32 = 1][size:int32][width][height][format][usage:int64][count:int32]

The generated `writeToParcel` writes the null flag FIRST, then a size
placeholder patched to `end_pos - start_pos` where `start_pos` is the
position of the size field — so the size INCLUDES its own 4-byte header
(24 for the standard descriptor) — and the generated `readFromParcel`
skips to `start_pos + size`. `on_transact` reads NOTHING before calling
the parser (code-2 path, 0x11654: straight to `bl read_size_prefixed_descriptor`),
then reads count immediately after the parser returns. V1's two bugs: (1)
it read the null flag (1) as the size and failed its size check — the
ORIGINAL parser died the same way (1 < 4) before ever reaching its
byte-array bug; (2) it skipped to start+4+size, overshooting by 4 under
the include-header convention (the original's start+size skip was the one
correct part of its logic; the Step 2 "off by 4" diagnosis was backwards).

V2 (parser.s, 6,203 B): dual-mode first word — v1 == 1 means the null flag
is present (read the size next); 24 <= v1 <= 0x100000 means v1 IS the size
(flag-less sender); anything else fails. Then 24 <= size <= 0x100000, the
same status-checked field reads, skip to start+size (no +4), the same
validation and format mapping. 396 B (<= 436). Disassembly reviewed
instruction by instruction: all 9 `bl` targets resolve to the exact PLT
addresses (getDataPosition x2, readInt32 x5, readInt64, setDataPosition);
no +4 in the end computation; constants correct (1, 24, 0x100000, 4095,
22, 1/2/4/5).

Fixed v2 binary: SHA fc8cb90f0ec9fc7a06804f75e49e611fc829d73a74e72642daee4295e33e270d,
1,711,816 B (same size), 390 bytes differ, all inside the footprint
(1-based file offsets 57150..57584). Pre-parser region (main/on_transact/
write_result/guest_log) and post-parser region disassembly identical to
the original. The v1 fixed binary (697fde01, 375 bytes) is superseded; its
source (parser-v1.s) and image are preserved.

### 3.2 Parser v3 (the flown fix) — v2 failed smoke #2, ground truth from the framework library

Smoke #2 (v2 parser) still REJECTed with the exact T716 signature (section
5b). The interface-token lead pursued after smoke #2 was REFUTED by
disassembling the guest's own `android.hardware.graphics.allocator-V2-ndk.so`
(136,432 B, stripped, Android 34, BuildID 3b4a319f68bd9cd626f180f7097a6f23)
from the T716 system image: `BpAllocator::allocate2` (0x11834) writes NO
interface-token string — it writes int32(1), calls
`BufferDescriptorInfo::writeToParcel`, writes the count, and transacts
with code 2. The same disassembly proved the TRUE wire format, from both
the writer and the reader:

    [null-flag:int32 = 1][size:int32][array-len:int32 = 128][128 bytes]
    [width:int32][height:int32][layerCount:int32][format:int32]
    [usage:int64][int64][parcelable-array][count:int32]

`BufferDescriptorInfo::writeToParcel` (0x10864): size placeholder, then
`AParcel_writeByteArray(parcel, this, 128)`, 4x writeInt32 from struct
offsets 128/132/136/140, 2x writeInt64 from offsets 144/152,
`AParcel_writeParcelableArray` for the vector at offset 160, then patches
size = end_pos - start_pos (size INCLUDES its own header).
`BufferDescriptorInfo::readFromParcel` (0x10668): reads the size directly
(no flag — the caller consumes it), requires size >= 4 with an overflow
guard, `AParcel_readByteArray` with a validator requiring length exactly
128, the 4 int32s and 2 int64s in the same order, the parcelable array,
then skips to start + size. The descriptor type is `BufferDescriptorInfo`,
not `AllocationDescriptor`, and its first field is a 128-byte array —
which is what killed v1 and v2 (both read the array length and array bytes
as width/height/format).

V3 (parser-v3.s, 5,919 B): flag (nonzero, else fail) -> start ->
size (152 <= size <= 0x100000: complete through the 4th int32) ->
end = start + size (int32-fit check) -> array length (exactly 128, matching
the framework reader's own validator) -> skip 128 bytes by position
arithmetic (`AParcel_readString` is NOT imported by the shim and no new
import can be added by a footprint patch; `AParcel_readByteArray` would
need a vector-struct + allocator callback) -> width/height/layerCount/
format -> `setDataPosition(end)` (skips the int64s + metadata array) ->
the same validation and format mapping. 412 B (<= 436). Disassembly
reviewed instruction by instruction: all 11 `bl` targets resolve to the
exact PLT addresses (readInt32 x7, getDataPosition x2, setDataPosition x2);
all branches land inside the footprint; constants correct (152, 0x100000,
128, 4095, 22, 1/2/4/5).

Fixed v3 binary: SHA cb37e02e34aba63caebb16d98233b2f993d89c4e6a813ff997b011906bba4520,
1,711,816 B (same size), 382 bytes differ, all inside the footprint
(splice asserted the original first/last footprint bytes, NOP-padded the
remaining 24 bytes, verified every other byte unchanged). The v2 fixed
binary (fc8cb90f, 390 bytes) is superseded; its source (parser-v2.s) and
image are preserved, and it is reproducible from parser-v2.s +
build_patch.sh + splice_patch.py.

### 3.3 Parser v4 (the flown fix) — v3 crashed on its own success path, frame fixed

Smoke #3 (v3 parser) reached REJECT 0 — the first allocate2 was accepted —
but the shim died with SIGSEGV inside the parser (section 5c). The
tombstone named PC 0x120b4 (`str w0,[x1]`, the out-code store) and fault
address 0x120c914c784. Root cause, proven from the fault address: the v3
64-byte frame OVERLAPPED two stack slots — `str x4,[sp,#8]` writes 8 bytes
(#8..#15) for the saved out-code pointer, while `str w0,[sp,#12]` writes
the 4-byte end value at #12..#15, clobbering the pointer's high word with
the end value (0x120 = 288). V1/v2 never reached the success store, so the
overlap was latent through two smokes; v3 was the first parser to parse
successfully, and it crashed only on the final store. The v3 parser LOGIC
was correct; only the frame layout was broken.

V4 (parser-v4.s, 5,736 B): identical logic to v3, 80-byte frame with ALL
slots disjoint by byte range — scratch [sp,#0..#3], start [sp,#4..#7],
out-code pointer [sp,#8..#15], end [sp,#16..#19], padding [sp,#20..#31],
saved x21/x22 [sp,#32..#47], x19/x20 [sp,#48..#63], x29/x30 [sp,#64..#79].
412 B (<= 436). Disassembly reviewed instruction by instruction INCLUDING
a slot-disjointness check (the check the v1/v2/v3 reviews missed): all 11
`bl` targets correct, epilogue offsets match the prologue saves, end at
[sp,#16] disjoint from the out-code pointer at [sp,#8].

Fixed v4 binary: SHA bf8661d0bbb67c996fbf8857cdfa88eb6fbc4175529480e7202ae5e969853a98,
1,711,816 B (same size), 380 bytes differ, all inside the footprint. The
v3 fixed binary (cb37e02e, 382 bytes) is superseded; its source
(parser-v3.s) and image are preserved.

## 4. Fix artifacts

Vendor patch (patch_vendor_t717.py, box, T716-script adaptation): base
t716-vendor.img SHA 9a2519b1 verified before patching; rm-first-then-write
(vendor.img has ZERO free inodes) for
`/bin/hw/android.hardware.graphics.allocator-service.t628`; allocated inode
== 731 asserted; mode 0100755, uid 0, gid 2000, xattr as above; e2fsck -f
-y then e2fsck -f -n both 0; dumped patched binary byte-compared to the
fixed binary; neighbours byte-identical to base (init.virt.rc 1693 B,
composer .rc 259 B, t628 .rc 275 B, init.ranchu.rc 5935 B, manifest.xml
2755 B, allocator .rc 258 B, mapper 2.0-impl.so 122456 B,
mapper.default.so 10416 B) with explicit SYS_RAWIO assertions (RC1 SHA
prefix 47a9bd4a, RC2 prefix 468f2a85; hwc stanza 1x `SYS_NICE SYS_RAWIO`,
allocator stanza 1x plain `SYS_NICE`); composer ELF (8d62a642) + HIDL
allocator (91d84fb9) byte-identical; base untouched; size 134,217,728.

- V1 (superseded after smoke #1): t717-vendor.img SHA 932b478abfeef95fef9a7ae43ec3b71422929546daf821f6483e5ee58ebbb632 (134,217,728 B); t717-fix.cpio.gz SHA f313b042fc74e2dffb90a96feedecd7fa8b314e75a30ba37e9ca6864f33db029 (53,378,872 B); t717-modified.dtb SHA dc3ec38777ff1b85652d851ced96dafc2e443d523ad1e6efe8de2cb11db5bd7c (2,049 B, initrd-end 0x1132e7f38). Preserved as *-v1.* on the box, sandbox, and Mac.
- V2 (flown for smoke #2 only; superseded after smoke #2): t717-vendor.img SHA dc0a7628a9f4c53b3a49027b95e5c3fdbd2c114be9cefba2bcbe0fdb2c8fe922 (134,217,728 B, inode 731, e2fsck 0/0, neighbours + SYS_RAWIO intact); t717-fix.cpio.gz SHA 1ed6b2f5ff38529f6a874e7bb914ada0e9db1001cd2142f13d6a304e8b327633 (53,377,927 B, 238 members, only `ramdisk-images/vendor.img` differs); t717-modified.dtb SHA 6d32a12cb1c35fe3954465252af4b8ffc6cd3b9cefdbdd38d85fe30af68305ca (2,049 B, initrd-start 0x110000000 unchanged, initrd-end 0x1132e7b87 = start + 53,377,927, 2 value bytes differ). Preserved as *-v2.* on the box, sandbox, and Mac.
- V3 (flown for smoke #3 only; superseded after smoke #3 — stack-overlap crash): t717-vendor.img SHA 6990e87f43ad878d7bb9c8d771a98719c3f7f0ee3448fe645c2326bfd1708afc (134,217,728 B, inode 731, e2fsck 0/0, neighbours + SYS_RAWIO intact); t717-fix.cpio.gz SHA c26bd6494bbd0d66d9666a4c555f91135510a7c9ab03fd050ebca25ad6747f35 (53,378,919 B, archive size unchanged 725,742,732); t717-modified.dtb SHA 46e563d511db1b85a1cece45f6e007a8eb971b6bd0889609b0da6c9373dcafac (2,049 B, initrd-start 0x110000000 unchanged, initrd-end 0x1132e7f67 = start + 53,378,919, 2 value bytes differ). Preserved as *-v3.* on the box, sandbox, and Mac.
- V4 (flown for smoke #4, flight-01, control): t717-vendor.img SHA 5d2d82b0fea609667512601e319290f0b6362d988167a0419b31143bc6788d00 (134,217,728 B, inode 731, e2fsck 0/0, neighbours + SYS_RAWIO intact); t717-fix.cpio.gz SHA fceef5dfe84b1bde94976c36dc58a0a64018b686b75f8e29217d93b5683c2ecf (53,378,799 B, archive size unchanged 725,742,732); t717-modified.dtb SHA 1a9617951a7d0dacd309b39ccf1716e33059ec99a4667136b9757fde7eaa27e0 (2,049 B, initrd-start 0x110000000 unchanged, initrd-end 0x1132e7eef = start + 53,378,799, 2 value bytes differ).
- Unchanged bases: t715-system.img 1283cbf2 (2,059,816,960 B), t705-Image 53d9563e (20,126,208 B), canonical-Image 05c43917 (23,073,280 B), QEMU 11.1.2 8e5c46dc (31,498,864 B).

Shadow-copy check: the CPIO member table contains a top-level vendor/ tree
with DIFFERENT allocator/composer binaries and .rc files — but NO t628
binary and NO t628 .rc there. The only t628 copy is in vendor.img, and the
mounted vendor.img is the live tree (proven by T716: the SYS_RAWIO fix in
vendor.img .rc files took effect). No shadow-copy trap for this fix.

All three artifact SHAs verified on the box, in the sandbox, and on the
Mac (guest-inputs/) before flight.

## 5. Step 3a smoke #1 (240 s guest, v1 parser) — WALL STANDS

Launched 16:00:22Z (supervisor PID 97172, QEMU PID 97175). Ended 16:04:47Z:
guest target reached (264.8 s >= 240 s), host 265.63 s, QEMU exit 0. UART:
13,485 lines, 1,375,440 B, SHA c0d63737cd251e94dacc103a2e6fe7997c2b91886930a1531bdda0cb4e405e5a.
Evidence preserved in flights/smoke-v1-parser/ (archived before the
re-run; never overwritten).

Counts: T628 lines 3 (shim exercised), REJECT 1 (L10408, guest 114.2 s),
GraphicBufferAllocator failure 1 (L10647: 1280x720 format 1 usage 1b00: 22),
allocateBuffers failure 1 (L10648, 0x0), getDisplayAttribute failed 1
(tolerated), EPERM 0, mapped 0 (not expected in the 240 s window), Watchdog
kills 0, system_server starts 1, installd-not-found 0, queueBuffer 0,
presentDisplay 0, DisplayDeviceInfo 1 (L11920, display formed). The
signature is IDENTICAL to the T716 smoke (REJECT 1, GBA 1, allocbuf 1):
the v1 fix did not take effect. No flight-01 was launched on the v1 stack.
Root cause (section 3.1): the v1 parser read the AIDL null flag as the
size and skipped 4 bytes too far. The v1 binary provably RAN (verified
chain vendor.img -> CPIO -> preflight SHAs + T628 lines present; no second
t628 copy exists), so this was a parser-logic failure, not a delivery
failure.

## 5b. Step 3a smoke #2 (240 s guest, v2 parser) — WALL STANDS

Launched 16:17:31Z (supervisor PID 98383, QEMU PID 98393). Ended
16:22:31Z on the host cap (300.3 s host, guest 196.8 s), QEMU exit 0.
UART: 13,847 lines, 1,467,482 B, SHA
0e048c1c3deeaabf64bdce4e66665c39400a7fa317407da1a44213b582817f6e.
Evidence preserved in flights/smoke-v2-parser/ (archived before the
re-run; never overwritten).

Counts (verify_smoke_t717.py report): T628 lines 3 (L10392/10398/10404,
guest ~109.9 s), REJECT 1 (L10405, guest 109.96 s), GraphicBufferAllocator
failure 1 (L10833: 1280x720 format 1 usage 1b00: 22), allocateBuffers
failure 1 (L10834, 0x0), getDisplayAttribute failed 1 (L10821, tolerated),
DisplayDeviceInfo 1 (L11999, display formed), EPERM 0, mapped 0 (not
expected in the 240 s window), Watchdog kills 0, system_server starts 1,
installd-not-found 0 (installd started, pid 2430, firing up),
SensorService onBootPhase RETURN 1 (L13314, 6699 ms — the T715 wall stays
broken), queueBuffer/presentDisplay 0, boot_completed 0, sensorservice
avc denials 0, CALL getIMapperLibrarySuffix 1, REGISTERED 1, T637_MAPPER
1. Structural checks all PASS (kernel memory, init first stage, fb0,
/system mount, subHal 0, Prime 0). The signature is IDENTICAL to smoke
#1 (REJECT 1, GBA 1, allocbuf 1): the v2 fix did not take effect. No
flight-01 was launched on the v2 stack. Root cause (section 3.2): the v2
parser read the 128-byte BufferDescriptorInfo array field as
width/height/format. The v2 binary provably RAN (verified chain vendor.img
-> CPIO -> preflight SHAs + T628 lines present; no second t628 copy
exists), so this was a parser-logic failure, not a delivery failure.

## 5c. Step 3a smoke #3 (240 s guest, v3 parser) — PARSER WALL BROKEN, SHIM CRASHED

Launched 16:48:50Z (supervisor PID 267). Ended 16:53:51Z on the host cap
(300.4 s host, guest 231.8 s), QEMU exit 0. UART: 14,938 lines, 1,556,843
B, SHA 6af2f8b96a328fdff57d27ec3953d6dfddc59bfb7e28c67614a7683722302831.
Evidence preserved in flights/smoke-v3-parser/ (archived before the
re-run; never overwritten).

Counts (verify_smoke_t717.py report): T628 lines 2 (L10196/L10203),
REJECT 0 — the first time any T717 smoke (or any T715/T716 flight)
accepted a descriptor — GraphicBufferAllocator failure 1 (L10815:
1280x720 format 1 usage 1b00: -129, a NEW error code, not 22),
allocateBuffers failure 1 (L10816, 0x0), getDisplayAttribute failed 1
(tolerated), DisplayDeviceInfo 1, EPERM 0, mapped 0, Watchdog kills 0,
system_server starts 1, installd-not-found 0, SensorService onBootPhase
RETURN 1, queueBuffer/presentDisplay 0, boot_completed 0, sensorservice
avc denials 0. Structural checks all PASS.

The -129 is EX_TRANSACTION_FAILED: the shim process (pid 2522) died
mid-transaction. Tombstone (L10779): `Fatal signal 11 (SIGSEGV), code 1
(SEGV_MAPERR), fault addr 0x120c914c784 in tid 2522 (binder:2522_2)`,
backtrace #00 PC 0x120b4 `read_size_prefixed_descriptor`, #01
`on_transact`, #02 `ABB Binder::onTransact`. Root cause (section 3.3):
the v3 parser's stack-slot overlap, a lane-agent bug — the parser logic
was correct and reached its success path. The smoke criterion (REJECT 0
AND GBA-fail 0) was NOT met because of the crash, so no flight-01 was
launched on the v3 stack. The parser wall itself is BROKEN: REJECT 0
with the shim exercised proves the BufferDescriptorInfo parsing is
correct.

## 5d. Step 3a smoke #4 (240 s guest, v4 parser) — PARSER PROVEN, NEW WALL IN WRITE_RESULT

Launched 17:00:33Z (supervisor PID 973). Ended 17:05:33Z on the host cap
(300.5 s host, guest 220.4 s), QEMU exit 0. UART: 13,600 lines, 1,432,959
B, SHA 4c877ebdb7ab8a089888849d0de7a637e335c542cb9f6999a4a52d8edd87d841.
Evidence in flights/smoke/ (the v4 smoke; flights/smoke-v3-parser holds
the v3 smoke).

Counts (verify_smoke_t717.py report): T628 lines 3, REJECT 0,
GraphicBufferAllocator failure 1 (L10684: 1280x720 format 1 usage 1b00:
-129), allocateBuffers failure 1 (L10685, 0x0), getDisplayAttribute
failed 1 (tolerated), DisplayDeviceInfo 1, EPERM 0, mapped 0, Watchdog
kills 0, system_server starts 1, installd-not-found 0, SensorService
onBootPhase RETURN 1, queueBuffer/presentDisplay 0, boot_completed 0,
sensorservice avc denials 0. Structural checks all PASS. NO crash: zero
"fatal signal" lines, zero crash_dump lines, single REGISTERED (no
restart), zero binder-error lines.

The decisive line (L10355, guest 110.69 s):

    CALL method=allocate2 count=1 width=1280 height=720

The shim logged the allocate2 call with the CORRECT parsed values — the
v4 parser parsed the BufferDescriptorInfo correctly, on_transact
proceeded past the parser, and write_result ran without crashing. The
transaction then failed with -129 (EX_TRANSACTION_FAILED) in the reply
path, with the shim alive and no diagnostic. The smoke criterion (REJECT
0 AND GBA-fail 0) is NOT met, so no flight-01 was launched. The parser
wall is BROKEN and PROVEN; the first failing link has moved downstream
to write_result's reply path (section 9).

## 6. Step 3b flight-01 (2400 s guest) — NOT FLOWN

No flight-01 was launched. The brief gates Step 3b on the Step 3a smoke
criterion (REJECT 0 AND GraphicBufferAllocator failures 0 with the shim
exercised and the T716 holds intact). Smoke #4 met REJECT 0 and the holds
but still had 1 GBA failure (-129), caused deterministically by the
shim's write_result reply code — not by timing, not by the guest window.
A 2400 s flight would reproduce the same -129 at every allocation attempt
and could not show queueBuffer > 0 or a present dispatch while allocate2
returns failure. Flying it would burn ~45 min for zero new information.
The flight-01 criteria (nonzero allocation, queueBuffer > 0, first
present dispatch, mapped >= 1, Dexopt, FATAL rules) are therefore NOT
adjudicated — they remain open for the successor lane once the reply-path
wall is fixed.

## 7. A/B control (canonical kernel, 240 s guest) — NOT RUN

No control was run. The control's purpose is to discriminate
kernel-vs-fix effects against a passing run; with no passing run, it
adds nothing. The T716 control baseline stands: canonical kernel
05c43917, 576 lines, guest 20.36 s, QEMU self-exit 0, system_server
never reached. The T717 fix does not touch the kernel, DTB boot path, or
system image, so the canonical-kernel behaviour is unchanged by
construction. The control should be re-run by the successor lane
alongside its flight-01.

## 8. Adjudication

Verdict: FAIL — the mission (nonzero allocation + queueBuffer > 0 +
first present dispatch) was not achieved. The diagnosed parser wall WAS
broken; a new wall in the shim's reply path blocks allocation.

| Criterion | Required | Smoke #4 (v4) | Flight-01 | Met? |
|---|---|---|---|---|
| T628 REJECT allocate2 | 0 | 0 | not flown | YES (smoke) |
| GraphicBufferAllocator failures | 0 | 1 (-129) | not flown | NO |
| allocateBuffers failures | 0 | 1 (0x0) | not flown | NO |
| Nonzero allocation (CALL log) | > 0 | CALL logged, correct values, reply failed | not flown | NO |
| queueBuffer > 0 | > 0 | 0 | not flown | NO |
| First present dispatch | >= 1 | 0 | not flown | NO |
| EPERM (simplefb open /dev/mem) | 0 | 0 | not flown | YES (smoke) |
| simplefb: mapped | >= 1 | 0 (smoke window) | not flown | NOT ADJUDICATED |
| Dexopt NOT SKIPPED (Launcher3) | yes | n/a (smoke window) | not flown | NOT ADJUDICATED |
| Launcher3 FATAL / composer FATAL | 0 | 0 in window | not flown | NOT ADJUDICATED |
| Watchdog kills | 0 | 0 | not flown | YES (smoke) |
| boot_completed | — | 0 (smoke window) | not flown | — |
| system_server lifetimes | — | 1 (smoke window) | not flown | — |
| installd-not-found | 0 | 0 | not flown | YES (smoke) |
| SensorService onBootPhase return | >= 1 | 1 | not flown | YES (smoke) |
| getDisplayAttribute failures | tolerated | 1 | — | TOLERATED |

The T716 holds (EPERM 0, installd trigger, onBootPhase return, SYS_RAWIO
fix intact in the vendor image) all stand. The parser fix does not
regress any of them. What fails is solely the allocation reply path.

## 9. First failing link and next fix (FAIL)

Exact first failing link: `write_result`'s reply path in the T628 shim's
`on_transact` code-2 handler, AFTER the parser returns success and AFTER
the CALL log, BEFORE the framework's BpAllocator reads a valid reply.
Evidence chain (smoke #4 UART, all line numbers 1-based):

1. L10355: `CALL method=allocate2 count=1 width=1280 height=720` — the
   parser accepted the descriptor and on_transact logged the call with
   the correct values. The parser wall is broken.
2. No crash: zero "fatal signal", zero crash_dump, single REGISTERED,
   shim alive for the rest of the flight. The v3 crash cause (stack-slot
   overlap) is fixed and gone.
3. No binder diagnostic: zero "transaction failed" / FAILED_REPLY /
   DEAD_OBJECT lines anywhere in the UART.
4. L10684: `GraphicBufferAllocator: Failed to allocate (1280 x 720)
   layerCount 1 format 1 usage 1b00: -129` — EX_TRANSACTION_FAILED, the
   Bp reporting that the transaction itself failed.
5. L10685: the consequent 0x0 allocateBuffers failure.

The failure is therefore inside `write_result` (0x11a58) or the
on_transact success-path epilogue: either the reply status header is
missing/malformed, the returned AStatus is wrong, or the reply parcel's
NativeHandle/AllocationResult content fails driver-level validation (fd
translation) so the transaction fails in transport. The parser, the
transaction dispatch, the service lifecycle, and the framework caller are
all exonerated by the evidence above.

Next fix (for the successor lane), in order:

1. Disassemble `write_result` (0x11a58–0x11f3c) and the on_transact
   code-2 success epilogue in the ORIGINAL binary (SHA 745d19d0) and
   answer, from the disassembly: (a) is `AParcel_writeStatusHeader`
   called on the success path, and with what status? (b) what AStatus
   does write_result return, and what does on_transact return? (c) how
   is the NativeHandle built — numFds/numInts/data layout vs the
   `native_handle_t` contract, and is the memfd fd placed correctly?
   (d) what AllocationResult struct is passed to the imported
   `AllocationResult::writeToParcel`?
2. Compare against the framework's reply-read expectations in the
   guest's allocator-V2-ndk.so: `BpAllocator::allocate2`'s reply path
   (0x11998 onward: status-header read, AllocationResult read).
3. Apply the smallest fix in write_result (a second footprint patch —
   write_result's own function body is ~1250 bytes; assess its slack —
   or, if the disassembly is inconclusive, a diagnostic-logging shim
   build that dumps the reply shape to kmsg before returning).
4. Re-smoke (240 s): PASS requires REJECT 0 AND GBA-fail 0 with the shim
   exercised. Then flight-01 + control per the T717 brief's Step 3b.

Do NOT rework the parser: v4 is proven correct (CALL log with exact
values, twice — smoke #3's parse succeeded before its unrelated crash,
smoke #4's parse succeeded cleanly).

## 10. Gate evidence

G717_1_EVIDENCE: T717 Step 2 diagnosis + parser fix delivered and proven at the parser level — smoke #4 (v4, 13,600 lines, SHA 4c877ebd) has REJECT 0 with the shim exercised and CALL method=allocate2 count=1 width=1280 height=720 logged at L10355 with the correct parsed values; the BufferDescriptorInfo wire format was proven from the guest's allocator-V2-ndk.so disassembly (BpAllocator::allocate2 + writeToParcel + readFromParcel). The T628 parser wall is broken.
G717_2_EVIDENCE: NOT MET — no nonzero allocation reached the framework: smoke #4 GBA failure 1 with error -129 (EX_TRANSACTION_FAILED in write_result's reply path), queueBuffer 0, present dispatch 0, no flight-01 flown (smoke criterion not met). The mission criteria remain open for the successor lane.

## 11. Reflection

Per the standing reflection rule, the lane's own problems — answered in
writing before closing. (The -129 reply-path wall is a finding, not a
lane problem; it is covered in section 9.)

### R1. The parser diagnosis was wrong twice before it was right

(1) What happened, with evidence? The Step 2 diagnosis claimed the
original parser's byte-array read was "simply wrong", its field model
matched no real format, and its skip was "off by 4". All three claims
were wrong: the real parcelable (BufferDescriptorInfo) starts with a
128-byte array, the field model matched, and the skip was correct. V1
and v2 were then built on a wrong layout ([size][width][height][format]
[usage]) and both failed their smokes with the exact T716 signature.
Only the third investigation — disassembling the guest's own
allocator-V2-ndk.so — proved the true wire format. (2) Why? The
diagnosis was reasoned from AIDL conventions and the shim's own code
instead of from the framework library that actually writes the parcels.
The ground truth sat in the guest system image all along and was not
consulted until two smokes had burned. (3) Missed signal / earlier
check? The disassembly of BpAllocator::allocate2 + BufferDescriptorInfo
(30 min of work) before ANY parser was written would have given the
exact layout on the first attempt. (4) What changes? For any wire-format
fix, the framework library disassembly is the FIRST step, before the
diagnosis is written; a diagnosis citing only conventions (not
disassembly) is marked INFERENCE and cannot authorize a build. (5)
Where does it live? This reflection, plus a recommendation to the driver
that the T718 brief carry it as a Step 2 requirement.

### R2. The v3 parser crashed on its own success path (stack-slot overlap)

(1) What happened? Smoke #3 reached REJECT 0, then the shim died with
SIGSEGV at PC 0x120b4, fault 0x120c914c784: the v3 64-byte frame stored
the 8-byte out-code pointer at [sp,#8] and the 4-byte end value at
[sp,#12], so the end value clobbered the pointer's high word. (2) Why?
The frame layout was copied across v1→v2→v3 without a disjointness
check; all three reviews checked bl targets, constants, and branches but
never stack-slot byte ranges; and the bug was latent because v1/v2 never
reached the success store that dereferences the pointer. (3) Missed
signal? The source comments listed slots WITHOUT byte ranges — the
overlap is invisible in "[sp,#8] out_code, [sp,#12] end" unless the
8-byte width is made explicit. (4) What changes? Every hand-written asm
frame carries explicit byte-range comments, and the review checklist
gains a slot-disjointness check verified from the ASSEMBLED offsets, not
the source. (5) Where does it live? parser-v4.s header (done: full byte
ranges + the disjointness statement) and this reflection.

### R3. Clock estimation — third occurrence, reported as a defect

(1) What happened? The progress/heartbeat label "16:47Z" for the v3
splice was an estimate running ~12 min fast (clock read 16:41:06Z after
the box→sandbox transfer; build mtime 16:31Z). This is the third
occurrence after T716 and the pre-compaction T717 labels. (2) Why?
Timestamps were estimated from elapsed-work feel instead of read from
the clock. (3) Missed signal? The standing rule already said "record
times from the clock or spawn receipt, never from estimation" — the
failure was compliance, not knowledge; prose reminders had already
failed twice. (4) What changes — mechanical guard, APPLIED this lane?
Timestamps in progress/heartbeat files come ONLY from a `date` read
inside the same command that writes them (e.g. `TS=$(date -u ...)` in
the macssh command). No timestamp is ever typed from feel. (5) Where
does it live? TIMELINE.md clock-correction note + this reflection. Per
the standing rule, a third occurrence is reported plainly as a defect:
the first two reflections produced prose; only the mechanical guard
(typing no timestamp without a clock read in the same command) can
prevent a fourth.

## 12. Clock note

Two progress-file labels written before compaction (15:59Z STEP2 DONE,
16:20Z STEP2 FIX BUILT) were estimates running fast — the clock read
15:56:30Z after the vendor patch had already run (box mtime 15:48Z). A
THIRD occurrence followed in this lane: the progress/heartbeat label
"16:47Z" for the v3 splice was an estimate running ~12 min fast (clock
read 16:41:06Z after the box -> sandbox v3 transfer; build mtime 16:31Z).
The mechanical guard, applied from that point on: timestamps in
progress/heartbeat files come ONLY from a `date` read inside the same
command that writes them — never from elapsed-work estimation. This
repeats the T716 clock lesson: record times from the clock or spawn
receipt, never from estimation.

## 13. Inputs verified (Step 1)

findings-t716.md fab8a822 (25,437 B, Verdict FAIL); DEATH_NOTE-T716.md
321e4c30 (5,097 B); T716 UARTs flight-01 b178e4c6 (219,931 lines),
flight-ab ee9f6284 (576 lines), smoke d224de95 (13,664 lines), smoke-ext
2246dca5 (24,291 lines); T715 flight-01 UART 8d69ee3b (244,911 lines);
guest inputs t716-vendor.img 9a2519b1, t716-fix.cpio.gz 08eb665b,
t716-modified.dtb cbb25094, t715-system.img 1283cbf2, t705-Image 53d9563e,
canonical-Image 05c43917, extracted_kernel.config d840310a; composer ELF
8d62a642; composer source tree HEAD 40e9f153. No network fetch. No debugfs
on the Mac PATH; all image work on the box (debugfs 1.47.2).
