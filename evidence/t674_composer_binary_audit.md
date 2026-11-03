# T674 composer binary audit

This is a read-only audit of the composer service carried by the verified T670
base image. It records a variance against T673's stored disassembly claims; it
does not alter the T673 evidence or the guest image.

## Inputs and runtime binary

- Base initramfs SHA-256: `afb2991f59288d1de1b9855b6cbcb6e8050bde3e9a23d2391d3f17c892a3d022`.
- The archive contains `ramdisk-images/vendor.img` (134,217,728 bytes) and a
  top-level copy of the composer executable. The executable was extracted from
  the vendor ext4 image at `/bin/hw/android.hardware.graphics.composer@2.1-service`
  and compared with the top-level CPIO entry; both are identical.
- Runtime service ELF SHA-256:
  `5246e2844230e6884398de0845fc49a84e92085adfdbd050804ccddba12c0eca`
  (2,223,240 bytes; AArch64 PIE, symbol table present).

## FMQ path

- `SimpleFBClient::setInputCommandQueue` starts at `0x6865c`; it constructs an
  input `MessageQueue` and stores it at `this + 0x70`.
- `SimpleFBClient::getOutputCommandQueue` starts at `0x68708`; it constructs a
  512-word output `MessageQueue` and stores it at `this + 0x78`.
- The actual `SimpleFBClient::executeCommands` symbol starts at `0x687ec`.
  At `0x68820`, a zero `inHandles.size()` branches to `0x688c4`; a null handle
  data pointer at `0x6882c` takes the same branch. The callback at `0x688d0`
  returns `Error::NONE`, `outQueueChanged=false`, `outLength=0`, and no handles.
- The method body does not read the input queue at `this + 0x70`, write the
  output queue at `this + 0x78`, or parse FMQ commands. This confirms the Link 4
  failure mechanism in the T673 flight. The T673 note's stated function start
  `0x687c8` is not the symbol start in this verified ELF; `0x687c8` is inside
  `getOutputCommandQueue`.

## Blit path variance

- This ELF has **one static direct call site** to
  `FramebufferDevice::blitFromBuffer` (`0x69b40`): the `bl` at `0x688a8` in
  `executeCommands`.
- That call is reached only after `inHandles` is nonempty and the first handle
  has been mapped at `0x68874`. It is not gated on a parsed
  `PRESENT_DISPLAY` command; the method does not parse the FMQ command stream.
- Therefore the T673 statement “zero callers anywhere in the binary” is not
  correct for this ELF. The supported conclusion is narrower: there is no
  FMQ-driven presentation dispatch. T673's UART contains no instrumentation
  that establishes whether the conditional static call executed during that
  flight.

## Result

The runtime binary confirms the empty-handle FMQ omission and returns no
validation output. Fixing the display path requires a correct command reader,
output writer, and presentation-time client-target dispatch. The worktree has
no `SimpleFBClient.cpp`, command-engine implementation, Android platform
headers, or Android cross-build toolchain. No speculative binary rewrite or
replacement HAL was built under the single-image rule. T674 stopped before
image creation and did not boot a guest.

