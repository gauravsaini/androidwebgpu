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

# Prevent multiple instances
if [ -f /data/t636-diag.pid ]; then
    oldpid=$("$TOY" cat /data/t636-diag.pid 2>/dev/null)
    if [ -n "$oldpid" ] && [ -d "/proc/$oldpid" ]; then
        exit 0
    fi
fi
echo $$ > /data/t636-diag.pid 2>/dev/null || :

mark() {
    if [ -c "$KMSG" ]; then
        echo "$*" > "$KMSG" 2>/dev/null || :
    fi
    if [ -c /dev/console ]; then
        echo "$*" > /dev/console 2>/dev/null || :
    fi
}

mark "T641_DIAGNOSTICS_STARTED version=1 pid=$$"

# 1. Start full logcat streaming to kmsg with restart loop
start_logcat() {
    while [ ! -x /system/bin/logcat ]; do
        "$TOY" sleep 0.5 2>/dev/null || :
    done
    mark T641_LOGCAT_STREAMER_STARTED
    while :; do
        /system/bin/logcat -b all -v threadtime >> "$KMSG" 2>&1
        "$TOY" sleep 1 2>/dev/null || :
    done
}
start_logcat &

# 2. Check flattened tethering jar on disk
if [ -f /system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar ]; then
    mark T641_FLATTENED_TETHERING_JAR_EXISTS
    "$TOY" ls -la /system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar > "$KMSG" 2>&1
else
    mark T641_FLATTENED_TETHERING_JAR_MISSING
fi

# 3. Watch /data/system/environ/classpath and ensure framework-tethering.jar is present
watch_classpath_source() {
    updated=0
    while [ $updated -eq 0 ]; do
        if [ -f /data/system/environ/classpath ]; then
            mark T641_SOURCE_BOOTCLASSPATH_BEGIN
            "$TOY" grep '^export BOOTCLASSPATH ' /data/system/environ/classpath | "$TOY" sed 's/^export BOOTCLASSPATH //' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                [ -n "$item" ] && mark "T641_SOURCE_BCP_ITEM=$item"
            done
            mark T641_SOURCE_BOOTCLASSPATH_END
            if ! "$TOY" grep -q "framework-tethering.jar" /data/system/environ/classpath; then
                EXTRA_JARS="/system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar:/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity-t.jar:/system/system_ext/apex/com.android.appsearch/javalib/framework-appsearch.jar:/system/system_ext/apex/com.android.wifi/javalib/framework-wifi.jar:/system/system_ext/apex/com.android.media/javalib/updatable-media.jar:/system/system_ext/apex/com.android.permission/javalib/framework-permission-s.jar:/system/system_ext/apex/com.android.scheduling/javalib/framework-scheduling.jar:/system/system_ext/apex/com.android.adservices/javalib/framework-sdksandbox.jar:/system/system_ext/apex/com.android.adservices/javalib/framework-adservices.jar:/system/system_ext/apex/com.android.uwb/javalib/framework-uwb.jar:/system/system_ext/apex/com.android.ondevicepersonalization/javalib/framework-ondevicepersonalization.jar"
                "$TOY" sed -i "s|export BOOTCLASSPATH .*|&:$EXTRA_JARS|" /data/system/environ/classpath
                mark T641_DATA_SYSTEM_ENVIRON_CLASSPATH_UPDATED
                mark T641_UPDATED_BOOTCLASSPATH_BEGIN
                "$TOY" grep '^export BOOTCLASSPATH ' /data/system/environ/classpath | "$TOY" sed 's/^export BOOTCLASSPATH //' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                    [ -n "$item" ] && mark "T641_UPDATED_BCP_ITEM=$item"
                done
                mark T641_UPDATED_BOOTCLASSPATH_END
            fi
            updated=1
        fi
        "$TOY" sleep 0.5 2>/dev/null || :
    done
}
watch_classpath_source &

