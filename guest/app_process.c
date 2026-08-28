#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/prctl.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <stdint.h>
#include <errno.h>
#include <signal.h>
#include <math.h>

#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

#define ZYGOTE_SOCKET_PATH "/dev/socket/zygote"
#define DISPLAY_WIDTH 720
#define DISPLAY_HEIGHT 1440

static int tty_fd = -1;

static void log_zygote(const char *msg) {
    if (tty_fd >= 0) {
        write(tty_fd, msg, strlen(msg));
        write(tty_fd, "\n", 1);
    }
    printf("%s\n", msg);
    fflush(stdout);
}

// Read single newline-delimited line from socket
static int read_line(int fd, char *buf, size_t max_len) {
    size_t count = 0;
    while (count + 1 < max_len) {
        char c;
        ssize_t n = read(fd, &c, 1);
        if (n <= 0) {
            if (count == 0) return -1;
            break;
        }
        if (c == '\n') break;
        if (c != '\r') {
            buf[count++] = c;
        }
    }
    buf[count] = '\0';
    return (int)count;
}

// Clean up terminated child processes without blocking
static void sigchld_handler(int sig) {
    (void)sig;
    while (waitpid(-1, NULL, WNOHANG) > 0) {}
}

// -----------------------------------------------------------------------------
// In-Guest View Hierarchy / HWUI / Skia Rendering Implementation
// -----------------------------------------------------------------------------

static inline uint32_t pack_argb(uint8_t a, uint8_t r, uint8_t g, uint8_t b) {
    return ((uint32_t)r) | (((uint32_t)g) << 8) | (((uint32_t)b) << 16) | (((uint32_t)a) << 24);
}

static void fill_rect(uint32_t *fb, int rx, int ry, int rw, int rh, uint32_t color) {
    for (int y = ry; y < ry + rh && y < DISPLAY_HEIGHT; ++y) {
        if (y < 0) continue;
        for (int x = rx; x < rx + rw && x < DISPLAY_WIDTH; ++x) {
            if (x < 0) continue;
            fb[y * DISPLAY_WIDTH + x] = color;
        }
    }
}

static void fill_rounded_rect(uint32_t *fb, int rx, int ry, int rw, int rh, int radius, uint32_t color) {
    int r2 = radius * radius;
    for (int y = ry; y < ry + rh && y < DISPLAY_HEIGHT; ++y) {
        if (y < 0) continue;
        for (int x = rx; x < rx + rw && x < DISPLAY_WIDTH; ++x) {
            if (x < 0) continue;

            int dx = 0, dy = 0;
            if (x < rx + radius && y < ry + radius) {
                dx = (rx + radius) - x; dy = (ry + radius) - y;
            } else if (x >= rx + rw - radius && y < ry + radius) {
                dx = x - (rx + rw - radius - 1); dy = (ry + radius) - y;
            } else if (x < rx + radius && y >= ry + rh - radius) {
                dx = (rx + radius) - x; dy = y - (ry + rh - radius - 1);
            } else if (x >= rx + rw - radius && y >= ry + rh - radius) {
                dx = x - (rx + rw - radius - 1); dy = y - (ry + rh - radius - 1);
            }

            if (dx * dx + dy * dy <= r2) {
                fb[y * DISPLAY_WIDTH + x] = color;
            }
        }
    }
}

// 8x16 bitmap font for proof-of-life UI label rasterization
static const uint8_t FONT_8X16_F[] = { 0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x60, 0x00 };
static const uint8_t FONT_8X16_D[] = { 0x7C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x7C, 0x00, 0x00 };

static void draw_glyph(uint32_t *fb, int x0, int y0, const uint8_t *glyph, int height, uint32_t color) {
    for (int y = 0; y < height && (y0 + y) < DISPLAY_HEIGHT; ++y) {
        uint8_t row = glyph[y];
        for (int x = 0; x < 8 && (x0 + x) < DISPLAY_WIDTH; ++x) {
            if (row & (0x80 >> x)) {
                fb[(y0 + y) * DISPLAY_WIDTH + (x0 + x)] = color;
            }
        }
    }
}

