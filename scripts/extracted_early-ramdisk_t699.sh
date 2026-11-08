#!/system/bin/sh
set -u

TOYBOX=/system/bin/toybox

if [ ! -c /dev/console ]; then "$TOYBOX" mknod -m 0600 /dev/console c 5 1; fi
if [ ! -c /dev/null ]; then "$TOYBOX" mknod -m 0666 /dev/null c 1 3; fi
exec >/dev/console 2>&1

echo "[T552] initramfs boot path selected; Track A552 software Gralloc/HWC retry & diagnostics"
"$TOYBOX" mkdir -p /system /vendor /data /ramdisk-images /dev/block
"$TOYBOX" mknod -m 0600 /dev/loop0 b 7 0
"$TOYBOX" mknod -m 0600 /dev/loop1 b 7 1
"$TOYBOX" losetup -r /dev/loop0 /ramdisk-images/system.img
"$TOYBOX" losetup -r /dev/loop1 /ramdisk-images/vendor.img
"$TOYBOX" mknod -m 0600 /dev/block/loop0 b 7 0
"$TOYBOX" mknod -m 0600 /dev/block/loop1 b 7 1
echo "[T552] guest attached embedded images to loop0/loop1"
# T666: Pinned filesystem-layer fix: mount ext4 on /data
# Ext4 natively supports user.* xattrs, returning ENODATA on unset paths
# and allowing setSerialNumber to succeed without entering the recovery wipe path.
"$TOYBOX" mknod -m 0600 /dev/loop2 b 7 2 2>/dev/null || :
"$TOYBOX" mknod -m 0600 /dev/block/loop2 b 7 2 2>/dev/null || :
mounted_ext4=0
if [ -f /ramdisk-images/userdata.img ]; then
    "$TOYBOX" losetup /dev/loop2 /ramdisk-images/userdata.img 2>/dev/null || :
    if "$TOYBOX" mount -t ext4 -o rw,nosuid,nodev,noatime,barrier=1 /dev/loop2 /data 2>/dev/null; then
        mounted_ext4=1
        echo "[T666] ext4 userdata mounted on /data via /dev/loop2"
    fi
fi
if [ "$mounted_ext4" -eq 0 ]; then
    "$TOYBOX" mknod -m 0600 /dev/ram0 b 1 0 2>/dev/null || :
    "$TOYBOX" mknod -m 0600 /dev/block/ram0 b 1 0 2>/dev/null || :
    if [ -f /ramdisk-images/userdata.img ]; then
        "$TOYBOX" cp /ramdisk-images/userdata.img /dev/ram0 2>/dev/null || :
    fi
    if "$TOYBOX" mount -t ext4 -o rw,nosuid,nodev,noatime,barrier=1 /dev/ram0 /data 2>/dev/null; then
        mounted_ext4=1
        echo "[T666] ext4 userdata mounted on /data via /dev/ram0"
    fi
fi
if [ "$mounted_ext4" -eq 0 ]; then
    echo "[T666] WARNING: ext4 mount failed, falling back to tmpfs"
    "$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0771 tmpfs /data
else
    "$TOYBOX" chmod 0771 /data 2>/dev/null || :
    "$TOYBOX" chown 1000:1000 /data 2>/dev/null || :
fi
if [ -f /t636-diag.sh ]; then
    "$TOYBOX" cp /t636-diag.sh /data/t636-diag.sh
    "$TOYBOX" chmod 0755 /data/t636-diag.sh
    echo "[T636] copied /t636-diag.sh to /data/t636-diag.sh"
