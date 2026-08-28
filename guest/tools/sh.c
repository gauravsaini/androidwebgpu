#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/sysmacros.h>
#include <sys/wait.h>

static int g_tty_fd = -1;

static void log_line(const char *msg) {
    if (g_tty_fd >= 0) {
        write(g_tty_fd, msg, strlen(msg));
        write(g_tty_fd, "\n", 1);
    }
    printf("%s\n", msg);
    fflush(stdout);
}

// Builtin command dispatcher
static int run_line(char *line) {
    // Strip trailing \r \n
    size_t len = strlen(line);
    while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r' || line[len - 1] == ' ' || line[len - 1] == '\t')) {
        line[--len] = '\0';
    }
    while (*line == ' ' || *line == '\t') line++;
    if (*line == '#' || *line == '\0') return 0;

    // Handle redirection e.g. > /dev/ttyS0
    char *redir = strstr(line, ">");
    if (redir) {
        char *target = redir + 1;
        while (*target == ' ' || *target == '&' || *target == '1' || *target == '2') target++;
        char target_buf[128];
        int ti = 0;
        while (target[ti] && target[ti] != ' ' && target[ti] != '&' && target[ti] != '|' && ti < 127) {
            target_buf[ti] = target[ti];
            ti++;
        }
        target_buf[ti] = '\0';
        if (g_tty_fd < 0 && ti > 0) {
            g_tty_fd = open(target_buf, O_WRONLY | O_NONBLOCK);
        }
    }

    // echo command
    char *echo_pos = strstr(line, "echo ");
    if (echo_pos) {
        const char *msg = echo_pos + 5;
        while (*msg == ' ' || *msg == '"' || *msg == '\'') msg++;
        char clean_msg[512];
        strncpy(clean_msg, msg, sizeof(clean_msg) - 1);
        clean_msg[sizeof(clean_msg) - 1] = '\0';
        char *end_r = strstr(clean_msg, ">");
        if (end_r) *end_r = '\0';
        char *end_pipe = strstr(clean_msg, "||");
        if (end_pipe) *end_pipe = '\0';
        size_t c_len = strlen(clean_msg);
        while (c_len > 0 && (clean_msg[c_len - 1] == '"' || clean_msg[c_len - 1] == '\'' || clean_msg[c_len - 1] == ' ')) clean_msg[--c_len] = '\0';
        if (c_len > 0) log_line(clean_msg);
    }

    // mount commands
    if (strstr(line, "mount ")) {
        if (strstr(line, "proc")) {
            mkdir("/proc", 0755);
            mount("proc", "/proc", "proc", 0, NULL);
        }
        if (strstr(line, "sysfs")) {
            mkdir("/sys", 0755);
            mount("sysfs", "/sys", "sysfs", 0, NULL);
        }
        if (strstr(line, "devtmpfs")) {
            mkdir("/dev", 0755);
            mount("devtmpfs", "/dev", "devtmpfs", 0, NULL);
        }
        if (strstr(line, "devpts")) {
            mkdir("/dev/pts", 0755);
            mount("devpts", "/dev/pts", "devpts", 0, NULL);
        }
        if (strstr(line, "binder")) {
            mkdir("/dev/binderfs", 0755);
            mount("binder", "/dev/binderfs", "binder", 0, NULL);
        }
        if (strstr(line, "/tmp")) {
            mkdir("/tmp", 0777);
            mount("tmpfs", "/tmp", "tmpfs", 0, NULL);
        }
        if (strstr(line, "/data")) {
            mkdir("/data", 0755);
            mount("tmpfs", "/data", "tmpfs", 0, NULL);
        }
    }

    // mkdir command
    if (strstr(line, "mkdir ")) {
        mkdir("/dev", 0755);
        mkdir("/dev/pts", 0755);
        mkdir("/dev/socket", 0755);
        mkdir("/dev/binderfs", 0755);
        mkdir("/dev/dri", 0755);
        mkdir("/data", 0755);
        mkdir("/data/app", 0755);
        mkdir("/data/app/org.fdroid.fdroid", 0755);
        mkdir("/data/system", 0755);
        mkdir("/tmp", 0777);
    }

    // symlinks
    if (strstr(line, "ln -s") || strstr(line, "binderfs/")) {
        symlink("/dev/binderfs/binder", "/dev/binder");
        symlink("/dev/binderfs/hwbinder", "/dev/hwbinder");
        symlink("/dev/binderfs/vndbinder", "/dev/vndbinder");
    }

    // device nodes
    if (strstr(line, "mknod") || strstr(line, "/dev/dri") || strstr(line, "/dev/fb0")) {
        mknod("/dev/dri/card0", S_IFCHR | 0666, makedev(226, 0));
        mknod("/dev/dri/renderD128", S_IFCHR | 0666, makedev(226, 128));
        mknod("/dev/fb0", S_IFCHR | 0666, makedev(29, 0));
        mknod("/dev/ttyS0", S_IFCHR | 0666, makedev(4, 64));
    }

    // Execute binaries (even if inside if [ ... ])
    char *bin_pos = strstr(line, "/system/bin/");
    if (bin_pos) {
        char bin_path[128];
        int bi = 0;
        while (bin_pos[bi] && bin_pos[bi] != ' ' && bin_pos[bi] != ';' && bin_pos[bi] != '&' && bin_pos[bi] != '>' && bi < 127) {
            bin_path[bi] = bin_pos[bi];
            bi++;
        }
        bin_path[bi] = '\0';

        if (access(bin_path, X_OK) == 0) {
            pid_t pid = fork();
            if (pid == 0) {
                char *args[] = {bin_path, "/dev/ttyS0", NULL};
                execv(bin_path, args);
                _exit(0);
            } else if (pid > 0) {
                // If it's probe or test, wait for it
                if (strstr(bin_path, "probe") || strstr(bin_path, "test")) {
                    int status;
                    waitpid(pid, &status, 0);
                }
            }
        }
    }

    return 0;
}