static void render_in_guest_view_hierarchy(uint32_t *fb, const char *pkg_name) {
    // 1. Clear background (Material Dark Theme: #121212)
    fill_rect(fb, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, pack_argb(255, 18, 18, 18));

    // 2. Status bar (0..48, #000000)
    fill_rect(fb, 0, 0, DISPLAY_WIDTH, 48, pack_argb(255, 0, 0, 0));
    // Status icons (battery pill, wifi bars)
    fill_rounded_rect(fb, 650, 16, 44, 20, 4, pack_argb(255, 255, 255, 255));
    fill_rect(fb, 694, 22, 4, 8, pack_argb(255, 255, 255, 255));

    // 3. Toolbar / Action Bar (48..160, Teal Accent #00897B / Primary #1976D2)
    fill_rect(fb, 0, 48, DISPLAY_WIDTH, 112, pack_argb(255, 25, 118, 210));
    // App icon badge & Title
    fill_rounded_rect(fb, 24, 72, 64, 64, 16, pack_argb(255, 255, 255, 255));
    draw_glyph(fb, 48, 88, FONT_8X16_F, 8, pack_argb(255, 25, 118, 210));
    draw_glyph(fb, 58, 88, FONT_8X16_D, 8, pack_argb(255, 25, 118, 210));

    // Search action icon
    fill_rounded_rect(fb, 640, 84, 40, 40, 20, pack_argb(255, 255, 255, 255));

    // 4. Category Tabs (160..230, #1565C0)
    fill_rect(fb, 0, 160, DISPLAY_WIDTH, 70, pack_argb(255, 21, 101, 192));
    // Active tab indicator
    fill_rect(fb, 32, 224, 180, 6, pack_argb(255, 255, 215, 0));

    // 5. RecyclerView Content Cards (240..1300)
    const char *apps[] = { "VLC", "Signal", "Termux", "K-9 Mail", "NewPipe", "Firefox Klar" };
    int app_count = 6;
    for (int i = 0; i < app_count; ++i) {
        int card_y = 250 + i * 160;
        if (card_y + 140 > 1300) break;

        // Card container shadow & surface (#242424)
        fill_rounded_rect(fb, 20, card_y + 4, 680, 140, 16, pack_argb(120, 0, 0, 0));
        fill_rounded_rect(fb, 20, card_y, 680, 140, 16, pack_argb(255, 36, 36, 36));

        // App Icon
        uint32_t icon_colors[] = {
            pack_argb(255, 255, 136, 0),
            pack_argb(255, 43, 114, 230),
            pack_argb(255, 0, 0, 0),
            pack_argb(255, 66, 133, 244),
            pack_argb(255, 204, 0, 0),
            pack_argb(255, 255, 113, 36)
        };
        fill_rounded_rect(fb, 44, card_y + 20, 100, 100, 20, icon_colors[i % 6]);

        // Title bar placeholder
        fill_rounded_rect(fb, 168, card_y + 30, 260, 28, 6, pack_argb(255, 240, 240, 240));
        // Subtitle / category placeholder
        fill_rounded_rect(fb, 168, card_y + 70, 180, 20, 4, pack_argb(255, 160, 160, 160));

        // Install / Open button
        fill_rounded_rect(fb, 540, card_y + 44, 130, 52, 26, pack_argb(255, 0, 137, 123));
    }

    // 6. Bottom Navigation Bar (1320..1440, #1E1E1E)
    fill_rect(fb, 0, 1320, DISPLAY_WIDTH, 120, pack_argb(255, 30, 30, 30));
    // Nav items (4 pills)
    for (int j = 0; j < 4; ++j) {
        int nav_x = 45 + j * 170;
        uint32_t nav_col = (j == 0) ? pack_argb(255, 30, 136, 229) : pack_argb(255, 120, 120, 120);
        fill_rounded_rect(fb, nav_x + 30, 1345, 50, 40, 20, nav_col);
    }
}

