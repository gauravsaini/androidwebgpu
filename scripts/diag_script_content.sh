#!/system/bin/sh
KMSG=/dev/kmsg
TOY=/system/bin/toybox

# Ensure device nodes exist
if [ ! -c /dev/null ]; then
    /system/bin/toybox mknod -m 0666 /dev/null c 1 3 2>/dev/null || true
fi
if [ ! -c /dev/kmsg ]; then
    /system/bin/toybox mknod -m 0666 /dev/kmsg c 1 11 2>/dev/null || true
fi

# Wait for toybox to be available
while [ ! -x "$TOY" ]; do
    sleep 0.5 2>/dev/null || :
done

mark() {
    if [ -c "$KMSG" ]; then
        echo "$*" > "$KMSG" 2>/dev/null || :
    fi
    if [ -c /dev/console ]; then
        echo "$*" > /dev/console 2>/dev/null || :
    fi
}

BOOTCLASSPATH_31_APEX="/apex/com.android.art/javalib/core-oj.jar:/apex/com.android.art/javalib/core-libart.jar:/apex/com.android.art/javalib/okhttp.jar:/apex/com.android.art/javalib/bouncycastle.jar:/apex/com.android.art/javalib/apache-xml.jar:/system/framework/framework.jar:/system/framework/framework-graphics.jar:/system/framework/ext.jar:/system/framework/telephony-common.jar:/system/framework/voip-common.jar:/system/framework/ims-common.jar:/apex/com.android.i18n/javalib/core-icu4j.jar:/apex/com.android.btservices/javalib/framework-bluetooth.jar:/apex/com.android.conscrypt/javalib/conscrypt.jar:/apex/com.android.devicelock/javalib/framework-devicelock.jar:/apex/com.android.healthfitness/javalib/framework-healthfitness.jar:/apex/com.android.os.statsd/javalib/framework-statsd.jar:/apex/com.android.sdkext/javalib/framework-sdkextensions.jar:/apex/com.android.virt/javalib/framework-virtualization.jar:/apex/com.android.tethering/javalib/framework-connectivity.jar:/apex/com.android.tethering/javalib/framework-tethering.jar:/apex/com.android.tethering/javalib/framework-connectivity-t.jar:/apex/com.android.appsearch/javalib/framework-appsearch.jar:/apex/com.android.wifi/javalib/framework-wifi.jar:/apex/com.android.media/javalib/updatable-media.jar:/apex/com.android.permission/javalib/framework-permission-s.jar:/apex/com.android.scheduling/javalib/framework-scheduling.jar:/apex/com.android.adservices/javalib/framework-sdksandbox.jar:/apex/com.android.adservices/javalib/framework-adservices.jar:/apex/com.android.uwb/javalib/framework-uwb.jar:/apex/com.android.ondevicepersonalization/javalib/framework-ondevicepersonalization.jar"

