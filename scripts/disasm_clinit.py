import struct

def parse_dex(dex_bytes):
    string_ids_size, string_ids_off = struct.unpack_from("<II", dex_bytes, 0x38)
    type_ids_size, type_ids_off = struct.unpack_from("<II", dex_bytes, 0x40)
    proto_ids_size, proto_ids_off = struct.unpack_from("<II", dex_bytes, 0x48)
    field_ids_size, field_ids_off = struct.unpack_from("<II", dex_bytes, 0x50)
    method_ids_size, method_ids_off = struct.unpack_from("<II", dex_bytes, 0x58)
    class_defs_size, class_defs_off = struct.unpack_from("<II", dex_bytes, 0x60)
    
    string_offsets = [struct.unpack_from("<I", dex_bytes, string_ids_off + 4*i)[0] for i in range(string_ids_size)]
    def get_string(idx):
        off = string_offsets[idx]
        while dex_bytes[off] & 0x80:
            off += 1
        off += 1
        end = dex_bytes.find(b"\0", off)
        return dex_bytes[off:end].decode("latin1", errors="replace")
    
    type_descriptors = [get_string(struct.unpack_from("<I", dex_bytes, type_ids_off + 4*i)[0]) for i in range(type_ids_size)]
    
    methods = []
    for i in range(method_ids_size):
        class_idx, proto_idx, name_idx = struct.unpack_from("<HHI", dex_bytes, method_ids_off + 8*i)
        methods.append((type_descriptors[class_idx], get_string(name_idx)))
        
    def read_uleb128(off):
        result = 0
        shift = 0
        while True:
            b = dex_bytes[off]
            off += 1
            result |= (b & 0x7f) << shift
            if not (b & 0x80):
                break
            shift += 7
        return result, off

    # Find Landroid/app/SystemServiceRegistry; class def
    ssr_def_off = None
    for i in range(class_defs_size):
        class_idx, access_flags, superclass_idx, interfaces_off, source_file_idx, annotations_off, class_data_off, static_values_off = struct.unpack_from("<IIIIIIII", dex_bytes, class_defs_off + 32*i)
        if type_descriptors[class_idx] == "Landroid/app/SystemServiceRegistry;":
            ssr_def_off = class_data_off
            break

    if not ssr_def_off:
        print("Landroid/app/SystemServiceRegistry; not found in this dex")
        return

    off = ssr_def_off
    static_fields_size, off = read_uleb128(off)
    instance_fields_size, off = read_uleb128(off)
    direct_methods_size, off = read_uleb128(off)
    virtual_methods_size, off = read_uleb128(off)
    
    for _ in range(static_fields_size):
        _, off = read_uleb128(off)
        _, off = read_uleb128(off)
    for _ in range(instance_fields_size):
        _, off = read_uleb128(off)
        _, off = read_uleb128(off)

    method_idx = 0
    clinit_code_off = None
    for _ in range(direct_methods_size):
        idx_diff, off = read_uleb128(off)
        method_idx += idx_diff
        access_flags, off = read_uleb128(off)
        code_off, off = read_uleb128(off)
        m_class, m_name = methods[method_idx]
        if m_name == "<clinit>":
            clinit_code_off = code_off
            print(f"Found <clinit> at code_off 0x{code_off:x}")

    if not clinit_code_off:
        print("<clinit> not found")
        return

    # Parse code_item
    registers_size, ins_size, outs_size, tries_size, debug_info_off, insns_size = struct.unpack_from("<HHHHII", dex_bytes, clinit_code_off)
    insns_off = clinit_code_off + 16
    insns = dex_bytes[insns_off : insns_off + insns_size * 2]
    print(f"Code item: registers={registers_size}, insns_size={insns_size} uint16s ({len(insns)} bytes)")

    # Scan instructions for const-class (0x1c) and invoke-static (0x71, 0x77)
    i = 0
    const_classes = []
    invokes = []
    while i < len(insns):
        opcode = insns[i]
        if opcode == 0x1c: # const-class vAA, type@BBBB
            reg = insns[i+1]
            type_idx = struct.unpack_from("<H", insns, i+2)[0]
            const_classes.append((i // 2, type_descriptors[type_idx]))
            i += 4
        elif opcode in (0x71, 0x77): # invoke-static
            count_reg = insns[i+1]
            method_idx = struct.unpack_from("<H", insns, i+2)[0]
            invokes.append((i // 2, methods[method_idx]))
            i += 6
        else:
            # We just need to find all const-classes and invokes, let us also scan 2-byte increments
            i += 2

    print(f"\nTotal const-class instructions found: {len(const_classes)}")
    for pc, cls in const_classes:
        print(f"  PC {pc:4d} (0x{pc:04x}): const-class {cls}")

    print(f"\nTotal static invokes found: {len(invokes)}")
    for pc, (mcls, mname) in invokes:
        if "FrameworkInitializer" in mcls or "registerService" in mname or "initialize" in mname:
            print(f"  PC {pc:4d} (0x{pc:04x}): invoke-static {mcls}->{mname}")

with open("/tmp/t641_framework/classes.dex", "rb") as f:
    parse_dex(f.read())
