#!/system/bin/sh

tombstones_dir=/data/tombstones

emit_uart() {
    echo "$*" > /dev/kmsg 2>/dev/null
}

get_uptime() {
    /system/bin/cat /proc/uptime 2>/dev/null | /system/bin/toybox awk '{print $1}'
}

uptime=$(get_uptime)
[ -n "$uptime" ] || uptime="0.0"
emit_uart "T637_COLLECTOR_SERVICE_STARTED pid=$$ guest_uptime=$uptime"

# Ensure /data/tombstones directory exists and is writable
/system/bin/toybox mkdir -p "$tombstones_dir" 2>/dev/null
/system/bin/toybox chmod 0777 "$tombstones_dir" 2>/dev/null

# Request tombstoned daemon to start if not already running
/system/bin/start tombstoned 2>/dev/null

emit_uart "T637_COLLECTOR_READY path=$tombstones_dir guest_uptime=$uptime"

# Watch for tombstone files in /data/tombstones
seen=" "
while :; do
    for tombstone in "$tombstones_dir"/tombstone_*; do
        [ -s "$tombstone" ] || continue
        name=${tombstone##*/}
        case "$name" in
            *.pb) continue ;;
            *.tmp) continue ;;
        esac
        case "$seen" in
            *" $name "*) continue ;;
        esac

        # Ensure file write is complete (size stable across 1s)
        before=$(/system/bin/toybox stat -c %s "$tombstone" 2>/dev/null)
        [ -n "$before" ] || continue
        /system/bin/sleep 1
        after=$(/system/bin/toybox stat -c %s "$tombstone" 2>/dev/null)
        [ "$before" = "$after" ] || continue

        seen="${seen}${name} "
        pid_line=$(/system/bin/toybox grep -m 1 '^pid:' "$tombstone" 2>/dev/null)
        cmdline=$(/system/bin/toybox grep -m 1 '^Cmdline:' "$tombstone" 2>/dev/null)

        emit_uart "T637_TOMBSTONE_DETECTED name=$name size=$after $pid_line $cmdline"

        # Paced dump to /dev/console (Route B proved by T635)
        cdev=/dev/console
        [ -c "$cdev" ] || cdev=/dev/ttyAMA0

        emit_uart "T637_DUMP_route_b_start cdev=$cdev path=$tombstone bytes=$after"
        if [ -c "$cdev" ]; then
            echo "T637_TOMBSTONE_BEGIN route=route_b path=$tombstone bytes=$after $pid_line $cmdline" > "$cdev" 2>/dev/null
            line_count=0
            while IFS= read -r line; do
                echo "$line" > "$cdev" 2>/dev/null
                line_count=$((line_count + 1))
                if [ $((line_count % 30)) -eq 0 ]; then
                    /system/bin/sleep 0.02
                fi
            done < "$tombstone"
            echo "T637_TOMBSTONE_END route=route_b path=$tombstone bytes=$after $pid_line $cmdline" > "$cdev" 2>/dev/null
            emit_uart "T637_DUMP_route_b=ok cdev=$cdev lines=$line_count"
        fi
    done
    /system/bin/sleep 1
done