do_prep() {
    mark "T642_DO_PREP_ENTER pid=$$"
    # 1. Bind-mount flattened APEXes into /apex/<pkg>
    for base in /system/system_ext/apex /system_ext/apex; do
        [ -d "$base" ] || continue
        for d in "$base"/com.android.*; do
            [ -d "$d/javalib" ] || continue
            pkg=${d##*/}
            if [ ! -d "/apex/$pkg/javalib" ]; then
                "$TOY" mkdir -p "/apex/$pkg"
                "$TOY" mount -o bind "$d" "/apex/$pkg"
                mark "T642_BIND_MOUNTED pkg=$pkg from=$d"
            fi
        done
    done

    # 2. Derive classpath if derive_classpath tool exists
    if [ -x /apex/com.android.sdkext/bin/derive_classpath ]; then
        /apex/com.android.sdkext/bin/derive_classpath /data/system/environ/classpath 2>/dev/null || :
    fi

    # 3. Ensure /data/system/environ/classpath has exact 31-jar BOOTCLASSPATH using /apex/...
    "$TOY" mkdir -p /data/system/environ
    if [ -f /data/system/environ/classpath ]; then
        if ! "$TOY" grep -q "framework-tethering.jar" /data/system/environ/classpath; then
            "$TOY" sed -i "s|^export BOOTCLASSPATH .*|export BOOTCLASSPATH $BOOTCLASSPATH_31_APEX|" /data/system/environ/classpath
            mark "T642_CLASSPATH_FILE_UPDATED"
        fi
    else
        echo "export BOOTCLASSPATH $BOOTCLASSPATH_31_APEX" > /data/system/environ/classpath
        mark "T642_CLASSPATH_FILE_CREATED"
    fi

    # 4. Verify connectivity jar is accessible via /apex/
    if [ -f /apex/com.android.tethering/javalib/framework-connectivity.jar ]; then
        mark "T642_APEX_CONNECTIVITY_JAR_OK"
    else
        mark "T642_APEX_CONNECTIVITY_JAR_MISSING"
    fi

    mark "T642_DO_PREP_EXIT pid=$$"
}

# Mode handling: prep vs background daemon
MODE="${1:-bg}"

if [ "$MODE" = "prep" ]; then
    mark "T642_PREP_MODE_STARTED version=2 pid=$$"
    do_prep
    mark "T642_PREP_MODE_COMPLETE"
    exit 0
fi

# Background monitoring daemon
# Prevent multiple instances
if [ -f /data/t636-diag.pid ]; then
    oldpid=$("$TOY" cat /data/t636-diag.pid 2>/dev/null)
    if [ -n "$oldpid" ] && [ -d "/proc/$oldpid" ]; then
        exit 0
    fi
fi
echo $$ > /data/t636-diag.pid 2>/dev/null || :

mark "T642_DIAGNOSTICS_STARTED version=2 pid=$$"

# Always ensure prep is done on startup
do_prep

# 1. Start full logcat streaming to kmsg with restart loop
start_logcat() {
    while [ ! -x /system/bin/logcat ]; do
        "$TOY" sleep 0.5 2>/dev/null || :
    done
    mark T642_LOGCAT_STREAMER_STARTED
    while :; do
        /system/bin/logcat -b all -v threadtime >> "$KMSG" 2>&1
        "$TOY" sleep 1 2>/dev/null || :
    done
}
start_logcat &

# 2. Check flattened tethering jar and apex jars on disk
if [ -f /system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar ]; then
    mark T642_FLATTENED_TETHERING_JAR_EXISTS
    "$TOY" ls -la /system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar > "$KMSG" 2>&1
else
    mark T642_FLATTENED_TETHERING_JAR_MISSING
fi
if [ -f /apex/com.android.tethering/javalib/framework-connectivity.jar ]; then
    mark T642_APEX_CONNECTIVITY_JAR_EXISTS
    "$TOY" ls -la /apex/com.android.tethering/javalib/framework-connectivity.jar > "$KMSG" 2>&1
fi

# 3. Watch /data/system/environ/classpath and ensure exact 31-jar BOOTCLASSPATH
watch_classpath_source() {
    updated=0
    while [ $updated -eq 0 ]; do
        if [ -f /data/system/environ/classpath ]; then
            mark T642_SOURCE_BOOTCLASSPATH_BEGIN
            "$TOY" grep '^export BOOTCLASSPATH ' /data/system/environ/classpath | "$TOY" sed 's/^export BOOTCLASSPATH //' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                [ -n "$item" ] && mark "T642_SOURCE_BCP_ITEM=$item"
            done
            mark T642_SOURCE_BOOTCLASSPATH_END
            if ! "$TOY" grep -q "framework-tethering.jar" /data/system/environ/classpath; then
                mark T642_ENFORCING_31_JAR_BOOTCLASSPATH
                "$TOY" sed -i "s|^export BOOTCLASSPATH .*|export BOOTCLASSPATH $BOOTCLASSPATH_31_APEX|" /data/system/environ/classpath
                mark T642_DATA_SYSTEM_ENVIRON_CLASSPATH_ENFORCED
            elif "$TOY" grep -q "/system/system_ext/apex/" /data/system/environ/classpath; then
                mark T642_REPLACING_LEGACY_SYSTEM_EXT_APEX_PATHS
                "$TOY" sed -i "s|^export BOOTCLASSPATH .*|export BOOTCLASSPATH $BOOTCLASSPATH_31_APEX|" /data/system/environ/classpath
                mark T642_DATA_SYSTEM_ENVIRON_CLASSPATH_PURIFIED
            fi
            updated=1
        fi
        "$TOY" sleep 0.5 2>/dev/null || :
    done
}
watch_classpath_source &

# 4. Watcher for Zygote PID and /proc/<pid>/environ BOOTCLASSPATH verbatim
watch_zygote_environ() {
    mark T642_ZYGOTE_WATCHER_STARTED
    seen_pids=""
    while :; do
        for proc in /proc/[0-9]*; do
            [ -d "$proc" ] || continue
            pid=$("$TOY" basename "$proc" 2>/dev/null)
            [ "$pid" = "$$" ] && continue
            case "$seen_pids" in
                *" $pid "*) continue ;;
            esac
            [ -r "$proc/cmdline" ] || continue
            cmd=$("$TOY" tr '\000' ' ' < "$proc/cmdline" 2>/dev/null)
            case "$cmd" in
                *logcat*|*toybox*|*grep*|*sleep*|*t636*|*t641*|*t642*) continue ;;
            esac
            comm=$("$TOY" cat "$proc/comm" 2>/dev/null)
            is_zygote=0
            if [ "$comm" = "zygote64" ] || [ "$comm" = "zygote" ] || [ "$comm" = "main" ] || [ "$comm" = "app_process64" ]; then
                is_zygote=1
            elif echo "$cmd" | "$TOY" grep -q "app_process.*zygote"; then
                is_zygote=1
            fi
            if [ $is_zygote -eq 1 ]; then
                seen_pids="$seen_pids $pid "
                mark "T642_ZYGOTE_PROCESS_FOUND pid=$pid comm=$comm cmd=$cmd"
                if [ -r "$proc/environ" ]; then
                    mark "T642_ZYGOTE_ENVIRON_BEGIN pid=$pid"
                    raw_env=$("$TOY" tr '\000' ' ' < "$proc/environ" 2>/dev/null)
                    mark "T642_ZYGOTE_ENVIRON_RAW pid=$pid $raw_env"
                    bcp=$("$TOY" tr '\000' '\n' < "$proc/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                    if [ -n "$bcp" ]; then
                        mark "T642_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$pid $bcp"
                        printf '%s\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                            [ -n "$item" ] && mark "T642_ZYGOTE_BCP_ITEM pid=$pid item=$item"
                        done
                    else
                        mark "T642_ZYGOTE_BOOTCLASSPATH_MISSING pid=$pid"
                    fi
                    mark "T642_ZYGOTE_ENVIRON_END pid=$pid"
                else
                    mark "T642_ZYGOTE_ENVIRON_UNREADABLE pid=$pid"
                fi
            fi
        done
        for zprop in init.svc_debug_pid.zygote init.svc_debug_pid.zygote64; do
            zypid=$(/system/bin/getprop "$zprop" 2>/dev/null)
            if [ -n "$zypid" ] && [ -d "/proc/$zypid" ]; then
                case "$seen_pids" in
                    *" $zypid "*) ;;
                    *)
                        seen_pids="$seen_pids $zypid "
                        mark "T642_ZYGOTE_PROP_FOUND pid=$zypid prop=$zprop"
                        if [ -r "/proc/$zypid/environ" ]; then
                            mark "T642_ZYGOTE_ENVIRON_BEGIN pid=$zypid"
                            bcp=$("$TOY" tr '\000' '\n' < "/proc/$zypid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T642_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$zypid $bcp"
                            printf '%s\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                                [ -n "$item" ] && mark "T642_ZYGOTE_BCP_ITEM pid=$zypid item=$item"
                            done
                            mark "T642_ZYGOTE_ENVIRON_END pid=$zypid"
                        fi
                        ;;
                esac
            fi
        done
        "$TOY" sleep 1 2>/dev/null || :
    done
}
watch_zygote_environ &