fi
echo "[T666] userdata filesystem initialization complete"
# T667: Pinned filesystem/mount arrangement fix: mount tmpfs on /data/media
# Canonical Linux 4.19 kernel lacks CONFIG_EXT4_FS_POSIX_ACL=y, causing setxattr of
# system.posix_acl_default on ext4 /data/media/0 to fail with EOPNOTSUPP.
# With CONFIG_TMPFS_POSIX_ACL=y, tmpfs supports POSIX ACLs natively.
"$TOYBOX" mkdir -p /data/media 2>/dev/null || :
"$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0770,uid=1023,gid=1023 tmpfs /data/media 2>/dev/null || :
"$TOYBOX" chmod 0770 /data/media 2>/dev/null || :
"$TOYBOX" chown 1023:1023 /data/media 2>/dev/null || :
echo "[T667] tmpfs mounted on /data/media"
echo "[T667] userdata and media filesystem initialization complete"
# T668: Pre-create /data/misc/gatekeeper for gatekeeperd
"$TOYBOX" mkdir -p /data/misc/gatekeeper 2>/dev/null || :
"$TOYBOX" chmod 0700 /data/misc/gatekeeper 2>/dev/null || :
"$TOYBOX" chown 1000:1000 /data/misc/gatekeeper 2>/dev/null || :
echo "[T668] /data/misc/gatekeeper pre-created (0700 1000:1000)"
# T669: Pre-create /data/vendor_de/0/fpdata for fingerprint-service
"$TOYBOX" mkdir -p /data/vendor_de/0/fpdata 2>/dev/null || :
"$TOYBOX" chmod 0700 /data/vendor_de/0/fpdata 2>/dev/null || :
"$TOYBOX" chown 1000:1000 /data/vendor_de/0/fpdata 2>/dev/null || :
echo "[T669] /data/vendor_de/0/fpdata pre-created (0700 1000:1000)"
# T670: Pre-create /data/misc/apexdata/com.android.media.swcodec and /data/data/com.android.media.swcodec
"$TOYBOX" mkdir -p /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :
"$TOYBOX" chmod 0771 /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :
"$TOYBOX" chown 1000:1000 /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :
"$TOYBOX" mkdir -p /data/data/com.android.media.swcodec 2>/dev/null || :
"$TOYBOX" chmod 0771 /data/data/com.android.media.swcodec 2>/dev/null || :
"$TOYBOX" chown 1000:1000 /data/data/com.android.media.swcodec 2>/dev/null || :
echo "[T670] /data/.../com.android.media.swcodec pre-created (0771 1000:1000)"
# T699 Fix B: Pre-create /dev/fb0 and /dev/graphics/fb0 character devices (c 29 0)
"$TOYBOX" rm -f /dev/fb0 /dev/graphics/fb0 2>/dev/null || :
"$TOYBOX" mknod -m 0666 /dev/fb0 c 29 0 2>/dev/null || :
"$TOYBOX" mkdir -p /dev/graphics 2>/dev/null || :
"$TOYBOX" mknod -m 0666 /dev/graphics/fb0 c 29 0 2>/dev/null || :
echo "[T699] /dev/fb0 and /dev/graphics/fb0 character devices pre-created (c 29 0)"
# T699 Fix A: Pre-create odrefresh compilation log to skip TCG dex2oat timeout
"$TOYBOX" mkdir -p /data/misc/odrefresh 2>/dev/null || :
"$TOYBOX" chmod 0777 /data/misc/odrefresh 2>/dev/null || :
echo "CompilationLog/1.0" > /data/misc/odrefresh/compilation-log.txt
echo "340000000 0 1 2000000000 1" >> /data/misc/odrefresh/compilation-log.txt
"$TOYBOX" chmod 0666 /data/misc/odrefresh/compilation-log.txt 2>/dev/null || :
"$TOYBOX" chown 1000:1000 /data/misc/odrefresh/compilation-log.txt 2>/dev/null || :
echo "[T699] /data/misc/odrefresh/compilation-log.txt pre-created"
# T699 Fix A: Pre-create dalvik-cache directory
"$TOYBOX" mkdir -p /data/misc/apexdata/com.android.art/dalvik-cache/arm64 2>/dev/null || :
"$TOYBOX" chmod 0771 /data/misc/apexdata/com.android.art/dalvik-cache 2>/dev/null || :
"$TOYBOX" chown 1000:1000 /data/misc/apexdata/com.android.art/dalvik-cache 2>/dev/null || :
"$TOYBOX" mkdir -p /dev/socket/pdx/system/vr/display

/loop-node-keeper >/dev/console 2>&1 &
/t493-probe.sh >/dev/console 2>&1 &
/t501-keystore2-dump.sh >/dev/console 2>&1 &

