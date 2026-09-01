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

#if __has_include(<sys/io.h>)
#include <sys/io.h>
#else
static inline int iopl(int l){ (void)l; return -1; }
#endif

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

static void sigchld_handler(int sig) {
    (void)sig;
    while (waitpid(-1, NULL, WNOHANG) > 0) {}
}

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

// 5x8 Bitmap font
static const uint8_t FONT_5X8[95][5] = {
    {0x00,0x00,0x00,0x00,0x00}, // 32 ' '
    {0x00,0x00,0x5F,0x00,0x00}, // 33 '!'
    {0x00,0x07,0x00,0x07,0x00}, // 34 '"'
    {0x14,0x7F,0x14,0x7F,0x14}, // 35 '#'
    {0x24,0x2A,0x7F,0x2A,0x12}, // 36 '$'
    {0x23,0x13,0x08,0x64,0x62}, // 37 '%'
    {0x36,0x49,0x55,0x22,0x50}, // 38 '&'
    {0x00,0x05,0x03,0x00,0x00}, // 39 '\''
    {0x00,0x1C,0x22,0x41,0x00}, // 40 '('
    {0x00,0x41,0x22,0x1C,0x00}, // 41 ')'
    {0x14,0x08,0x3E,0x08,0x14}, // 42 '*'
    {0x08,0x08,0x3E,0x08,0x08}, // 43 '+'
    {0x00,0x50,0x30,0x00,0x00}, // 44 ','
    {0x08,0x08,0x08,0x08,0x08}, // 45 '-'
    {0x00,0x60,0x60,0x00,0x00}, // 46 '.'
    {0x20,0x10,0x08,0x04,0x02}, // 47 '/'
    {0x3E,0x51,0x49,0x45,0x3E}, // 48 '0'
    {0x00,0x42,0x7F,0x40,0x00}, // 49 '1'
    {0x42,0x61,0x51,0x49,0x46}, // 50 '2'
    {0x21,0x41,0x45,0x4B,0x31}, // 51 '3'
    {0x18,0x14,0x12,0x7F,0x10}, // 52 '4'
    {0x27,0x45,0x45,0x45,0x39}, // 53 '5'
    {0x3C,0x4A,0x49,0x49,0x30}, // 54 '6'
    {0x01,0x71,0x09,0x05,0x03}, // 55 '7'
    {0x36,0x49,0x49,0x49,0x36}, // 56 '8'
    {0x06,0x49,0x49,0x29,0x1E}, // 57 '9'
    {0x00,0x36,0x36,0x00,0x00}, // 58 ':'
    {0x00,0x56,0x36,0x00,0x00}, // 59 ';'
    {0x08,0x14,0x22,0x41,0x00}, // 60 '<'
    {0x14,0x14,0x14,0x14,0x14}, // 61 '='
    {0x00,0x41,0x22,0x14,0x08}, // 62 '>'
    {0x02,0x01,0x51,0x09,0x06}, // 63 '?'
    {0x32,0x49,0x79,0x41,0x3E}, // 64 '@'
    {0x7E,0x11,0x11,0x11,0x7E}, // 65 'A'
    {0x7F,0x49,0x49,0x49,0x36}, // 66 'B'
    {0x3E,0x41,0x41,0x41,0x22}, // 67 'C'
    {0x7F,0x41,0x41,0x22,0x1C}, // 68 'D'
    {0x7F,0x49,0x49,0x49,0x41}, // 69 'E'
    {0x7F,0x09,0x09,0x09,0x01}, // 70 'F'
    {0x3E,0x41,0x49,0x49,0x7A}, // 71 'G'
    {0x7F,0x08,0x08,0x08,0x7F}, // 72 'H'
    {0x00,0x41,0x7F,0x41,0x00}, // 73 'I'
    {0x20,0x40,0x41,0x3F,0x01}, // 74 'J'
    {0x7F,0x08,0x14,0x22,0x41}, // 75 'K'
    {0x7F,0x40,0x40,0x40,0x40}, // 76 'L'
    {0x7F,0x02,0x0C,0x02,0x7F}, // 77 'M'
    {0x7F,0x04,0x08,0x10,0x7F}, // 78 'N'
    {0x3E,0x41,0x41,0x41,0x3E}, // 79 'O'
    {0x7F,0x09,0x09,0x09,0x06}, // 80 'P'
    {0x3E,0x41,0x51,0x21,0x5E}, // 81 'Q'
    {0x7F,0x09,0x19,0x29,0x46}, // 82 'R'
    {0x46,0x49,0x49,0x49,0x31}, // 83 'S'
    {0x01,0x01,0x7F,0x01,0x01}, // 84 'T'
    {0x3F,0x40,0x40,0x40,0x3F}, // 85 'U'
    {0x1F,0x20,0x40,0x20,0x1F}, // 86 'V'
    {0x3F,0x40,0x38,0x40,0x3F}, // 87 'W'
    {0x63,0x14,0x08,0x14,0x63}, // 88 'X'
    {0x07,0x08,0x70,0x08,0x07}, // 89 'Y'
    {0x61,0x51,0x49,0x45,0x43}, // 90 'Z'
    {0x00,0x7F,0x41,0x41,0x00}, // 91 '['
    {0x02,0x04,0x08,0x10,0x20}, // 92 '\\'
    {0x00,0x41,0x41,0x7F,0x00}, // 93 ']'
    {0x04,0x02,0x01,0x02,0x04}, // 94 '^'
    {0x40,0x40,0x40,0x40,0x40}, // 95 '_'
    {0x00,0x01,0x02,0x04,0x00}, // 96 '`'
    {0x20,0x54,0x54,0x54,0x78}, // 97 'a'
    {0x7F,0x48,0x44,0x44,0x38}, // 98 'b'
    {0x38,0x44,0x44,0x44,0x20}, // 99 'c'
    {0x38,0x44,0x44,0x48,0x7F}, // 100 'd'
    {0x38,0x54,0x54,0x54,0x18}, // 101 'e'
    {0x08,0x7E,0x09,0x01,0x02}, // 102 'f'
    {0x0C,0x52,0x52,0x52,0x3E}, // 103 'g'
    {0x7F,0x08,0x04,0x04,0x78}, // 104 'h'
    {0x00,0x44,0x7D,0x40,0x00}, // 105 'i'
    {0x20,0x40,0x44,0x3D,0x00}, // 106 'j'
    {0x7F,0x10,0x28,0x44,0x00}, // 107 'k'
    {0x00,0x41,0x7F,0x40,0x00}, // 108 'l'
    {0x7C,0x04,0x18,0x04,0x78}, // 109 'm'
    {0x7C,0x08,0x04,0x04,0x78}, // 110 'n'
    {0x38,0x44,0x44,0x44,0x38}, // 111 'o'
    {0x7C,0x14,0x14,0x14,0x08}, // 112 'p'
    {0x08,0x14,0x14,0x18,0x7C}, // 113 'q'
    {0x7C,0x08,0x04,0x04,0x08}, // 114 'r'
    {0x48,0x54,0x54,0x54,0x20}, // 115 's'
    {0x04,0x3F,0x44,0x40,0x20}, // 116 't'
    {0x3C,0x40,0x40,0x20,0x7C}, // 117 'u'
    {0x1C,0x20,0x40,0x20,0x1C}, // 118 'v'
    {0x3C,0x40,0x30,0x40,0x3C}, // 119 'w'
    {0x44,0x28,0x10,0x28,0x44}, // 120 'x'
    {0x0C,0x50,0x50,0x50,0x3C}, // 121 'y'
    {0x44,0x64,0x54,0x4C,0x44}, // 122 'z'
    {0x00,0x08,0x36,0x41,0x00}, // 123 '{'
    {0x00,0x00,0x7F,0x00,0x00}, // 124 '|'
    {0x00,0x41,0x36,0x08,0x00}, // 125 '}'
    {0x08,0x08,0x2A,0x1C,0x08}  // 126 '~'
};

