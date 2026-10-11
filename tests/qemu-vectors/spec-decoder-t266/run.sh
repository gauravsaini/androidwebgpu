#!/bin/sh
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
evidence="$here/evidence"
build=$(mktemp -d "${TMPDIR:-/tmp}/track266-qemu.XXXXXX")
trap 'rm -rf "$build"' EXIT HUP INT TERM

LLVM_MC=${LLVM_MC:-llvm-mc}
LLVM_OBJCOPY=${LLVM_OBJCOPY:-llvm-objcopy}
LLVM_READOBJ=${LLVM_READOBJ:-llvm-readobj}
QEMU_SYSTEM_AARCH64=${QEMU_SYSTEM_AARCH64:-qemu-system-aarch64}

for tool in "$LLVM_MC" "$LLVM_OBJCOPY" "$LLVM_READOBJ" "$QEMU_SYSTEM_AARCH64"; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'missing required tool: %s\n' "$tool" >&2
    exit 2
  fi
done

mkdir -p "$evidence"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

run_vector() {
  name=$1
  source="$here/$name.S"
  object="$build/$name.o"
  binary="$build/$name.bin"
  trace="$evidence/$name.qemu.log"
  raw_trace="$build/$name.raw.qemu.log"
  console="$build/$name.console"

  "$LLVM_MC" -triple=aarch64-none-elf -filetype=obj "$source" -o "$object"
  if "$LLVM_READOBJ" --relocations "$object" | grep -q 'Relocation {'; then
    fail "$name.S contains unresolved relocations; raw vector bytes would be incomplete"
  fi
  "$LLVM_OBJCOPY" --only-section=.text -O binary "$object" "$binary"

  if ! "$QEMU_SYSTEM_AARCH64" \
      -machine virt -cpu cortex-a57 -m 128M \
      -nographic -monitor none -serial null \
      -semihosting-config enable=on,target=native \
      -device "loader,file=$binary,addr=0x40100000,cpu-num=0" \
      -d in_asm,cpu,int -D "$raw_trace" >"$console" 2>&1; then
    fail "$name QEMU run returned nonzero; inspect $console and $trace"
  fi

  # QEMU pads disassembly mnemonics with spaces. Trim those for clean diffs;
  # register values, instruction words, and exception records are unchanged.
  sed 's/[[:space:]]*$//' "$raw_trace" > "$trace"

  if ! grep -Fq 'handling as semihosting call 0x20' "$trace"; then
    fail "$name QEMU trace did not reach the guest's clean semihost exit"
  fi
}

snapshot_at() {
  trace=$1
  marker=$2
  awk -v marker="PC=$marker" '
    index($0, marker) > 0 { capture = 1 }
    capture {
      print
      if ($0 ~ /^PSTATE=/) exit
    }
  ' "$trace"
}

expect_reg() {
  state=$1
  name=$2
  expected=$3
  if ! printf '%s\n' "$state" | grep -Fq "$name=$expected"; then
    fail "QEMU snapshot missing $name=$expected"
  fi
}

bitfield_trace="$evidence/bitfield.qemu.log"
run_vector bitfield
state=$(snapshot_at "$bitfield_trace" 0000000040100400)
[ -n "$state" ] || fail "bitfield QEMU snapshot at 0x40100400 is missing"
expect_reg "$state" X00 0123456789abcdef
expect_reg "$state" X01 000000000000abcd
expect_reg "$state" X02 ffffffffffffffef
expect_reg "$state" X03 11223344556677cd
expect_reg "$state" X06 000000000000f0f0
grep -Fq 'ubfx' "$bitfield_trace" || fail 'QEMU trace lacks UBFM/UBFX'
grep -Fq 'sxtb' "$bitfield_trace" || fail 'QEMU trace lacks SBFM/SXTB'
grep -Fq 'bfxil' "$bitfield_trace" || fail 'QEMU trace lacks BFM/BFXIL'
grep -Fq 'bic' "$bitfield_trace" || fail 'QEMU trace lacks BIC'
printf 'PASS: bitfield (UBFM/SBFM/BFM and N=1 BIC); evidence %s\n' "$bitfield_trace"

load_store_trace="$evidence/load_store.qemu.log"
run_vector load_store
state=$(snapshot_at "$load_store_trace" 0000000040100400)
[ -n "$state" ] || fail "load/store QEMU snapshot at 0x40100400 is missing"
expect_reg "$state" X01 89ab45670123beef
expect_reg "$state" X03 ef01abcd12345678
expect_reg "$state" X06 00000000000000a5
expect_reg "$state" X07 00000000000000a5
expect_reg "$state" X12 0000000000000010
expect_reg "$state" X13 0000000000000020
expect_reg "$state" X22 0000000040200050
expect_reg "$state" X23 0000000040200120
grep -Fq 'str' "$load_store_trace" || fail 'QEMU trace lacks STR'
grep -Fq 'ldr' "$load_store_trace" || fail 'QEMU trace lacks LDR'
grep -Fq 'stp' "$load_store_trace" || fail 'QEMU trace lacks STP'
grep -Fq 'ldp' "$load_store_trace" || fail 'QEMU trace lacks LDP'
printf 'PASS: load/store (immediate, register-offset, indexed, pairs); evidence %s\n' "$load_store_trace"

system_trace="$evidence/system.qemu.log"
run_vector system
state=$(snapshot_at "$system_trace" 0000000040100400)
[ -n "$state" ] || fail "system QEMU snapshot at 0x40100400 is missing"
expect_reg "$state" X01 00000000deadbeef
expect_reg "$state" X04 0000000000004321
expect_reg "$state" X05 0000000000004321
expect_reg "$state" X08 0000000056000266
expect_reg "$state" X09 000000004010004c
grep -Fq 'dmb' "$system_trace" || fail 'QEMU trace lacks DMB'
grep -Fq 'dsb' "$system_trace" || fail 'QEMU trace lacks DSB'
grep -Fq 'isb' "$system_trace" || fail 'QEMU trace lacks ISB'
grep -Fq 'msr' "$system_trace" || fail 'QEMU trace lacks MSR'
grep -Fq 'mrs' "$system_trace" || fail 'QEMU trace lacks MRS'
grep -Fq 'Taking exception 2 [SVC] on CPU 0' "$system_trace" || fail 'QEMU trace lacks SVC exception'
grep -Fq 'with ESR 0x15/0x56000266' "$system_trace" || fail 'QEMU trace has the wrong SVC syndrome'
printf 'PASS: system (persistent MSR/MRS, barriers, SVC); evidence %s\n' "$system_trace"

printf 'All three QEMU regression vectors passed.\n'
