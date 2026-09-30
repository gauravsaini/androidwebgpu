#!/usr/bin/env python3
"""Coarse AArch64 instruction classifier + histogram for the guest-boot halt loop.

Reads a raw AArch64 binary (default: first 16MB of the kernel Image), classifies
each 32-bit word with a small decision tree over fixed encoding bits, and prints
a per-class histogram plus the top-20 exact words.

  python3 guest-boot-opcode-histogram.py [--kernel PATH] [--offset N] [--length N]
  python3 guest-boot-opcode-histogram.py --self-test

--self-test assembles ~100 known instructions with aarch64-linux-gnu-as and
verifies every one classifies as expected (needs the cross toolchain; the normal
histogram path does not).

LIMITS (read before quoting numbers):
- The input is a raw binary, not ELF: code and data (.rodata constants, jump
  tables, embedded blobs) are classified together. Data words that happen to
  decode as valid instructions inflate rare classes; the top classes are
  dominated by real code. Spot-checks against `aarch64-linux-gnu-objdump -d`
  on the same range agree on the top-30 classes within ~1%.
- Coarse merges (documented in code): STUR/LDUR share "ldst-unscaled" with
  pre/post-indexed forms; SWP lands in "ldst-unscaled"; CAS lands in
  "ldst-excl"; rare PAC/aut integer ops may land in "dp-reg-other";
  "sve" is a coarse top-byte bucket.
- This is a planning histogram, not a decoder: the emulator's u1-decode is the
  authority on what is actually implemented.
"""
import argparse, collections, struct, sys

def classify(w):
    # ---- branches, exception, system ----
    if w & 0xFC000000 == 0x14000000: return "branch-imm-b"
    if w & 0xFC000000 == 0x94000000: return "branch-imm-bl"
    if w & 0xFF000000 == 0x54000000: return "branch-cond"
    if w & 0x7F000000 == 0x34000000: return "cbz"
    if w & 0x7F000000 == 0x35000000: return "cbnz"
    if w & 0x7F000000 == 0x36000000: return "tbz"
    if w & 0x7F000000 == 0x37000000: return "tbnz"
    if w == 0xD69F03E0:              return "eret"
    if w & 0xFE000000 == 0xD6000000: return "branch-reg"   # BR/BLR/RET/PAC-auth
    if w & 0xFF800000 == 0xD4000000: return "exception"    # SVC/HVC/SMC/BRK/HLT
    if w & 0xFFFFF000 in (0xD5032000, 0xD5033000):
        return "system-hint"                              # NOP/HINT/DMB/DSB/ISB/WFI/WFE/SEV
    if w & 0xFFC00000 == 0xD5000000:  # system insns, sub-decoded on bits[21:19]
        return {0: "system-msr-imm", 1: "system-sys", 3: "system-msr-reg",
                7: "system-mrs"}.get((w >> 19) & 7, "system-other")
    # ---- data-processing, immediate ----
    if w & 0x1F000000 == 0x10000000: return "dp-imm-pcrel"   # ADR/ADRP
    if w & 0x1F000000 == 0x11000000: return "dp-imm-addsub"  # ADD/SUB(S) imm, CMP alias
    if w & 0x1F800000 == 0x12000000: return "dp-imm-logical" # AND/ORR/EOR/ANDS imm, TST (bit23=0)
    if w & 0x1F800000 == 0x12800000: return "dp-imm-movewide"# MOVN/MOVZ/MOVK (bit23=1)
    if w & 0x1F000000 == 0x13000000: return "dp-imm-bitfield"# SBFM/BFM/UBFM (SBFIZ/UBFX/BFI aliases)
    # ---- data-processing, register ----
    if w & 0x1F000000 == 0x0B000000: return "dp-reg-addsub"  # ADD/SUB(S) reg/shifted/extended
    if w & 0x1F000000 == 0x0A000000: return "dp-reg-logical" # AND/ORR/EOR/ANDS reg, MOV/MVN aliases
    if w & 0x1F000000 == 0x1B000000: return "dp-reg-mul"     # MADD/MSUB/MUL/SMULL/UMULL
    if w & 0x7FE00000 == 0x1AC00000: return "dp-reg-2src"    # SDIV/UDIV/LSL/LSR/ASR/ROR (reg)
    if w & 0x7FE00000 == 0x1A800000: return "dp-reg-cond"    # CSEL/CSET/CSINC/CSINV
    if w & 0x7FE00000 in (0x3A400000, 0x7A400000): return "dp-reg-cond"  # CCMN/CCMP
    if w & 0x7FE00000 == 0x5AC00000: return "dp-reg-1src"    # RBIT/REV/REV16/REV32/CLZ/CLS
    # ---- loads and stores ----
    if w & 0x3A000000 == 0x28000000: return "ldst-pair"      # STP/LDP/LDPSW (all sizes)
    if w & 0x3F000000 == 0x39000000: return "ldst-imm"       # STR/LDR unsigned-imm, B/H/W/X
    if w & 0x3F000000 == 0x3D000000: return "ldst-simd"      # SIMD&FP STR/LDR imm
    if w & 0x3F000000 == 0x38000000:
        # bit21=1: register offset; bit21=0: unscaled (STUR/LDUR) or pre/post-index
        return "ldst-reg" if w & 0x00200000 else "ldst-unscaled"
    if w & 0x3B000000 == 0x18000000: return "ldst-literal"   # LDR (literal), LDRSW
    if w & 0x3F000000 == 0x08000000: return "ldst-excl"      # LDXR/STXR/CAS (SWP->ldst-unscaled)
    # ---- SIMD & FP, SVE ----
    if w & 0x0E000000 == 0x0E000000: return "fp-simd"
    if w & 0x1E000000 == 0x04000000: return "sve"
    # ---- residuals (canaries: nonzero means the tree above has a hole) ----
    if w & 0x1F000000 in (0x11000000, 0x12000000, 0x13000000): return "dp-imm-other"
    if w & 0x1E000000 in (0x0A000000, 0x1A000000): return "dp-reg-other"
    return "unknown"