# 5. Watcher for system_server
watch_system_server() {
    mark T642_SYSTEM_SERVER_WATCHER_STARTED
    seen_ss_pids=""
    while :; do
        ss_prop=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        sspids=$("$TOY" pidof system_server 2>/dev/null)
        if [ -n "$sspids" ]; then
            for sspid in $sspids; do
                case "$seen_ss_pids" in
                    *" $sspid "*) ;;
                    *)
                        seen_ss_pids="$seen_ss_pids $sspid "
                        cmd=$("$TOY" tr '\000' ' ' < "/proc/$sspid/cmdline" 2>/dev/null)
                        comm=$("$TOY" cat "/proc/$sspid/comm" 2>/dev/null)
                        mark "T642_SYSTEM_SERVER_STARTED pid=$sspid comm=$comm cmd=$cmd prop=$ss_prop"
                        if [ -r "/proc/$sspid/environ" ]; then
                            bcp=$("$TOY" tr '\000' '\n' < "/proc/$sspid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T642_SYSTEM_SERVER_BOOTCLASSPATH pid=$sspid $bcp"
                        fi
                        if [ -r "/proc/$sspid/status" ]; then
                            st=$("$TOY" head -n 5 "/proc/$sspid/status" 2>/dev/null | "$TOY" tr '\n' ';')
                            mark "T642_SYSTEM_SERVER_STATUS pid=$sspid $st"
                        fi
                        ;;
                esac
            done
        elif [ -n "$ss_prop" ]; then
            mark "T642_SYSTEM_SERVER_PROP_ONLY prop=$ss_prop"
        fi
        "$TOY" sleep 0.5 2>/dev/null || :
    done
}
watch_system_server &

# 6. Boot state watcher
watch_boot_state() {
    mark T642_BOOT_STATE_WATCHER_STARTED
    while :; do
        uptime=$("$TOY" cut -d ' ' -f 1 /proc/uptime 2>/dev/null)
        ss=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        zy=$(/system/bin/getprop init.svc.zygote 2>/dev/null)
        [ -z "$zy" ] && zy=$(/system/bin/getprop init.svc.zygote64 2>/dev/null)
        zypid=$(/system/bin/getprop init.svc_debug_pid.zygote 2>/dev/null)
        [ -z "$zypid" ] && zypid=$(/system/bin/getprop init.svc_debug_pid.zygote64 2>/dev/null)
        sspids=$("$TOY" pidof system_server 2>/dev/null)
        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)
        mark "T642_BOOT_STATE uptime=$uptime system_server_prop=$ss system_server_pids=$sspids zygote=$zy zypid=$zypid boot_completed=$completed"
        "$TOY" sleep 10 2>/dev/null || :
    done
}
watch_boot_state &

while :; do "$TOY" sleep 60 2>/dev/null || :; done
