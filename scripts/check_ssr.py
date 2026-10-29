import subprocess, zipfile, io, struct, os, glob

def get_dex_data(jar_path):
    cmd = ["/opt/homebrew/opt/e2fsprogs/sbin/debugfs", "-R", f"cat {jar_path}", "/Users/Shared/android14-gsi/system.img"]
    try:
        data = subprocess.check_output(cmd, stderr=subprocess.DEVNULL)
        z = zipfile.ZipFile(io.BytesIO(data))
        dexes = []
        for name in z.namelist():
            if name.endswith(".dex"):
                dexes.append(z.read(name))
        return dexes
    except Exception as e:
        return []

def parse_dex_classes(dex_bytes):
    magic = dex_bytes[:8]
    if not magic.startswith(b"dex\n"):
        return set(), set(), set()
    string_ids_size, string_ids_off = struct.unpack_from("<II", dex_bytes, 0x38)
    type_ids_size, type_ids_off = struct.unpack_from("<II", dex_bytes, 0x40)
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
    
    defined = {type_descriptors[struct.unpack_from("<I", dex_bytes, class_defs_off + 32*i)[0]] for i in range(class_defs_size)}
    referenced = set(type_descriptors)
    return defined, referenced

# Current 20-jar BOOTCLASSPATH from T640
BOOTCLASSPATH_20 = [
    "/apex/com.android.art/javalib/core-oj.jar",
    "/apex/com.android.art/javalib/core-libart.jar",
    "/apex/com.android.art/javalib/okhttp.jar",
    "/apex/com.android.art/javalib/bouncycastle.jar",
    "/apex/com.android.art/javalib/apache-xml.jar",
    "/system/framework/framework.jar",
    "/system/framework/framework-graphics.jar",
    "/system/framework/ext.jar",
    "/system/framework/telephony-common.jar",
    "/system/framework/voip-common.jar",
    "/system/framework/ims-common.jar",
    "/apex/com.android.i18n/javalib/core-icu4j.jar",
    "/apex/com.android.btservices/javalib/framework-bluetooth.jar",
    "/apex/com.android.conscrypt/javalib/conscrypt.jar",
    "/apex/com.android.devicelock/javalib/framework-devicelock.jar",
    "/apex/com.android.healthfitness/javalib/framework-healthfitness.jar",
    "/apex/com.android.os.statsd/javalib/framework-statsd.jar",
    "/apex/com.android.sdkext/javalib/framework-sdkextensions.jar",
    "/apex/com.android.virt/javalib/framework-virtualization.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar"
]

print("Reading SystemServiceRegistry from /tmp/t641_framework/classes.dex...")
with open("/tmp/t641_framework/classes.dex", "rb") as f:
    defined_fw, ref_fw = parse_dex_classes(f.read())

ssr_classes = [c for c in defined_fw if c.startswith("Landroid/app/SystemServiceRegistry")]
print(f"Found {len(ssr_classes)} SystemServiceRegistry classes defined in framework.jar classes.dex")

# Let us find all referenced types in SystemServiceRegistry methods / bytecodes
# In particular, look at disassembling SystemServiceRegistry.<clinit>