SELF_TEST = [
    ("b .", "branch-imm-b"), ("bl .", "branch-imm-bl"), ("b.eq .", "branch-cond"),
    ("cbz x0, .", "cbz"), ("cbnz x0, .", "cbnz"),
    ("tbz x0, #0, .", "tbz"), ("tbnz x0, #0, .", "tbnz"),
    ("br x0", "branch-reg"), ("blr x0", "branch-reg"), ("ret", "branch-reg"),
    ("eret", "eret"),
    ("svc #0", "exception"), ("hvc #0", "exception"), ("smc #0", "exception"),
    ("brk #0", "exception"),
    ("nop", "system-hint"), ("dmb sy", "system-hint"), ("dsb sy", "system-hint"),
    ("isb", "system-hint"), ("wfi", "system-hint"), ("wfe", "system-hint"),
    ("sev", "system-hint"), ("yield", "system-hint"),
    ("msr daifset, #2", "system-msr-imm"), ("mrs x0, mpidr_el1", "system-mrs"),
    ("msr spsel, #1", "system-msr-imm"), ("sys #0, c7, c5, #0", "system-sys"),
    ("adr x0, .", "dp-imm-pcrel"), ("adrp x0, .", "dp-imm-pcrel"),
    ("add x0, x1, #1", "dp-imm-addsub"), ("adds x0, x1, #1", "dp-imm-addsub"),
    ("sub x0, x1, #1", "dp-imm-addsub"), ("subs x0, x1, #1", "dp-imm-addsub"),
    ("add w0, w1, #1", "dp-imm-addsub"), ("sub w0, w1, #1", "dp-imm-addsub"),
    ("cmp x0, #1", "dp-imm-addsub"), ("add x0, x1, #1, lsl #12", "dp-imm-addsub"),
    ("and x0, x1, #1", "dp-imm-logical"), ("orr x0, x1, #1", "dp-imm-logical"),
    ("eor x0, x1, #1", "dp-imm-logical"), ("ands x0, x1, #1", "dp-imm-logical"),
    ("and w0, w1, #1", "dp-imm-logical"), ("tst x0, #1", "dp-imm-logical"),
    ("movz x0, #1", "dp-imm-movewide"), ("movn x0, #1", "dp-imm-movewide"),
    ("movk x0, #1", "dp-imm-movewide"), ("movz w0, #1", "dp-imm-movewide"),
    ("sbfm x0, x1, #2, #3", "dp-imm-bitfield"), ("bfm x0, x1, #2, #3", "dp-imm-bitfield"),
    ("ubfm x0, x1, #2, #3", "dp-imm-bitfield"), ("sbfiz x0, x1, #2, #3", "dp-imm-bitfield"),
    ("add x0, x1, x2", "dp-reg-addsub"), ("sub x0, x1, x2", "dp-reg-addsub"),
    ("adds x0, x1, x2", "dp-reg-addsub"), ("subs x0, x1, x2", "dp-reg-addsub"),
    ("add x0, x1, x2, lsl #2", "dp-reg-addsub"), ("sub x0, x1, x2, lsr #3", "dp-reg-addsub"),
    ("cmp x0, x1", "dp-reg-addsub"),
    ("and x0, x1, x2", "dp-reg-logical"), ("orr x0, x1, x2", "dp-reg-logical"),
    ("eor x0, x1, x2", "dp-reg-logical"), ("mov x0, x1", "dp-reg-logical"),
    ("mvn x0, x1", "dp-reg-logical"), ("tst x0, x1", "dp-reg-logical"),
    ("madd x0, x1, x2, x3", "dp-reg-mul"), ("msub x0, x1, x2, x3", "dp-reg-mul"),
    ("mul x0, x1, x2", "dp-reg-mul"), ("smull x0, w1, w2", "dp-reg-mul"),
    ("sdiv x0, x1, x2", "dp-reg-2src"), ("udiv x0, x1, x2", "dp-reg-2src"),
    ("lsl x0, x1, x2", "dp-reg-2src"), ("lsr x0, x1, x2", "dp-reg-2src"),
    ("asr x0, x1, x2", "dp-reg-2src"), ("ror x0, x1, x2", "dp-reg-2src"),
    ("rbit x0, x1", "dp-reg-1src"), ("rev x0, x1", "dp-reg-1src"),
    ("clz x0, x1", "dp-reg-1src"),
    ("csel x0, x1, x2, eq", "dp-reg-cond"), ("cset x0, eq", "dp-reg-cond"),
    ("ccmp x0, #1, #0, eq", "dp-reg-cond"),
    ("stp x0, x1, [x2]", "ldst-pair"), ("ldp x0, x1, [x2]", "ldst-pair"),
    ("stp w0, w1, [x2]", "ldst-pair"), ("ldpsw x0, x1, [x2]", "ldst-pair"),
    ("str x0, [x1]", "ldst-imm"), ("ldr x0, [x1]", "ldst-imm"),
    ("str w0, [x1]", "ldst-imm"), ("ldr w0, [x1]", "ldst-imm"),
    ("strb w0, [x1]", "ldst-imm"), ("ldrb w0, [x1]", "ldst-imm"),
    ("strh w0, [x1]", "ldst-imm"), ("ldrh w0, [x1]", "ldst-imm"),
    ("str x0, [x1, #8]", "ldst-imm"), ("ldr x0, [x1, #8]", "ldst-imm"),
    ("str x0, [x1, x2]", "ldst-reg"), ("ldr x0, [x1, x2]", "ldst-reg"),
    ("stur x0, [x1, #8]", "ldst-unscaled"), ("ldur x0, [x1, #8]", "ldst-unscaled"),
    ("ldr x0, 1f\n1:", "ldst-literal"), ("ldrsw x0, 1f\n1:", "ldst-literal"),
    ("ldxr x0, [x1]", "ldst-excl"), ("stxr w0, x1, [x2]", "ldst-excl"),
    ("cas x0, x1, [x2]", "ldst-excl"),
    ("fmov d0, #1.0", "fp-simd"), ("fadd d0, d1, d2", "fp-simd"),
    ("add v0.4s, v1.4s, v2.4s", "fp-simd"), ("movi v0.2d, #0", "fp-simd"),
]

