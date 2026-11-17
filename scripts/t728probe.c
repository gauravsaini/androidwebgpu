/* T728 observable-payload helper.
 *
 * Minimal freestanding static aarch64 binary, no libc, _start entry.
 * Adapted from the T723 probe (guest-inputs/t723-probe.c): same raw
 * syscall wrappers, string helpers and kmsg line builder; the EGL
 * sub-probes, ELF mini-loader and uid-drop are NOT needed here and
 * are omitted. The service runs as root (see t728probe.rc), so no
 * privilege drop.
 *
 * Behaviour: dump received argc/argv to /dev/kmsg (PRIMARY evidence
 * channel — the UART) and to /data/t728-argv.txt, then write
 * T728_SERVICE_OK to /dev/kmsg and exit 0.
 *
 * If /dev/kmsg cannot be opened, lines still go to stdout (fd 1);
 * in the guest a service's stdout is /dev/null, so a missing kmsg
 * open means the marker cannot appear — the flight verdict (FAIL)
 * records that. The /data file open failure is recorded, not fatal.
 */
typedef unsigned int u32;
typedef int s32;
typedef unsigned long long u64;
typedef long slong;
typedef unsigned long ulong;

#define NR_openat 56
#define NR_close 57
#define NR_read 63
#define NR_write 64
#define NR_ioctl 29
#define NR_exit_group 94
#define NR_getuid 174
#define NR_geteuid 175
#define NR_getgid 176
#define NR_getegid 177

#define AT_FDCWD (-100)
#define O_RDONLY 0
#define O_WRONLY 1
#define O_CREAT 0100
#define O_TRUNC 01000

static slong sys1(slong nr, slong a0) {
    register slong x0 __asm__("x0") = a0;
    register slong x8 __asm__("x8") = nr;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x8) : "memory", "cc");
    return x0;
}
static slong sys3(slong nr, slong a0, slong a1, slong a2) {
    register slong x0 __asm__("x0") = a0;
    register slong x1 __asm__("x1") = a1;
    register slong x2 __asm__("x2") = a2;
    register slong x8 __asm__("x8") = nr;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory", "cc");
    return x0;
}
static slong sys4(slong nr, slong a0, slong a1, slong a2, slong a3) {
    register slong x0 __asm__("x0") = a0;
    register slong x1 __asm__("x1") = a1;
    register slong x2 __asm__("x2") = a2;
    register slong x3 __asm__("x3") = a3;
    register slong x8 __asm__("x8") = nr;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x3), "r"(x8) : "memory", "cc");
    return x0;
}

static ulong slen(const char *s) {
    ulong n = 0;
    while (s[n]) n++;
    return n;
}
static void zbuf(char *b, ulong n) {
    ulong i;
    for (i = 0; i < n; i++) b[i] = 0;
}
static ulong fmt_long(char *buf, slong v) {
    char tmp[24];
    ulong n = 0, i, j = 0;
    int neg = 0;
    if (v < 0) { neg = 1; v = -v; }
    if (v == 0) tmp[n++] = 48;
    while (v > 0) { tmp[n++] = (char)(48 + (v % 10)); v /= 10; }
    if (neg) buf[j++] = 45;
    for (i = 0; i < n; i++) buf[j++] = tmp[n - 1 - i];
    return j;
}

static int kmsg_fd = -1;
static int data_fd = -1;
static void emit_raw(const char *s, ulong n) {
    sys3(NR_write, 1, (slong)s, (slong)n);
    if (kmsg_fd >= 0) sys3(NR_write, kmsg_fd, (slong)s, (slong)n);
    if (data_fd >= 0) sys3(NR_write, data_fd, (slong)s, (slong)n);
}
static void emit(const char *s) {
    emit_raw(s, slen(s));
    emit_raw("\n", 1);
}
/* Line builder: lb_reset/lb_s/lb_n/lb_emit. */
static char lb[512];
static ulong lbn;
static void lb_reset(void) { lbn = 0; }
static void lb_s(const char *s) {
    ulong i;
    for (i = 0; s[i] && lbn < 500; i++) lb[lbn++] = s[i];
}
static void lb_n(slong v) { lbn += fmt_long(lb + lbn, v); }
static void lb_emit(void) {
    emit_raw(lb, lbn);
    emit_raw("\n", 1);
}

void _start(void) {
    ulong *sp;
    slong argc;
    char **argv;
    slong rc;
    long i;

    /* _start is entered with sp -> argc, then argv pointers. */
    __asm__ volatile("mov %0, sp" : "=r"(sp));
    argc = (slong)sp[0];
    argv = (char **)(sp + 1);

    kmsg_fd = (int)sys3(NR_openat, AT_FDCWD, (slong)"/dev/kmsg", O_WRONLY);
    emit("T728_PROBE: start (argv dump, direct exec)");
    lb_reset(); lb_s("T728_PROBE: kmsg_fd="); lb_n(kmsg_fd);
    lb_s(" uid="); lb_n(sys1(NR_getuid, 0));
    lb_s(" euid="); lb_n(sys1(NR_geteuid, 0));
    lb_s(" gid="); lb_n(sys1(NR_getgid, 0));
    lb_s(" egid="); lb_n(sys1(NR_getegid, 0)); lb_emit();

    if (argc < 0 || argc > 64) {
        lb_reset(); lb_s("T728_PROBE: BAD argc="); lb_n(argc); lb_emit();
        argc = 0;
    }

    data_fd = (int)sys4(NR_openat, AT_FDCWD, (slong)"/data/t728-argv.txt",
                         O_WRONLY | O_CREAT | O_TRUNC, 0644);
    lb_reset(); lb_s("T728_PROBE: data_fd="); lb_n(data_fd); lb_emit();

    lb_reset(); lb_s("T728_ARGC="); lb_n(argc); lb_emit();
    for (i = 0; i < argc; i++) {
        lb_reset(); lb_s("T728_ARGV["); lb_n(i); lb_s("]=");
        lb_s(argv[i] ? argv[i] : "(null)"); lb_emit();
    }
    lb_reset(); lb_s("T728_ARGV_DONE argc="); lb_n(argc); lb_emit();

    if (data_fd >= 0) sys1(NR_close, data_fd);
    data_fd = -1;
    emit("T728_SERVICE_OK");
    sys1(NR_exit_group, 0);
}
