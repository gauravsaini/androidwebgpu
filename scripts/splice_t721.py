#!/usr/bin/env python3
"""
T721 composer splice: replace present()'s hardcoded setPresentFence(-1) with
a call to the t721 retire-fence trampoline hosted in repurposed dead
sync_file_info (0xb7670, 536 B), and extend .bss by 12 B for the trampoline
static storage (timeline_fd + fence_counter + logged_flag, zero-init).

Asserts every original value before writing. Verifies by full byte-diff that
the ONLY changes are: patch site (4 B), cave (536 B), one PHDR p_memsz (8 B),
.bss sh_size (8 B). Saves the original cave bytes for restoration (T718 rule).
"""
import hashlib
import struct

ORIG = "/home/muse/t721-work/composer-orig.bin"
TRAMP = "/home/muse/t721-work/tramp.bin"
OUT = "/home/muse/t721-work/composer-t721-patched.bin"
CAVE_SAVE = "/home/muse/t721-work/sync_file_info-orig.bin"

EXPECTED_ORIG_SHA = "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401"
EXPECTED_ORIG_SIZE = 19942712
PATCH_VADDR = 0x5BFF4
PATCH_ORIG_WORD = 0x12800001  # mov w1, #-1
TRAMP_VADDR = 0xB7670
CAVE_SIZE = 536
STATIC_EXTRA = 12
NOP = 0xD503201F

def sha(b):
    return hashlib.sha256(b).hexdigest()

