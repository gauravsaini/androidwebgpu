# T266 spec-decoder QEMU vectors

These three bare-metal AArch64 guests are regression vectors for the decoder and lift paths in `units/u1-decode` and `units/u2-ir-lift`. `run.sh` assembles each guest to raw bytes, rejects unresolved relocations, runs the bytes in real `qemu-system-aarch64`, and checks QEMU's `in_asm,cpu,int` trace for both executed encodings and result-register snapshots.

The guest image is loaded at `0x40100000` on `virt`, with `cortex-a57` and 128 MiB RAM. Each guest reaches a clean semihosting exit only after the result snapshot. The system vector installs a real EL1 vector table and records ESR_EL1/ELR_EL1 after SVC.

## Reproduce

On macOS with Homebrew LLVM, run:

```sh
LLVM_MC=/opt/homebrew/opt/llvm/bin/llvm-mc \
LLVM_OBJCOPY=/opt/homebrew/opt/llvm/bin/llvm-objcopy \
LLVM_READOBJ=/opt/homebrew/opt/llvm/bin/llvm-readobj \
./tests/qemu-vectors/spec-decoder-t266/run.sh
```

On a system where LLVM tools and QEMU are on `PATH`, run `./run.sh` from this directory. Set `QEMU_SYSTEM_AARCH64` to select another QEMU binary. Successful runs refresh the three committed `evidence/*.qemu.log` trace files.

## Vectors

| Class | Input bytes (little-endian instruction words) | Expected effect | QEMU-observed effect |
|---|---|---|---|
| Bitfield + N/BIC discriminator | `01 5c 48 d3` (UBFM), `02 1c 40 93` (SBFM), `03 3c 48 b3` (BFM), `86 00 25 8a` (BIC with N=1); generated from [bitfield.S](bitfield.S) | X1=`0xABCD`; X2=`0xFFFFFFFFFFFFFFEF`; X3=`0x11223344556677CD`; X6=`0xF0F0` | `evidence/bitfield.qemu.log`, decoded words and snapshot at PC `0x40100400` |
| Load/store | `80 06 00 f9` (STR immediate), `81 06 40 f9` (LDR immediate), `82 7a 35 f8` (STR register offset), `83 7a 75 f8` (LDR register offset), `c4 8e 00 f8` / `c5 86 00 f8` / `c6 8e 5f f8` / `c7 86 40 f8` (indexed STR/STR/LDR/LDR), `ea 2e 81 a9` (STP pre-index), `ec 36 c1 a8` (LDP post-index); generated from [load_store.S](load_store.S) | Reloaded memory: X1=`0x89AB45670123BEEF`; X3=`0xEF01ABCD12345678`; X6/X7=`0xA5`; X12/X13=`0x10/0x20`; writeback X22=`0x40200050`, X23=`0x40200120` | `evidence/load_store.qemu.log`, decoded words and snapshot at PC `0x40100400` |
| System | `bf 3b 03 d5` (DMB ISH), `9f 3f 03 d5` (DSB SY), `df 3f 03 d5` (ISB), `80 d0 18 d5` (MSR TPIDR_EL1), `81 d0 38 d5` (MRS TPIDR_EL1), `c1 4c 00 d4` (SVC #0x266); generated from [system.S](system.S) | X1=`0xDEADBEEF`; X4/X5=`0x4321`; SVC ESR=`0x56000266`; ELR=`0x4010004C` | `evidence/system.qemu.log`, snapshot at PC `0x40100400` and QEMU's `Taking exception 2 [SVC]` trace |

`N=1` here is the bit 21 inversion selector for logical shifted-register BIC; it is separate from the `N` width field in the UBFM/SBFM/BFM encoding. Both paths are included because the brief explicitly requires the N/BIC selection rule.

QEMU's `in_asm` records show the full input words in the same order. The table lists their little-endian byte order as consumed from each generated raw guest image.
