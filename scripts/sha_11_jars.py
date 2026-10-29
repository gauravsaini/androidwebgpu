import subprocess, hashlib

NEW_11_JARS = [
    "/system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity-t.jar",
    "/system/system_ext/apex/com.android.appsearch/javalib/framework-appsearch.jar",
    "/system/system_ext/apex/com.android.wifi/javalib/framework-wifi.jar",
    "/system/system_ext/apex/com.android.media/javalib/updatable-media.jar",
    "/system/system_ext/apex/com.android.permission/javalib/framework-permission-s.jar",
    "/system/system_ext/apex/com.android.scheduling/javalib/framework-scheduling.jar",
    "/system/system_ext/apex/com.android.adservices/javalib/framework-sdksandbox.jar",
    "/system/system_ext/apex/com.android.adservices/javalib/framework-adservices.jar",
    "/system/system_ext/apex/com.android.uwb/javalib/framework-uwb.jar",
    "/system/system_ext/apex/com.android.ondevicepersonalization/javalib/framework-ondevicepersonalization.jar",
]

for jar in NEW_11_JARS:
    cmd = ["/opt/homebrew/opt/e2fsprogs/sbin/debugfs", "-R", f"cat {jar}", "/Users/Shared/android14-gsi/system.img"]
    data = subprocess.check_output(cmd, stderr=subprocess.DEVNULL)
    sha = hashlib.sha256(data).hexdigest()
    print(f"{jar}: size={len(data)}, sha256={sha}")