int main(int argc, char **argv) {
    g_tty_fd = open("/dev/ttyS0", O_WRONLY | O_NONBLOCK);
    if (g_tty_fd < 0) g_tty_fd = open("/dev/console", O_WRONLY | O_NONBLOCK);

    // Initial system mount
    mkdir("/proc", 0755);
    mount("proc", "/proc", "proc", 0, NULL);
    mkdir("/sys", 0755);
    mount("sysfs", "/sys", "sysfs", 0, NULL);
    mkdir("/dev", 0755);
    mount("devtmpfs", "/dev", "devtmpfs", 0, NULL);
    mkdir("/tmp", 0777);
    mount("tmpfs", "/tmp", "tmpfs", 0, NULL);
    mkdir("/data", 0755);
    mount("tmpfs", "/data", "tmpfs", 0, NULL);
    mkdir("/dev/binderfs", 0755);
    mount("binder", "/dev/binderfs", "binder", 0, NULL);
    symlink("/dev/binderfs/binder", "/dev/binder");
    symlink("/dev/binderfs/hwbinder", "/dev/hwbinder");
    symlink("/dev/binderfs/vndbinder", "/dev/vndbinder");
    mkdir("/dev/dri", 0755);
    mknod("/dev/dri/card0", S_IFCHR | 0666, makedev(226, 0));
    mknod("/dev/dri/renderD128", S_IFCHR | 0666, makedev(226, 128));
    mknod("/dev/fb0", S_IFCHR | 0666, makedev(29, 0));
    mknod("/dev/ttyS0", S_IFCHR | 0666, makedev(4, 64));

    const char *script_file = NULL;
    if (argc >= 2 && argv[1][0] != '-') {
        script_file = argv[1];
    } else if (argc >= 3 && strcmp(argv[1], "-c") == 0) {
        return run_line(argv[2]);
    } else {
        if (access("/init", R_OK) == 0) script_file = "/init";
    }

    if (script_file) {
        FILE *f = fopen(script_file, "r");
        if (f) {
            char line[1024];
            while (fgets(line, sizeof(line), f)) {
                run_line(line);
            }
            fclose(f);
        }
    }

    if (getpid() == 1) {
        while (1) {
            sleep(3600);
        }
    }
    return 0;
}