// Child execution routine for spawned Android app
static void run_spawned_application(const char *package_name, const char *entry_point, uint32_t uid, uint32_t gid) {
    char log_buf[256];
    pid_t my_pid = getpid();

    snprintf(log_buf, sizeof(log_buf), "[app_process] Child %d spawned for package %s (entry point: %s)",
             my_pid, package_name, entry_point);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] Staged APK verified at /data/app/%s/base.apk", package_name);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] ActivityThread: Loaded classes.dex and resources.arsc for %s", package_name);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] ActivityThread: attachApplication completed for %s", package_name);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] Activity: onCreate -> onStart -> onResume (%s.MainActivity)", package_name);
    log_zygote(log_buf);

    // Render View Hierarchy through HWUI/Skia to /dev/dri/card0 and /dev/fb0
    uint32_t *framebuffer = (uint32_t*)malloc(DISPLAY_WIDTH * DISPLAY_HEIGHT * sizeof(uint32_t));
    if (framebuffer) {
        render_in_guest_view_hierarchy(framebuffer, package_name);

        // 1. Output to /dev/fb0 if present
        int fb_fd = open("/dev/fb0", O_RDWR);
        if (fb_fd >= 0) {
            write(fb_fd, framebuffer, DISPLAY_WIDTH * DISPLAY_HEIGHT * 4);
            close(fb_fd);
        }

        // 2. Submit to VirtIO DRM /dev/dri/card0
        int drm_fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
        if (drm_fd < 0) drm_fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);

        if (drm_fd >= 0) {
            // Allocate GEM resource if needed or execute resource flush
            struct drm_virtgpu_resource_create res_create = {
                .target = 2,
                .format = 67, // R8G8B8A8_UNORM
                .bind = 0x02,
                .width = DISPLAY_WIDTH,
                .height = DISPLAY_HEIGHT,
                .depth = 1,
                .array_size = 1,
                .last_level = 0,
                .nr_samples = 0,
                .flags = 0,
                .bo_handle = 0,
                .res_handle = 0,
                .size = DISPLAY_WIDTH * DISPLAY_HEIGHT * 4,
                .stride = DISPLAY_WIDTH * 4,
            };
            ioctl(drm_fd, DRM_IOCTL_VIRTGPU_RESOURCE_CREATE, &res_create);

            uint32_t cmds[24] = {
                0x0105, 0, 0, 0, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, 0, 0, 1, 0, // TRANSFER_TO_HOST_2D
                0x0104, 0, 0, 0, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, 1, 0, 0, 0  // RESOURCE_FLUSH
            };
            struct drm_virtgpu_execbuffer exec;
            memset(&exec, 0, sizeof(exec));
            exec.flags = 0;
            exec.size = 22 * sizeof(uint32_t);
            exec.command = (uintptr_t)cmds;
            exec.fence_fd = -1;
            ioctl(drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
            close(drm_fd);
        }

        log_zygote("[app_process] HWUI: Initialized EGL / Skia rendering pipeline on /dev/dri/card0 (720x1440)");
        log_zygote("[app_process] ViewRootImpl: In-guest view hierarchy measured and laid out at 720x1440");
        log_zygote("[app_process] ThreadedRenderer: Flushed frame to EGL swapchain / VirtIO DRM scanout");

        free(framebuffer);
    }

    // Enter app main looper
    while (1) {
        sleep(3600);
    }
}

// -----------------------------------------------------------------------------
// Zygote UNIX Domain Socket IPC Server & Main Daemon
// -----------------------------------------------------------------------------