def main():
    data = bytearray(open(ORIG, "rb").read())
    assert len(data) == EXPECTED_ORIG_SIZE, len(data)
    assert sha(data) == EXPECTED_ORIG_SHA, "orig SHA mismatch"
    tramp = open(TRAMP, "rb").read()
    print(f"trampoline: {len(tramp)} B SHA {sha(tramp)}")
    assert len(tramp) <= CAVE_SIZE, "trampoline exceeds cave"
    # Zero-pad to a 4-byte boundary so the NOP fill is whole words only and
    # can never spill past the cave end (241 B is not 4-aligned).
    pad = (-len(tramp)) % 4
    tramp = tramp + b"\x00" * pad
    assert (CAVE_SIZE - len(tramp)) % 4 == 0, "fill region not whole words"
    print(f"trampoline padded: {len(tramp)} B (+{pad} zero pad)")

    # --- ELF parse (64-bit LE) ---
    e_phoff = struct.unpack_from("<Q", data, 0x20)[0]
    e_shoff = struct.unpack_from("<Q", data, 0x28)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", data, 0x36)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", data, 0x3A)
    assert e_phentsize == 56 and e_shentsize == 64
    shstr_off = e_shoff + e_shstrndx * 64
    shstr_base = struct.unpack_from("<Q", data, shstr_off + 24)[0]
    sections = {}
    for i in range(e_shnum):
        o = e_shoff + i * 64
        name_off = struct.unpack_from("<I", data, o)[0]
        sh_addr, sh_off, sh_size = struct.unpack_from("<QQQ", data, o + 16)
        end = data.index(b"\x00", shstr_base + name_off)
        name = data[shstr_base + name_off:end].decode()
        sections[name] = (o, sh_addr, sh_off, sh_size)
    text_hdr, text_addr, text_off, text_size = sections[".text"]
    bss_hdr, bss_addr, bss_off, bss_size = sections[".bss"]
    print(f".text vaddr 0x{text_addr:x} off 0x{text_off:x} size 0x{text_size:x}")
    print(f".bss  vaddr 0x{bss_addr:x} size 0x{bss_size:x} end 0x{bss_addr + bss_size:x}")
    shift = text_addr - text_off
    assert shift == 0x4000, f"unexpected vaddr->offset shift 0x{shift:x}"

    # --- find LOAD segment ending at .bss end ---
    seg = None
    for i in range(e_phnum):
        o = e_phoff + i * 56
        p_type = struct.unpack_from("<I", data, o)[0]
        p_vaddr = struct.unpack_from("<Q", data, o + 16)[0]
        p_filesz = struct.unpack_from("<Q", data, o + 32)[0]
        p_memsz = struct.unpack_from("<Q", data, o + 40)[0]
        if p_type == 1 and p_vaddr + p_memsz == bss_addr + bss_size:
            seg = (o, p_vaddr, p_filesz, p_memsz)
    assert seg, "no LOAD segment ends at .bss end"
    phdr_off, seg_vaddr, seg_filesz, seg_memsz = seg
    print(f"last LOAD: vaddr 0x{seg_vaddr:x} filesz 0x{seg_filesz:x} memsz 0x{seg_memsz:x}")
    assert bss_size == 0x1748 and seg_memsz == 0x1A98, "unexpected .bss/memsz values"
    new_bss_end = bss_addr + bss_size + STATIC_EXTRA
    assert new_bss_end // 0x1000 == (bss_addr + bss_size) // 0x1000, "extension crosses page!"
    static_addr = bss_addr + bss_size
    assert static_addr == 0x115BC8, f"static addr 0x{static_addr:x} != trampoline assumption"
    print(f"static storage: 0x{static_addr:x}..0x{new_bss_end:x} (same page, zero-init)")

    # --- assert original bytes ---
    patch_off = PATCH_VADDR - shift
    cave_off = TRAMP_VADDR - shift
    assert text_off <= patch_off < text_off + text_size
    assert text_off <= cave_off and cave_off + CAVE_SIZE <= text_off + text_size
    orig_word = struct.unpack_from("<I", data, patch_off)[0]
    assert orig_word == PATCH_ORIG_WORD, f"patch site 0x{orig_word:08x}"
    cave_orig = bytes(data[cave_off:cave_off + CAVE_SIZE])
    open(CAVE_SAVE, "wb").write(cave_orig)
    print(f"cave original: {len(cave_orig)} B SHA {sha(cave_orig)} (saved for restoration)")

    # --- compute patch branch ---
    delta = TRAMP_VADDR - PATCH_VADDR
    assert delta % 4 == 0 and abs(delta) < (1 << 27)
    bl_word = 0x94000000 | ((delta // 4) & 0x3FFFFFF)
    print(f"patch: bl 0x{PATCH_VADDR:x} -> 0x{TRAMP_VADDR:x} = 0x{bl_word:08x}")

    # --- write ---
    struct.pack_into("<I", data, patch_off, bl_word)
    data[cave_off:cave_off + len(tramp)] = tramp
    for i in range(len(tramp), CAVE_SIZE, 4):
        struct.pack_into("<I", data, cave_off + i, NOP)
    struct.pack_into("<Q", data, phdr_off + 40, seg_memsz + STATIC_EXTRA)
    struct.pack_into("<Q", data, bss_hdr + 32, bss_size + STATIC_EXTRA)

    # --- full byte-diff confinement ---
    orig = open(ORIG, "rb").read()
    diffs = [i for i in range(len(orig)) if orig[i] != data[i]]
    print(f"total differing bytes: {len(diffs)}")
    expected = sorted([
        (patch_off, patch_off + 3),
        (cave_off, cave_off + CAVE_SIZE - 1),
        (phdr_off + 40, phdr_off + 47),
        (bss_hdr + 32, bss_hdr + 39),
    ])
    diffset = set(diffs)
    for a, b in expected:
        assert any(a <= i <= b for i in diffset), f"expected region untouched: 0x{a:x}"
    for i in diffs:
        assert any(a <= i <= b for a, b in expected), f"diff OUTSIDE expected regions: 0x{i:x}"
    print("[PASS] all diffs confined to the 4 expected regions")

    open(OUT, "wb").write(bytes(data))
    print(f"patched: {len(data)} B SHA {sha(bytes(data))}")
    # --- re-verify from disk ---
    back = open(OUT, "rb").read()
    assert struct.unpack_from("<I", back, patch_off)[0] == bl_word
    assert back[cave_off:cave_off + len(tramp)] == tramp
    assert struct.unpack_from("<Q", back, phdr_off + 40)[0] == seg_memsz + STATIC_EXTRA
    assert struct.unpack_from("<Q", back, bss_hdr + 32)[0] == bss_size + STATIC_EXTRA
    print("[PASS] patched binary re-verified from disk")

if __name__ == "__main__":
    main()
