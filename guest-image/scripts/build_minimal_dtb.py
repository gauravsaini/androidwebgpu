#!/usr/bin/env python3
"""Build the minimal ARM virt DTB for the Path N guest kernel.

Reproducible source for guest-image/minimal-virt.dtb (previously a
hand-built binary with no source). Hand-rolled FDT v17 writer -- no dtc
dependency.

Usage: python3 build_minimal_dtb.py [-o guest-image/minimal-virt.dtb]

Layout mirrors a minimal QEMU -M virt device tree:
  / { compatible = "arm,virt"; #address-cells = <2>; #size-cells = <2>; }
    chosen { bootargs, stdout-path }
    aliases { uart0 = "/uart@9000000"; }
    memory@40000000 { device_type = "memory"; reg = <0x40000000 0x8000000>; }
    cpus { #address-cells = <1>; #size-cells = <0>;
      cpu@0 { compatible = "arm,cortex-a57"; device_type = "cpu"; reg = <0>; }; }
    psci { compatible = "arm,psci-1.0"; method = "hvc"; }
    uart@9000000 { compatible = "arm,pl011"; reg = <0x9000000 0x1000>; }
    timer { compatible = "arm,armv8-timer"; }
"""
import argparse
import struct
import sys

FDT_BEGIN_NODE = 1
FDT_END_NODE = 2
FDT_PROP = 3
FDT_END = 9


class FdtBuilder:
    def __init__(self):
        self.struct = bytearray()
        self.strings = bytearray()
        self.str_off = {}  # string -> offset

    def _str(self, s: str) -> int:
        if s not in self.str_off:
            self.str_off[s] = len(self.strings)
            self.strings += s.encode() + b"\x00"
        return self.str_off[s]

    def _align(self):
        while len(self.struct) % 4:
            self.struct += b"\x00"

    def begin_node(self, name: str):
        self.struct += struct.pack(">I", FDT_BEGIN_NODE)
        self.struct += name.encode() + b"\x00"
        self._align()

    def end_node(self):
        self.struct += struct.pack(">I", FDT_END_NODE)

    def prop(self, name: str, value: bytes):
        self.struct += struct.pack(">I", FDT_PROP)
        self.struct += struct.pack(">II", len(value), self._str(name))
        self.struct += value
        self._align()

    def prop_str(self, name: str, s: str):
        self.prop(name, s.encode() + b"\x00")

    def prop_u32(self, name: str, *vals: int):
        self.prop(name, struct.pack(">" + "I" * len(vals), *vals))

    def prop_u64(self, name: str, *vals: int):
        self.prop(name, struct.pack(">" + "Q" * len(vals), *vals))

    def finish(self) -> bytes:
        self.struct += struct.pack(">I", FDT_END)
        # Header: magic, totalsize, off_dt_struct, off_dt_strings,
        # off_mem_rsvmap, version, last_comp_version, boot_cpuid_phys,
        # size_dt_strings, size_dt_struct
        off_rsvmap = 40
        off_struct = off_rsvmap + 16  # empty reservation block
        off_strings = off_struct + len(self.struct)
        totalsize = off_strings + len(self.strings)
        hdr = struct.pack(
            ">10I",
            0xD00DFEED,
            totalsize,
            off_struct,
            off_strings,
            off_rsvmap,
            17,  # version
            16,  # last compatible version
            0,   # boot_cpuid_phys
            len(self.strings),
            len(self.struct),
        )
        rsvmap = struct.pack(">2Q", 0, 0)  # empty reservation list terminator
        return hdr + rsvmap + bytes(self.struct) + bytes(self.strings)


def build() -> bytes:
    b = FdtBuilder()
    b.begin_node("")  # root
    b.prop_str("compatible", "arm,virt")
    b.prop_u32("#address-cells", 2)
    b.prop_u32("#size-cells", 2)

    b.begin_node("chosen")
    b.prop_str("bootargs", "console=ttyAMA0,115200 earlycon")
    b.prop_str("stdout-path", "/pl011@9000000")
    b.end_node()

    b.begin_node("aliases")
    b.prop_str("uart0", "/pl011@9000000")
    b.end_node()

    b.begin_node("memory@40000000")
    b.prop_str("device_type", "memory")
    b.prop_u64("reg", 0x40000000, 0x8000000)  # 128 MiB
    b.end_node()

    b.begin_node("cpus")
    b.prop_u32("#address-cells", 1)
    b.prop_u32("#size-cells", 0)
    b.begin_node("cpu@0")
    b.prop_str("compatible", "arm,cortex-a53")
    b.prop_str("device_type", "cpu")
    b.prop_u32("reg", 0)
    b.prop_str("enable-method", "psci")
    b.end_node()
    b.end_node()

    b.begin_node("psci")
    b.prop_str("compatible", "arm,psci-1.0")
    b.prop_str("method", "hvc")
    b.end_node()

    b.begin_node("pl011@9000000")
    b.prop_str("compatible", "arm,pl011")
    b.prop_u64("reg", 0x9000000, 0x1000)
    b.prop_u32("clock-frequency", 24000000)
    b.prop_u32("interrupts", 0, 1, 4)  # SPI 1, level-high
    b.end_node()

    b.begin_node("intc@8000000")
    b.prop_str("compatible", "arm,cortex-a15-gic")
    b.prop_u64("reg", 0x8000000, 0x10000, 0x8010000, 0x10000)
    b.prop_u32("#interrupt-cells", 3)
    b.prop_u32("#address-cells", 0)
    b.prop("interrupt-controller", b"")
    b.end_node()

    b.begin_node("timer")
    b.prop_str("compatible", "arm,armv8-timer")
    b.end_node()

    b.end_node()  # root
    return b.finish()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--output", default="guest-image/minimal-virt.dtb")
    args = ap.parse_args()
    blob = build()
    with open(args.output, "wb") as f:
        f.write(blob)
    print(f"wrote {args.output} ({len(blob)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