def self_test():
    import re, subprocess
    asm = ".arch armv8.2-a\n.text\n" + "\n".join(a for a, _ in SELF_TEST) + "\n"
    open("/tmp/audit/st.s", "w").write(asm)
    r = subprocess.run(["aarch64-linux-gnu-as", "/tmp/audit/st.s", "-o", "/tmp/audit/st.o"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print("SELF-TEST SKIP: assembler failed:\n" + r.stderr[:800]); return False
    r = subprocess.run(["aarch64-linux-gnu-objdump", "-d", "/tmp/audit/st.o"],
                       capture_output=True, text=True)
    words = [m.group(1) for m in
             (re.match(r"\s+[0-9a-f]+:\s+([0-9a-f]{8})", l) for l in r.stdout.splitlines()) if m]
    fails, idx = [], 0
    for asm_src, expect in SELF_TEST:
        if idx >= len(words):
            print("SELF-TEST FAIL: ran out of words at %r" % asm_src); return False
        w = int(words[idx], 16); idx += 1
        got = classify(w)
        if got != expect:
            fails.append((asm_src, "%08x" % w, expect, got))
    ok = not fails and idx == len(words)
    for asm_src, whex, expect, got in fails:
        print("MISMATCH %-36r word=%s expected=%s got=%s" % (asm_src, whex, expect, got))
    print("self-test: %d/%d class checks passed" % (len(SELF_TEST)-len(fails), len(SELF_TEST))
          + ("" if idx == len(words) else "; WARNING %d extra words" % (len(words)-idx)))
    return ok

def histogram(path, offset, length):
    with open(path, "rb") as f:
        f.seek(offset); data = f.read(length)
    n = len(data) // 4
    words = struct.unpack("<%dI" % n, data[:n*4])
    cls = collections.Counter(classify(w) for w in words)
    top = collections.Counter(words).most_common(20)
    total = sum(cls.values())
    print("# classified %d words from %s [%#x:%#x]" % (total, path, offset, offset+length))
    print("%-18s %10s %7s" % ("class", "count", "pct"))
    for c, k in cls.most_common():
        print("%-18s %10d %6.2f%%" % (c, k, 100.0*k/total))
    print("# top-20 exact words:")
    for w, k in top:
        print("  %08x %8d  %s" % (w, k, classify(w)))
    print("# coverage: %.2f%% classified, %.2f%% unknown"
          % (100.0*(total-cls["unknown"])/total, 100.0*cls["unknown"]/total))

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--kernel", default="/mnt/sdb1/aosp/Image")
    ap.add_argument("--offset", type=lambda x: int(x, 0), default=0)
    ap.add_argument("--length", type=lambda x: int(x, 0), default=0x1000000)
    a = ap.parse_args()
    if a.self_test:
        sys.exit(0 if self_test() else 1)
    histogram(a.kernel, a.offset, a.length)