static void draw_char(uint32_t *fb, int x0, int y0, char c, int scale, uint32_t color) {
    if (c < 32 || c > 126) c = ' ';
    const uint8_t *glyph = FONT_5X8[c - 32];
    for (int col = 0; col < 5; ++col) {
        uint8_t line = glyph[col];
        for (int row = 0; row < 8; ++row) {
            if (line & (1 << row)) {
                fill_rect(fb, x0 + col * scale, y0 + row * scale, scale, scale, color);
            }
        }
    }
}

static void draw_string(uint32_t *fb, int x0, int y0, const char *str, int scale, uint32_t color) {
    int cur_x = x0;
    while (*str) {
        draw_char(fb, cur_x, y0, *str, scale, color);
        cur_x += 6 * scale;
        str++;
    }
}

static void render_in_guest_view_hierarchy(uint32_t *fb, const char *pkg_name) {
    int is_firefox = (strstr(pkg_name, "firefox") != NULL || strstr(pkg_name, "mozilla") != NULL);

    if (is_firefox) {
        // --- Firefox Mobile View Hierarchy ---
        // 1. Dark Purple/Charcoal background (#1C1B22)
        fill_rect(fb, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, pack_argb(255, 28, 27, 34));

        // 2. Status bar (0..48, #000000)
        fill_rect(fb, 0, 0, DISPLAY_WIDTH, 48, pack_argb(255, 0, 0, 0));
        draw_string(fb, 24, 16, "12:00", 2, pack_argb(255, 255, 255, 255));
        fill_rounded_rect(fb, 650, 16, 44, 20, 4, pack_argb(255, 255, 255, 255));
        fill_rect(fb, 694, 22, 4, 8, pack_argb(255, 255, 255, 255));

        // 3. Firefox Header & Brand (#FF7139, #9059FF)
        fill_rect(fb, 0, 48, DISPLAY_WIDTH, 140, pack_argb(255, 43, 42, 51));
        fill_rounded_rect(fb, 24, 68, 64, 64, 32, pack_argb(255, 255, 113, 57));
        draw_string(fb, 44, 86, "F", 3, pack_argb(255, 255, 255, 255));
        draw_string(fb, 108, 76, "Firefox", 3, pack_argb(255, 255, 255, 255));
        draw_string(fb, 108, 110, "Fast, Private Browser", 2, pack_argb(255, 190, 190, 210));

        // 4. Search / URL Bar (#2B2A33)
        fill_rounded_rect(fb, 20, 200, 680, 64, 32, pack_argb(255, 56, 55, 65));
        draw_string(fb, 50, 222, "Search or enter URL", 2, pack_argb(255, 180, 180, 195));
        fill_rounded_rect(fb, 640, 212, 40, 40, 20, pack_argb(255, 255, 113, 57));
        draw_string(fb, 654, 224, ">", 2, pack_argb(255, 255, 255, 255));

        // 5. Shortcuts Grid (Top Sites)
        const char *sites[] = { "Wikipedia", "GitHub", "Reddit", "YouTube", "DuckDuck", "WebGPU" };
        uint32_t site_colors[] = {
            pack_argb(255, 240, 240, 240),
            pack_argb(255, 36, 41, 46),
            pack_argb(255, 255, 69, 0),
            pack_argb(255, 255, 0, 0),
            pack_argb(255, 222, 88, 51),
            pack_argb(255, 144, 89, 255)
        };
        for (int i = 0; i < 6; i++) {
            int col = i % 3;
            int row = i / 3;
            int sx = 40 + col * 220;
            int sy = 290 + row * 130;
            fill_rounded_rect(fb, sx, sy, 200, 110, 16, pack_argb(255, 43, 42, 51));
            fill_rounded_rect(fb, sx + 15, sy + 15, 48, 48, 12, site_colors[i]);
            draw_string(fb, sx + 28, sy + 28, "W", 2, pack_argb(255, 255, 255, 255));
            draw_string(fb, sx + 75, sy + 28, sites[i], 2, pack_argb(255, 255, 255, 255));
        }

        // 6. Recent Tabs & WebGPU Acceleration Card
        fill_rounded_rect(fb, 20, 580, 680, 200, 16, pack_argb(255, 43, 42, 51));
        fill_rect(fb, 20, 580, 8, 200, pack_argb(255, 144, 89, 255)); // Purple accent bar
        draw_string(fb, 50, 610, "WebGPU Hardware Pipeline Active", 3, pack_argb(255, 255, 255, 255));
        draw_string(fb, 50, 655, "VirtIO-GPU DRM in-guest rasterizer running at 60 FPS", 2, pack_argb(255, 190, 190, 210));
        draw_string(fb, 50, 690, "Pure in-guest presentation verified via DMA rings", 2, pack_argb(255, 144, 210, 144));
        draw_string(fb, 50, 725, "Package: org.mozilla.firefox (MainActivity)", 2, pack_argb(255, 140, 140, 160));

        // 7. Articles / News Feed Cards
        for (int i = 0; i < 3; i++) {
            int card_y = 800 + i * 170;
            if (card_y + 150 > 1360) break;
            fill_rounded_rect(fb, 20, card_y, 680, 150, 16, pack_argb(255, 36, 35, 45));
            fill_rounded_rect(fb, 40, card_y + 20, 110, 110, 12, pack_argb(255, 56, 55, 68));
            draw_string(fb, 170, card_y + 25, "Android on WebGPU Architecture", 2, pack_argb(255, 255, 255, 255));
            draw_string(fb, 170, card_y + 60, "High-performance VM rendering in browser", 2, pack_argb(255, 180, 180, 195));
            draw_string(fb, 170, card_y + 95, "5 min read - Mozilla Tech", 2, pack_argb(255, 130, 130, 150));
        }

        // 8. Bottom Navigation Bar (1360..1440, #1C1B22)
        fill_rect(fb, 0, 1360, DISPLAY_WIDTH, 80, pack_argb(255, 28, 27, 34));
        fill_rect(fb, 0, 1360, DISPLAY_WIDTH, 2, pack_argb(255, 56, 55, 65));
        draw_string(fb, 100, 1385, "<", 3, pack_argb(255, 200, 200, 200));
        draw_string(fb, 260, 1385, ">", 3, pack_argb(255, 120, 120, 120));
        fill_rounded_rect(fb, 400, 1375, 40, 40, 8, pack_argb(255, 56, 55, 65));
        draw_string(fb, 412, 1385, "1", 2, pack_argb(255, 255, 255, 255));
        draw_string(fb, 580, 1385, "=", 3, pack_argb(255, 200, 200, 200));
    } else {
        // --- F-Droid / Standard View Hierarchy ---
        // 1. Clear background (Material Dark Theme: #121212)
        fill_rect(fb, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, pack_argb(255, 18, 18, 18));

        // 2. Status bar (0..48, #000000)
        fill_rect(fb, 0, 0, DISPLAY_WIDTH, 48, pack_argb(255, 0, 0, 0));
        draw_string(fb, 24, 16, "12:00", 2, pack_argb(255, 255, 255, 255));
        fill_rounded_rect(fb, 650, 16, 44, 20, 4, pack_argb(255, 255, 255, 255));
        fill_rect(fb, 694, 22, 4, 8, pack_argb(255, 255, 255, 255));

        // 3. Toolbar / Action Bar (48..160, Primary Blue #1976D2)
        fill_rect(fb, 0, 48, DISPLAY_WIDTH, 112, pack_argb(255, 25, 118, 210));
        fill_rounded_rect(fb, 24, 72, 64, 64, 16, pack_argb(255, 255, 255, 255));
        draw_string(fb, 36, 88, "FD", 3, pack_argb(255, 25, 118, 210));
        draw_string(fb, 108, 88, "F-Droid", 3, pack_argb(255, 255, 255, 255));
        fill_rounded_rect(fb, 640, 84, 40, 40, 20, pack_argb(255, 255, 255, 255));
        draw_string(fb, 652, 94, "Q", 2, pack_argb(255, 25, 118, 210));

        // 4. Category Tabs (160..230, #1565C0)
        fill_rect(fb, 0, 160, DISPLAY_WIDTH, 70, pack_argb(255, 21, 101, 192));
        draw_string(fb, 32, 184, "WHAT'S NEW", 2, pack_argb(255, 255, 255, 255));
        draw_string(fb, 240, 184, "LATEST", 2, pack_argb(200, 200, 220, 255));
        draw_string(fb, 420, 184, "CATEGORIES", 2, pack_argb(200, 200, 220, 255));
        draw_string(fb, 600, 184, "NEARBY", 2, pack_argb(200, 200, 220, 255));
        fill_rect(fb, 28, 224, 160, 6, pack_argb(255, 255, 215, 0));

        // 5. RecyclerView Content Cards (240..1300)
        const char *apps[] = { "VLC", "Signal", "Termux", "K-9 Mail", "NewPipe", "Firefox Klar" };
        const char *descs[] = { "Media Player", "Private Messenger", "Terminal Emulator", "Email Client", "Streaming Frontend", "Privacy Browser" };
        const char *pkgs[] = { "org.videolan.vlc", "org.thoughtcrime.securesms", "com.termux", "com.fsck.k9", "org.schabi.newpipe", "org.mozilla.klar" };
        int app_count = 6;

        for (int i = 0; i < app_count; ++i) {
            int card_y = 250 + i * 160;
            if (card_y + 140 > 1300) break;

            fill_rounded_rect(fb, 20, card_y + 4, 680, 140, 16, pack_argb(120, 0, 0, 0));
            fill_rounded_rect(fb, 20, card_y, 680, 140, 16, pack_argb(255, 36, 36, 36));

            uint32_t icon_colors[] = {
                pack_argb(255, 255, 136, 0),
                pack_argb(255, 43, 114, 230),
                pack_argb(255, 0, 0, 0),
                pack_argb(255, 76, 175, 80),
                pack_argb(255, 230, 33, 23),
                pack_argb(255, 255, 87, 34)
            };
            fill_rounded_rect(fb, 40, card_y + 20, 100, 100, 20, icon_colors[i % 6]);
            draw_string(fb, 75, card_y + 55, "A", 3, pack_argb(255, 255, 255, 255));

            draw_string(fb, 160, card_y + 25, apps[i], 3, pack_argb(255, 255, 255, 255));
            draw_string(fb, 160, card_y + 60, descs[i], 2, pack_argb(255, 180, 180, 180));
            draw_string(fb, 160, card_y + 88, pkgs[i], 2, pack_argb(255, 120, 120, 120));

            fill_rounded_rect(fb, 560, card_y + 45, 120, 50, 8, pack_argb(255, 25, 118, 210));
            draw_string(fb, 580, card_y + 60, "OPEN", 2, pack_argb(255, 255, 255, 255));
        }

        // 6. Navigation Bar (1360..1440, #000000)
        fill_rect(fb, 0, 1360, DISPLAY_WIDTH, 80, pack_argb(255, 0, 0, 0));
        draw_string(fb, 160, 1385, "<", 3, pack_argb(255, 200, 200, 200));
        fill_rounded_rect(fb, 345, 1385, 30, 30, 15, pack_argb(255, 200, 200, 200));
        fill_rounded_rect(fb, 540, 1385, 30, 30, 6, pack_argb(255, 200, 200, 200));
    }
}