# 4. Watcher for Zygote PID and /proc/<pid>/environ BOOTCLASSPATH verbatim
watch_zygote_environ() {
    mark T641_ZYGOTE_WATCHER_STARTED
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
                *logcat*|*toybox*|*grep*|*sleep*|*t636*|*t641*) continue ;;
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
                mark "T641_ZYGOTE_PROCESS_FOUND pid=$pid comm=$comm cmd=$cmd"
                if [ -r "$proc/environ" ]; then
                    mark "T641_ZYGOTE_ENVIRON_BEGIN pid=$pid"
                    raw_env=$("$TOY" tr '\000' ' ' < "$proc/environ" 2>/dev/null)
                    mark "T641_ZYGOTE_ENVIRON_RAW pid=$pid $raw_env"
                    bcp=$("$TOY" tr '\000' '\n' < "$proc/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                    if [ -n "$bcp" ]; then
                        mark "T641_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$pid $bcp"
                        printf '%s\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                            [ -n "$item" ] && mark "T641_ZYGOTE_BCP_ITEM pid=$pid item=$item"
                        done
                    else
                        mark "T641_ZYGOTE_BOOTCLASSPATH_MISSING pid=$pid"
                    fi
                    mark "T641_ZYGOTE_ENVIRON_END pid=$pid"
                else
                    mark "T641_ZYGOTE_ENVIRON_UNREADABLE pid=$pid"
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
                        mark "T641_ZYGOTE_PROP_FOUND pid=$zypid prop=$zprop"
                        if [ -r "/proc/$zypid/environ" ]; then
                            mark "T641_ZYGOTE_ENVIRON_BEGIN pid=$zypid"
                            bcp=$("$TOY" tr '\000' '\n' < "/proc/$zypid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T641_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$zypid $bcp"
                            printf '%s\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\n' | while IFS= read -r item; do
                                [ -n "$item" ] && mark "T641_ZYGOTE_BCP_ITEM pid=$zypid item=$item"
                            done
                            mark "T641_ZYGOTE_ENVIRON_END pid=$zypid"
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
    mark T641_SYSTEM_SERVER_WATCHER_STARTED
    seen_ss=0
    while :; do
        ss_prop=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        sspids=$("$TOY" pidof system_server 2>/dev/null)
        if [ -n "$sspids" ] || [ "$ss_prop" = "running" ]; then
            if [ $seen_ss -eq 0 ]; then
                seen_ss=1
                mark "T641_SYSTEM_SERVER_STARTED pid=$sspids prop=$ss_prop"
                if [ -n "$sspids" ]; then
                    for sspid in $sspids; do
                        if [ -r "/proc/$sspid/environ" ]; then
                            bcp=$("$TOY" tr '\000' '\n' < "/proc/$sspid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T641_SYSTEM_SERVER_BOOTCLASSPATH pid=$sspid $bcp"
                        fi
                    done
                fi
            fi
        fi
        "$TOY" sleep 1 2>/dev/null || :
    done
}
watch_system_server &

# 6. Boot state watcher
watch_boot_state() {
    mark T641_BOOT_STATE_WATCHER_STARTED
    while :; do
        uptime=$("$TOY" cut -d ' ' -f 1 /proc/uptime 2>/dev/null)
        ss=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        zy=$(/system/bin/getprop init.svc.zygote 2>/dev/null)
        [ -z "$zy" ] && zy=$(/system/bin/getprop init.svc.zygote64 2>/dev/null)
        zypid=$(/system/bin/getprop init.svc_debug_pid.zygote 2>/dev/null)
        [ -z "$zypid" ] && zypid=$(/system/bin/getprop init.svc_debug_pid.zygote64 2>/dev/null)
        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)
        mark "T641_BOOT_STATE uptime=$uptime system_server=$ss zygote=$zy zypid=$zypid boot_completed=$completed"
        "$TOY" sleep 10 2>/dev/null || :
    done
}
watch_boot_state &

while :; do "$TOY" sleep 60 2>/dev/null || :; done
