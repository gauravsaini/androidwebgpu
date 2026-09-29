# U9 Guest Platform Contract (`guest-image/PLATFORM.md`)

The device-side contract leaf 5.1 (U12 orchestrator) implements against.
Every address here is mirrored by a constant in `src/platform.rs`, and
`cargo test -p guest-image` asserts the critical invariants
(entry inside RAM, console MMIO outside RAM).

## Physical memory map

- RAM base: `0x4000_0000`
- RAM size: `0x0800_0000` (128 MiB); top (exclusive): `0x4800_0000`
- Guest load address = entry point: `0x4000_0000` (start of RAM)
- Guest blob layout inside the image: `[code][zero pad to 0x1000][data]`
  - Code must fit below file offset `0x1000` (asserted at build time;
    current guest is 800 bytes).
  - Data section loads at `0x4000_1000`. Offsets within it:
    - `0x000`: line buffer (256 bytes)
    - `0x100`: first-word buffer (256 bytes)
    - `0x200`: `"pathn-sh> \0"` (prompt)
    - `0x210`: `"commands: echo <args> | help\n\0"` (help text)
    - `0x230`: `"unknown cmd: \0"`
    - `0x240`: `"\n\0"`
    - `0x250`: `"line too long\n\0"`

## Console MMIO (outside RAM — no overlap, asserted)

- Base: `0x0900_0000`, size `0x1000` (one page)
- `CONSOLE_TX = 0x0900_0000`: STRB a byte here → the host emits it on the console.
- `CONSOLE_RX = 0x0900_0008`: LDRB here → next input byte, or `0` if none available.
- Register protocol: byte-wide accesses (`LDRB`/`STRB`).

## GPU command-stream MMIO (Track A; outside RAM — no overlap, asserted)

- Base: `0x0A00_0000`, size `0x1000` (one page), clear of the console page.
- `GPU_DATA = 0x0A00_0000`: STRB one virtio-gpu control-stream byte here →
  the host appends it to the GPU port buffer.
- `GPU_SUBMIT = 0x0A00_0008`: STRB here (value ignored) → the buffered
  stream is submitted: the host decodes it (U7) and dispatches it (U8) to
  the WebGPU canvas. The buffer drains; the guest may stream again.
- Register protocol: byte-wide accesses (`STRB`), mirroring the console
  model. Reads return 0 (no readable registers).
- The `triangle` shell builtin streams a 104-byte `VIRTIO_GPU_CMD_SUBMIT_3D`
  packet (viewport 640×480, red clear, one `DRAW_ARRAYS` triangle) through
  this port.

## Entry contract (what 5.1 must establish before jumping to the entry point)

- `pc` = `0x4000_0000` (the image entry field; see `initial_cpu_state`)
- `sp` = `0x4800_0000` (RAM top)
- All general-purpose registers = 0, `pstate` = 0
- MMU off (identity map not required — the guest uses physical addresses only)
- Single vCPU; the guest executes `WFI` while idle waiting for input
- No FPU/SIMD use, no exceptions expected, no device tree: the console
  registers plus the GPU command-stream port above are the entire device
  model.

## Image format (`build` output)

- Header (24 bytes, little-endian): magic `"PNIM"` (`0x4D494E50`),
  `u32` version (`1`), `u64` entry, `u64` guest-blob size.
- Blob: guest code words (LE), zero-padded to file offset `0x1000`,
  then the data section.
- SBOM (`<image>.sbom.json`): `{"format":"pathn-sbom","image":"guest-image",
  "format_version":1,"inputs":[{"name","sha256","bytes"}...],"image_sha256"}`.
  Inputs hashed: `guest-code` (code words only, no padding), `guest-data`
  (data section bytes), `manifest` (canonical JSON
  `{"name":"pathn-sh","version":1,"load_addr":1073741824}`).

## Shell behavior (`pathn-sh`)

- Prints `pathn-sh> `, polls `CONSOLE_RX` (`WFI` between polls), echoes each
  input byte via `CONSOLE_TX`.
- Line buffer: 256 bytes. A longer line prints `\nline too long\n` and resets.
- Builtins: `echo <args>` (prints args + newline; no args → blank line),
  `help` (prints the command list).
- Unknown command: prints `unknown cmd: <first-word>` and re-prompts.
- Empty lines and lines that are only whitespace are ignored.
- Leading whitespace makes the line a no-op (first word is empty).
- Matching is exact and case-sensitive; `\n` and `\r` both end a line.

## Mini-assembler instruction set

Exactly: `ADRP`, `ADD` (immediate), `LDR`/`STR` (unsigned immediate, 64-bit),
`LDRB`/`STRB` (unsigned immediate), `MOVZ`, `ORR` (shifted register),
`CBZ`/`CBNZ`, `B`, `BL`, `RET`, `WFI`. One hand-computed golden word per
form lives in `src/asm.rs` tests; overlapping vectors are cross-checked
against `units/u1-decode` (`ADD` imm `0x91004420`, `B` `0x14000001`,
`RET` `0xD65F03C0`). There is intentionally no `CMP`/`SUB`: the guest
tests byte equality with the `ADD`+`ORR (LSL #56)`+`CBZ` trick proven
exhaustive in `asm::image_asm_eq_trick_exhaustive`.

## Honest bounds

This is a bare-metal shell, not Linux and not AOSP. Nothing here boots or
executes the guest — actual boot execution belongs to leaf 5.1 (U12
orchestrator), which implements its device side against this exact contract.
Linux/AOSP guests are M1+ work, out of scope for Path N.