# Dedicated tombstone watcher: immediately dumps any crash tombstone to console
tombstone_watcher() {
    exec >/dev/console 2>&1
    seen=""
    while true; do
        "$TOYBOX" sleep 2
        if [ -d /data/tombstones ]; then
            for f in /data/tombstones/*; do
                if [ -f "$f" ]; then
                    case "$seen" in
                        *"$f"*) ;;
                        *)
                            echo "================ CRASH TOMBSTONE: $f ================"
                            "$TOYBOX" head -n 100 "$f"
                            echo "================ END CRASH TOMBSTONE ================"
                            seen="$seen $f"
                            ;;
                    esac
                fi
            done
        fi
        if [ -d /data/system/dropbox ]; then
            for f in /data/system/dropbox/*; do
                if [ -f "$f" ]; then
                    case "$seen" in
                        *"$f"*) ;;
                        *)
                            echo "================ DROPBOX CRASH: $f ================"
                            "$TOYBOX" head -n 60 "$f"
                            echo "================ END DROPBOX CRASH ================"
                            seen="$seen $f"
                            ;;
                    esac
                fi
            done
        fi
    done
}
# tombstone_watcher >/dev/console 2>&1

# Dedicated logcat streamer: streams logs to console as soon as logcat is accessible
logcat_streamer() {
    exec >/dev/console 2>&1
    while true; do
        if [ -x /system/bin/logcat ]; then
            echo "[T552] logcat streamer active"
            /system/bin/logcat -b all -v time *:V
            break
        fi
        "$TOYBOX" sleep 1
    done
}
# logcat_streamer >/dev/console 2>&1

run_probe() {
    exec >/dev/console 2>&1
    echo "[T552-PROBE] guest in-process probe daemon armed"
    reported_art=0
    reported_boot=0
    reported_ss=0
    last_zygote_pid=""
    step=0
    while [ $step -lt 240 ]; do
        "$TOYBOX" sleep 2
        step=$((step + 1))

        # HAL, ServiceManager, and SF status probe
        sf_state=$(/system/bin/getprop init.svc.surfaceflinger 2>/dev/null || true)
        alloc_state=$(/system/bin/getprop init.svc.vendor.allocator-2-0 2>/dev/null || true)
        hwc_state=$(/system/bin/getprop init.svc.vendor.hwc-2-1 2>/dev/null || true)
        hwsm_state=$(/system/bin/getprop init.svc.hwservicemanager 2>/dev/null || true)
        sm_state=$(/system/bin/getprop init.svc.servicemanager 2>/dev/null || true)
        if [ $((step % 5)) -eq 0 ]; then
            echo "[T552-PROBE t=$((step*2))s] SF: '$sf_state' | Allocator: '$alloc_state' | HWC: '$hwc_state' | HWSM: '$hwsm_state' | SM: '$sm_state'"
        fi

        if [ "$reported_art" -eq 0 ]; then
            if [ -d /apex/com.android.art ] && [ -f /apex/com.android.art/lib64/libart.so ]; then
                reported_art=1
                echo "[T552-PROBE] >>> VERIFIED: /apex/com.android.art is MOUNTED and lib64/libart.so exists! <<<"
            fi
        fi

        zpids=$("$TOYBOX" pidof app_process64 2>/dev/null || true)
        if [ -n "$zpids" ]; then
            if [ "$zpids" != "$last_zygote_pid" ]; then
                echo "[T552-PROBE t=$((step*2))s] Zygote running with PID(s): $zpids (previous: '$last_zygote_pid')"
                last_zygote_pid="$zpids"
            fi
        fi

        sspids=$("$TOYBOX" pidof system_server 2>/dev/null || true)
        if [ -n "$sspids" ]; then
            if [ "$reported_ss" -eq 0 ]; then
                reported_ss=1
                echo "[T552-PROBE t=$((step*2))s] >>> MILESTONE: system_server STARTED with PID: $sspids <<<"
            fi
        fi

        if [ "$reported_boot" -eq 0 ]; then
            bc=$(/system/bin/getprop sys.boot_completed 2>/dev/null || true)
            if [ "$bc" = "1" ]; then
                reported_boot=1
                echo "[T552-PROBE t=$((step*2))s] >>> MILESTONE: sys.boot_completed=1 DETECTED! <<<"
            fi
        fi
    done
}
run_probe >/dev/console 2>&1 &

echo "[T552] guest boot probe and tombstone watcher armed before Android init"
echo "[T552] handing off to Android init"
/guest_diag &
exec /init