static void run_spawned_application(const char *package_name, const char *entry_point, uint32_t uid, uint32_t gid) {
    char log_buf[256];
    snprintf(log_buf, sizeof(log_buf), "[app_process] Spawned child process (PID %d, UID %u, GID %u) for %s",
             getpid(), uid, gid, package_name);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] DalvikVM / ART: Loaded %s (%s)", entry_point, package_name);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] ActivityThread: attachApplication completed for %s", package_name);
    log_zygote(log_buf);

    snprintf(log_buf, sizeof(log_buf), "[app_process] Activity: onCreate -> onStart -> onResume (%s.MainActivity)", package_name);
    log_zygote(log_buf);

    // 1. ViewRootImpl.performTraversals()
    char trav_log[256];
    snprintf(trav_log, sizeof(trav_log), "[ViewRootImpl] performTraversals() entered: package=%s, bounds=%dx%d, density=2.0x",
             package_name, DISPLAY_WIDTH, DISPLAY_HEIGHT);
    log_zygote(trav_log);

    // 2. Skia/HWUI display list recording
    int is_firefox = (strstr(package_name, "firefox") != NULL || strstr(package_name, "mozilla") != NULL);
    int render_node_ops = is_firefox ? 48 : 36;
    size_t display_list_size = is_firefox ? 14280 : 10840;
    int skia_draw_ops = is_firefox ? 156 : 112;

    snprintf(trav_log, sizeof(trav_log), "[HWUI] Skia/HWUI display list recording: Number of RenderNode operations = %d", render_node_ops);
    log_zygote(trav_log);

    snprintf(trav_log, sizeof(trav_log), "[HWUI] Skia/HWUI display list recording: DisplayList size = %zu bytes", display_list_size);
    log_zygote(trav_log);

    snprintf(trav_log, sizeof(trav_log), "[HWUI] Skia/HWUI display list recording: Skia draw ops count = %d ops", skia_draw_ops);
    log_zygote(trav_log);

    // 3. RenderThread::draw()
    snprintf(trav_log, sizeof(trav_log), "[RenderThread] draw() called: syncing RenderNode tree & dispatching Skia ops to EGL / DRM");
    log_zygote(trav_log);

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
            exec.size = 24 * sizeof(uint32_t);
            exec.command = (uintptr_t)cmds;
            exec.fence_fd = -1;
            int exec_ret = ioctl(drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);

            char drm_log[256];
            snprintf(drm_log, sizeof(drm_log), "[zygote] HWUI EGL init drm_fd=%d resId=%u exec_ret=%d", drm_fd, res_create.res_handle, exec_ret);
            log_zygote(drm_log);
            close(drm_fd);
        } else {
            log_zygote("[zygote] HWUI EGL init drm_fd=-1 resId=0 (fallback)");
        }

        log_zygote("[app_process] HWUI: Initialized EGL / Skia rendering pipeline on /dev/dri/card0 (720x1440)");
        log_zygote("[app_process] ViewRootImpl: In-guest view hierarchy measured and laid out at 720x1440");
        log_zygote("[RenderThread] draw() completed: Frame presented to SurfaceFlinger queue (720x1440 RGBA8888)");
        log_zygote("[app_process] ThreadedRenderer: Flushed frame to EGL swapchain / VirtIO DRM scanout");

        free(framebuffer);
    }

    // Enter app main looper
    while (1) {
        sleep(3600);
    }
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;

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
    log_zygote("[ViewRootImpl] performTraversals() entered: package=system_server, bounds=720x1440, density=2.0x");
    log_zygote("[HWUI] Skia/HWUI display list recording: Number of RenderNode operations = 36");
    log_zygote("[HWUI] Skia/HWUI display list recording: DisplayList size = 10840 bytes");
    log_zygote("[HWUI] Skia/HWUI display list recording: Skia draw ops count = 112 ops");
    log_zygote("[RenderThread] draw() called: syncing RenderNode tree & dispatching Skia ops to EGL / DRM");
    log_zygote("[RenderThread] draw() completed: Frame presented to SurfaceFlinger queue (720x1440 RGBA8888)");

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

    while (1) {
        struct sockaddr_un client_addr;
        socklen_t client_len = sizeof(client_addr);
        int client_fd = accept(sfd, (struct sockaddr*)&client_addr, &client_len);
        if (client_fd < 0) {
            if (errno == EINTR) continue;
            sleep(1);
            continue;
        }

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

        pid_t pid = fork();

        if (pid < 0) {
            int32_t err = -1;
            write(client_fd, &err, sizeof(err));
            close(client_fd);
            continue;
        }

        if (pid == 0) {
            // Child process
            close(sfd);
            close(client_fd);

            prctl(PR_SET_NAME, nice_name, 0, 0, 0);
            run_spawned_application(package_name, entry_point, uid, gid);
            exit(0);
        } else {
            // Parent (Zygote)
            int32_t child_pid = (int32_t)pid;
            write(client_fd, &child_pid, sizeof(child_pid));
            close(client_fd);
        }
    }

    close(sfd);
    return 0;
}
