import subprocess, zipfile, io, struct, os, re

# Let us load all defined classes from the current BOOTCLASSPATH_20
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

def map_to_system_img_path(path):
    # Flattened APEX mapping:
    # /apex/com.android.art/... -> /system/system_ext/apex/com.android.art/...
    if path.startswith("/apex/"):
        sub = path[len("/apex/"):]
        return f"/system/system_ext/apex/{sub}"
    return path

def get_defined_classes_from_jar(img_path):
    cmd = ["/opt/homebrew/opt/e2fsprogs/sbin/debugfs", "-R", f"cat {img_path}", "/Users/Shared/android14-gsi/system.img"]
    try:
        data = subprocess.check_output(cmd, stderr=subprocess.DEVNULL)
        z = zipfile.ZipFile(io.BytesIO(data))
        defs = set()
        for name in z.namelist():
            if name.endswith(".dex"):
                dex_bytes = z.read(name)
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
                for i in range(class_defs_size):
                    defs.add(type_descriptors[struct.unpack_from("<I", dex_bytes, class_defs_off + 32*i)[0]])
        return defs
    except Exception as e:
        print(f"Error reading {img_path}: {e}")
        return set()

print("Reading all defined classes in BOOTCLASSPATH_20...")
bcp_classes = set()
for p in BOOTCLASSPATH_20:
    sp = map_to_system_img_path(p)
    c = get_defined_classes_from_jar(sp)
    print(f"  {p}: {len(c)} classes")
    bcp_classes.update(c)

print(f"\nTotal classes in BOOTCLASSPATH_20: {len(bcp_classes)}")

# Now read const-class and invokes from SystemServiceRegistry.<clinit>
from disasm_clinit import parse_dex
# Let us get the list of referenced classes in SystemServiceRegistry.<clinit>
with open("scripts/disasm_clinit.py") as f:
    pass

# We ran disasm_clinit earlier and got 138 const-class and 24 FrameworkInitializers.
# Let us check which of those 138 const-classes are in bcp_classes!
import subprocess
out = subprocess.check_output(["python3", "scripts/disasm_clinit.py"], text=True)
missing_const_classes = []
for line in out.splitlines():
    if "const-class" in line:
        cls = line.split("const-class")[1].strip()
        if cls.startswith("L") and cls.endswith(";"):
            if cls not in bcp_classes:
                missing_const_classes.append(cls)

print(f"\nMissing const-classes in SystemServiceRegistry.<clinit>: {len(missing_const_classes)}")
for m in missing_const_classes:
    print(f"  MISSING CONST-CLASS: {m}")

# Also check the 24 FrameworkInitializers:
missing_initializers = []
for line in out.splitlines():
    if "invoke-static" in line and "FrameworkInitializer" in line:
        # e.g. invoke-static Landroid/net/ConnectivityFrameworkInitializer;->registerServiceWrappers
        mcls = line.split("invoke-static")[1].split("->")[0].strip()
        if mcls not in bcp_classes:
            missing_initializers.append(mcls)

print(f"\nMissing FrameworkInitializers in SystemServiceRegistry.<clinit>: {len(missing_initializers)}")
for m in missing_initializers:
    print(f"  MISSING INITIALIZER: {m}")