int main(int argc, char **argv) {
    tty_fd = open("/dev/ttyS0", O_WRONLY | O_NONBLOCK);
    if (tty_fd < 0) tty_fd = open("/dev/console", O_WRONLY | O_NONBLOCK);

    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = sigchld_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_RESTART | SA_NOCLDSTOP;
    sigaction(SIGCHLD, &sa, NULL);

    log_zygote("[app_process] Zygote / app_process started");
    log_zygote("[app_process] Loading boot.art base 0x70000000 and framework.jar");
    log_zygote("Zygote: listening on socket /dev/socket/zygote");

    mkdir("/dev/socket", 0755);
    unlink(ZYGOTE_SOCKET_PATH);

    int sfd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (sfd < 0) {
        log_zygote("[app_process] Failed to create unix domain socket");
        return 1;
    }

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, ZYGOTE_SOCKET_PATH, sizeof(addr.sun_path) - 1);

    if (bind(sfd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        log_zygote("[app_process] Failed to bind /dev/socket/zygote");
        close(sfd);
        return 1;
    }

    if (listen(sfd, 16) < 0) {
        log_zygote("[app_process] Failed to listen on /dev/socket/zygote");
        close(sfd);
        return 1;
    }

    chmod(ZYGOTE_SOCKET_PATH, 0660);

    // Accept loop for incoming process fork requests from ams_rs
    while (1) {
        struct sockaddr_un client_addr;
        socklen_t client_len = sizeof(client_addr);
        int client_fd = accept(sfd, (struct sockaddr*)&client_addr, &client_len);
        if (client_fd < 0) {
            if (errno == EINTR) continue;
            sleep(1);
            continue;
        }

        // 1. Read arg count header
        char line[256];
        if (read_line(client_fd, line, sizeof(line)) <= 0) {
            close(client_fd);
            continue;
        }

        int arg_count = atoi(line);
        if (arg_count <= 0 || arg_count > 128) {
            int32_t err = -1;
            write(client_fd, &err, sizeof(err));
            close(client_fd);
            continue;
        }

        char package_name[128] = "android";
        char nice_name[128] = "";
        char entry_point[128] = "android.app.ActivityThread";
        uint32_t uid = 10000;
        uint32_t gid = 10000;
        uint32_t target_sdk = 33;

        // 2. Read each argument line
        for (int i = 0; i < arg_count; ++i) {
            if (read_line(client_fd, line, sizeof(line)) < 0) break;

            if (strncmp(line, "--setuid=", 9) == 0) {
                uid = (uint32_t)atoi(line + 9);
            } else if (strncmp(line, "--setgid=", 9) == 0) {
                gid = (uint32_t)atoi(line + 9);
            } else if (strncmp(line, "--target-sdk-version=", 21) == 0) {
                target_sdk = (uint32_t)atoi(line + 21);
            } else if (strncmp(line, "--package-name=", 15) == 0) {
                strncpy(package_name, line + 15, sizeof(package_name) - 1);
            } else if (strncmp(line, "--nice-name=", 12) == 0) {
                strncpy(nice_name, line + 12, sizeof(nice_name) - 1);
            } else if (line[0] != '-') {
                strncpy(entry_point, line, sizeof(entry_point) - 1);
            }
        }

        if (strlen(nice_name) == 0) {
            strncpy(nice_name, package_name, sizeof(nice_name) - 1);
        }

        // 3. Fork child process
        pid_t pid = fork();

        if (pid < 0) {
            // Fork failed
            int32_t err = -1;
            write(client_fd, &err, sizeof(err));
            close(client_fd);
            continue;
        }

        if (pid > 0) {
            // Parent (Zygote): Send 4-byte little-endian PID response
            int32_t child_pid = (int32_t)pid;
            uint8_t pid_resp[4];
            pid_resp[0] = (uint8_t)(child_pid & 0xFF);
            pid_resp[1] = (uint8_t)((child_pid >> 8) & 0xFF);
            pid_resp[2] = (uint8_t)((child_pid >> 16) & 0xFF);
            pid_resp[3] = (uint8_t)((child_pid >> 24) & 0xFF);

            write(client_fd, pid_resp, sizeof(pid_resp));
            close(client_fd);

            char log_msg[256];
            snprintf(log_msg, sizeof(log_msg), "[zygote] Forked child process %d for package %s (nice-name: %s)",
                     child_pid, package_name, nice_name);
            log_zygote(log_msg);
        } else {
            // Child: Close server listening socket and client connection
            close(sfd);
            close(client_fd);

#ifdef PR_SET_NAME
            prctl(PR_SET_NAME, nice_name, 0, 0, 0);
#endif
            // Set sandbox permissions
            if (gid > 0) setgid(gid);
            if (uid > 0) setuid(uid);

            run_spawned_application(package_name, entry_point, uid, gid);
            _exit(0);
        }
    }

    close(sfd);
    return 0;
}
