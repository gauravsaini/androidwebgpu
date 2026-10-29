import subprocess, zipfile, io, struct, os, re

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

NEW_JARS = [
    "/system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity-t.jar",
    "/system/system_ext/apex/com.android.appsearch/javalib/framework-appsearch.jar",
    "/system/system_ext/apex/com.android.wifi/javalib/framework-wifi.jar",
    "/system/system_ext/apex/com.android.media/javalib/updatable-media.jar",
    "/system/system_ext/apex/com.android.permission/javalib/framework-permission-s.jar",
    "/system/system_ext/apex/com.android.permission/javalib/framework-permission.jar",
    "/system/system_ext/apex/com.android.scheduling/javalib/framework-scheduling.jar",
    "/system/system_ext/apex/com.android.adservices/javalib/framework-sdksandbox.jar",
    "/system/system_ext/apex/com.android.adservices/javalib/framework-adservices.jar",
    "/system/system_ext/apex/com.android.uwb/javalib/framework-uwb.jar",
    "/system/system_ext/apex/com.android.ondevicepersonalization/javalib/framework-ondevicepersonalization.jar",
    "/system/system_ext/apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar",
    "/system/system_ext/apex/com.android.ipsec/javalib/android.net.ipsec.ike.jar",
    "/system/system_ext/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar",
]

def map_to_system_img_path(path):
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

candidate_classpath = BOOTCLASSPATH_20 + NEW_JARS

print(f"Checking full candidate classpath ({len(candidate_classpath)} jars)...")
all_classes = set()
for p in candidate_classpath:
    sp = map_to_system_img_path(p)
    defs = get_defined_classes_from_jar(sp)
    all_classes.update(defs)

print(f"Total defined classes across all {len(candidate_classpath)} jars: {len(all_classes)}")

out = subprocess.check_output(["python3", "scripts/disasm_clinit.py"], text=True)
missing_const_classes = []
for line in out.splitlines():
    if "const-class" in line:
        cls = line.split("const-class")[1].strip()
        if cls.startswith("L") and cls.endswith(";"):
            if cls not in all_classes:
                missing_const_classes.append(cls)

missing_initializers = []
for line in out.splitlines():
    if "invoke-static" in line and "FrameworkInitializer" in line:
        mcls = line.split("invoke-static")[1].split("->")[0].strip()
        if mcls not in all_classes:
            missing_initializers.append(mcls)

print(f"Missing const-classes: {len(missing_const_classes)} -> {missing_const_classes}")
print(f"Missing initializers:  {len(missing_initializers)} -> {missing_initializers}")
