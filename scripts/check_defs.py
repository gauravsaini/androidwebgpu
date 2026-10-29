import subprocess, zipfile, io, struct

def get_defined_classes(dex_bytes):
    magic = dex_bytes[:8]
    if not magic.startswith(b"dex\n"):
        return set()
    string_ids_size, string_ids_off = struct.unpack_from("<II", dex_bytes, 0x38)
    type_ids_size, type_ids_off = struct.unpack_from("<II", dex_bytes, 0x40)
    class_defs_size, class_defs_off = struct.unpack_from("<II", dex_bytes, 0x60)
    
    # Read string offsets
    string_offsets = [struct.unpack_from("<I", dex_bytes, string_ids_off + 4*i)[0] for i in range(string_ids_size)]
    
    def get_string(idx):
        off = string_offsets[idx]
        while dex_bytes[off] & 0x80:
            off += 1
        off += 1
        end = dex_bytes.find(b"\0", off)
        return dex_bytes[off:end].decode("latin1", errors="replace")
    
    # Read type descriptors
    type_descriptors = []
    for i in range(type_ids_size):
        str_idx = struct.unpack_from("<I", dex_bytes, type_ids_off + 4*i)[0]
        type_descriptors.append(get_string(str_idx))
        
    defined_classes = set()
    for i in range(class_defs_size):
        class_idx = struct.unpack_from("<I", dex_bytes, class_defs_off + 32*i)[0]
        defined_classes.add(type_descriptors[class_idx])
    return defined_classes

jars = [
    "/system/framework/framework.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity-t.jar",
]

for jar in jars:
    data = subprocess.check_output(["/opt/homebrew/opt/e2fsprogs/sbin/debugfs", "-R", f"cat {jar}", "/Users/Shared/android14-gsi/system.img"], stderr=subprocess.DEVNULL)
    z = zipfile.ZipFile(io.BytesIO(data))
    defs = set()
    for name in z.namelist():
        if name.endswith(".dex"):
            defs.update(get_defined_classes(z.read(name)))
    has_tm = "Landroid/net/TetheringManager;" in defs
    print(f"{jar}: DEFINES TetheringManager? {has_tm}")
    teth = [d for d in defs if "Tether" in d]
    print(f"   total defined classes: {len(defs)}, tethering classes: {len(teth)}")
    if has_tm:
        print(f"   -> FOUND DEFINITION IN {jar}!")
