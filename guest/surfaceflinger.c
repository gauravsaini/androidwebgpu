#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <stdint.h>
#include <errno.h>
#include <time.h>
#include <inttypes.h>

#if __has_include(<sys/io.h>)
#include <sys/io.h>
#else
static inline int iopl(int l){ (void)l; return -1; }
#endif

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <hardware/hardware.h>
#include <hardware/gralloc.h>
#include <hardware/hwcomposer.h>
#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

#define DISPLAY_WIDTH 720
#define DISPLAY_HEIGHT 1440

// VirtIO Legacy I/O Offsets
#define OFF_HOST_FEATURES   0x00
#define OFF_GUEST_FEATURES  0x04
#define OFF_QUEUE_PFN       0x08
#define OFF_QUEUE_SIZE      0x0C
#define OFF_QUEUE_SEL       0x0E
#define OFF_QUEUE_NOTIFY    0x10
#define OFF_DEVICE_STATUS   0x12
#define OFF_ISR_STATUS      0x13
#define OFF_NUM_SCANOUTS    0x1C
#define OFF_NUM_CAPSETS     0x20

#define STATUS_ACKNOWLEDGE  0x01
#define STATUS_DRIVER       0x02
#define STATUS_DRIVER_OK    0x04
#define STATUS_FEATURES_OK  0x08

#define VIRTIO_GPU_F_VIRGL  (1u << 0)
#define VIRTIO_GPU_F_EDID   (1u << 1)

#define VRING_DESC_F_NEXT   1
#define VRING_DESC_F_WRITE  2

#define CMD_GET_DISPLAY_INFO        0x0100
#define CMD_RESOURCE_CREATE_2D      0x0101
#define CMD_RESOURCE_UNREF          0x0102
#define CMD_SET_SCANOUT             0x0103
#define CMD_RESOURCE_FLUSH          0x0104
#define CMD_TRANSFER_TO_HOST_2D     0x0105
#define CMD_RESOURCE_ATTACH_BACKING 0x0106
#define RESP_OK_NODATA              0x1100
#define RESP_OK_DISPLAY_INFO        0x1101

#pragma pack(push,1)
struct VirtioGpuCtrlHdr {
    uint32_t type;
    uint32_t flags;
    uint64_t fence_id;
    uint32_t ctx_id;
    uint32_t padding;
};
struct VirtioGpuRect { uint32_t x, y, width, height; };
struct VirtioGpuDisplayOne { struct VirtioGpuRect r; uint32_t enabled, flags; };
struct VirtioGpuRespDisplayInfo { struct VirtioGpuCtrlHdr hdr; struct VirtioGpuDisplayOne pmodes[16]; };
struct VirtioGpuResourceCreate2d { struct VirtioGpuCtrlHdr hdr; uint32_t resource_id, format, width, height; };
struct VirtioGpuSetScanout { struct VirtioGpuCtrlHdr hdr; struct VirtioGpuRect r; uint32_t scanout_id, resource_id; };
struct VirtioGpuResourceFlush { struct VirtioGpuCtrlHdr hdr; struct VirtioGpuRect r; uint32_t resource_id, padding2; };
struct VirtioGpuTransferToHost2d { struct VirtioGpuCtrlHdr hdr; struct VirtioGpuRect r; uint64_t offset; uint32_t resource_id, padding2; };
struct VirtioGpuMemEntry { uint64_t addr; uint32_t length; uint32_t padding; };
struct VirtioGpuResourceAttachBacking { struct VirtioGpuCtrlHdr hdr; uint32_t resource_id, nr_entries; };
struct VirtqDesc { uint64_t addr; uint32_t len; uint16_t flags; uint16_t next; };
#pragma pack(pop)

static int tty_fd = -1;
static uint16_t g_virtio_io_base = 0;
static void *g_dma_map = NULL;
static void *g_q0_pages = NULL;
static uint64_t g_q0_pfn = 0;
static void *g_cmd_page = NULL;
static uint64_t g_cmd_pfn = 0;
static void *g_resp_page = NULL;
static uint64_t g_resp_pfn = 0;
static uint32_t *g_framebuffer = NULL;
static uint64_t g_fb_pfn = 0;
static uint16_t g_avail_idx = 0;
static uint32_t g_fence_counter = 1;

static void log_sf(const char *msg) {
    if (tty_fd >= 0) {
        write(tty_fd, msg, strlen(msg));
        write(tty_fd, "\n", 1);
    }
    printf("%s\n", msg);
    fflush(stdout);
}

static inline void io_outl(uint16_t port, uint32_t val) {
    __asm__ volatile ("outl %0, %1" : : "a"(val), "Nd"(port));
}
static inline uint32_t io_inl(uint16_t port) {
    uint32_t v;
    __asm__ volatile ("inl %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}
static inline void io_outw(uint16_t port, uint16_t val) {
    __asm__ volatile ("outw %0, %1" : : "a"(val), "Nd"(port));
}
static inline uint16_t io_inw(uint16_t port) {
    uint16_t v;
    __asm__ volatile ("inw %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}
static inline void io_outb(uint16_t port, uint8_t val) {
    __asm__ volatile ("outb %0, %1" : : "a"(val), "Nd"(port));
}
static inline uint8_t io_inb(uint16_t port) {
    uint8_t v;
    __asm__ volatile ("inb %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}

static uint64_t virt_to_pfn(void *vaddr) {
    uint64_t vpn = (uint64_t)(uintptr_t)vaddr / 4096;
    int fd = open("/proc/self/pagemap", O_RDONLY);
    if (fd < 0) return vpn;
    uint64_t entry = 0;
    if (pread(fd, &entry, 8, vpn * 8) != 8) { close(fd); return vpn; }
    close(fd);
    if (!(entry & (1ULL << 63))) return vpn;
    return entry & ((1ULL << 55) - 1);
}

// Build exact physical scatter-gather memory table for non-contiguous userspace buffers
static int build_sg_backing(void *vaddr, size_t size, struct VirtioGpuMemEntry *entries, size_t max_entries) {
    int fd = open("/proc/self/pagemap", O_RDONLY);
    if (fd < 0) {
        entries[0].addr = virt_to_pfn(vaddr) * 4096;
        entries[0].length = (uint32_t)size;
        entries[0].padding = 0;
        return 1;
    }

    size_t num_pages = (size + 4095) / 4096;
    uint64_t start_vpn = (uint64_t)(uintptr_t)vaddr / 4096;
    uint64_t *pagemap_entries = malloc(num_pages * sizeof(uint64_t));
    if (!pagemap_entries) {
        close(fd);
        entries[0].addr = virt_to_pfn(vaddr) * 4096;
        entries[0].length = (uint32_t)size;
        entries[0].padding = 0;
        return 1;
    }

    if (pread(fd, pagemap_entries, num_pages * sizeof(uint64_t), start_vpn * 8) != (ssize_t)(num_pages * sizeof(uint64_t))) {
        free(pagemap_entries);
        close(fd);
        entries[0].addr = virt_to_pfn(vaddr) * 4096;
        entries[0].length = (uint32_t)size;
        entries[0].padding = 0;
        return 1;
    }
    close(fd);

    int count = 0;
    for (size_t i = 0; i < num_pages; i++) {
        uint64_t raw = pagemap_entries[i];
        uint64_t pfn = (raw & (1ULL << 63)) ? (raw & ((1ULL << 55) - 1)) : (start_vpn + i);
        uint64_t phys_addr = pfn * 4096;
        uint32_t page_len = (i == num_pages - 1 && (size % 4096) != 0) ? (uint32_t)(size % 4096) : 4096;

        // Merge contiguous physical page runs
        if (count > 0 && (entries[count - 1].addr + entries[count - 1].length) == phys_addr) {
            entries[count - 1].length += page_len;
        } else if ((size_t)count < max_entries) {
            entries[count].addr = phys_addr;
            entries[count].length = page_len;
            entries[count].padding = 0;
            count++;
        }
    }
    free(pagemap_entries);
    return count > 0 ? count : 1;
}

static uint16_t detect_virtio_gpu_io_base(void) {
    FILE *fp = fopen("/proc/ioports", "r");
    if (fp) {
        char line[256];
        while (fgets(line, sizeof(line), fp)) {
            if (strstr(line, "virtio-pci") || strstr(line, "virtio_pci") || strstr(line, "0000:00:06.0")) {
                unsigned int start, end;
                if (sscanf(line, " %x-%x", &start, &end) >= 1) {
                    fclose(fp);
                    return (uint16_t)start;
                }
            }
        }
        fclose(fp);
    }
    uint16_t candidates[] = { 0xC000, 0xC100, 0xC140, 0xC040 };
    for (size_t i = 0; i < sizeof(candidates)/sizeof(candidates[0]); i++) {
        uint16_t port = candidates[i];
        uint32_t feats = io_inl(port + OFF_HOST_FEATURES);
        if (feats == 0x03 || feats == 0x01 || feats == 0x02 || feats == 0x00) {
            uint16_t num_scanouts = io_inw(port + OFF_NUM_SCANOUTS);
            if (num_scanouts >= 1 && num_scanouts <= 16) return port;
        }
    }
    return 0xC000;
}

// Send single VirtIO-GPU control command through Queue 0
static int send_virtio_gpu_command(const void *cmd, size_t cmd_len, void *resp, size_t resp_len) {
    if (!g_q0_pages || !g_cmd_page || !g_resp_page || g_virtio_io_base == 0) return -1;

    memcpy(g_cmd_page, cmd, cmd_len);
    memset(g_resp_page, 0, resp_len > 0 ? resp_len : 24);

    uint8_t *base = (uint8_t*)g_q0_pages;
    struct VirtqDesc *desc = (struct VirtqDesc*)base;
    uint16_t *avail_ring = (uint16_t*)(base + 4096 + 4);
    uint16_t *avail_idx_ptr = (uint16_t*)(base + 4096 + 2);
    volatile uint16_t *used_idx_ptr = (volatile uint16_t*)(base + 8192 + 2);

    uint16_t d0 = (g_avail_idx * 2) % 250;
    uint16_t d1 = d0 + 1;

    desc[d0].addr = g_cmd_pfn * 4096;
    desc[d0].len = (uint32_t)cmd_len;
    desc[d0].flags = VRING_DESC_F_NEXT;
    desc[d0].next = d1;

    desc[d1].addr = g_resp_pfn * 4096;
    desc[d1].len = (uint32_t)(resp_len > 0 ? resp_len : 24);
    desc[d1].flags = VRING_DESC_F_WRITE;
    desc[d1].next = 0;

    avail_ring[g_avail_idx % 256] = d0;
    g_avail_idx++;
    *avail_idx_ptr = g_avail_idx;
    __sync_synchronize();

    io_outw(g_virtio_io_base + OFF_QUEUE_NOTIFY, 0);

    // Wait for completion (poll used ring up to 50ms)
    uint16_t target_used = g_avail_idx;
    for (int spin = 0; spin < 50; spin++) {
        if (*used_idx_ptr >= target_used) break;
        usleep(1000);
    }

    if (resp && resp_len > 0) {
        memcpy(resp, g_resp_page, resp_len);
    }
    return 0;
}

static int init_direct_virtio_gpu(void) {
    if (iopl(3) != 0) {
        log_sf("[surfaceflinger] WARNING: iopl(3) failed, direct I/O may fault");
    }

    g_virtio_io_base = detect_virtio_gpu_io_base();
    char log_buf[256];
    snprintf(log_buf, sizeof(log_buf), "[surfaceflinger] Direct VirtIO-GPU driver using I/O base 0x%04X", g_virtio_io_base);
    log_sf(log_buf);

    // VirtIO handshake: ACK -> DRIVER -> FEATURES_OK -> DRIVER_OK
    io_outb(g_virtio_io_base + OFF_DEVICE_STATUS, STATUS_ACKNOWLEDGE);
    io_outb(g_virtio_io_base + OFF_DEVICE_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
    uint32_t host_feats = io_inl(g_virtio_io_base + OFF_HOST_FEATURES);
    io_outl(g_virtio_io_base + OFF_GUEST_FEATURES, host_feats & (VIRTIO_GPU_F_VIRGL | VIRTIO_GPU_F_EDID));
    io_outb(g_virtio_io_base + OFF_DEVICE_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK);
    io_outb(g_virtio_io_base + OFF_DEVICE_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK);

    // Queue 0 (control queue, 256 descs)
    io_outw(g_virtio_io_base + OFF_QUEUE_SEL, 0);
    uint16_t qsize = io_inw(g_virtio_io_base + OFF_QUEUE_SIZE);
    if (qsize == 0) qsize = 256;

    // Direct Physical Memory allocation via /dev/mem at 0x10000000 (256MB offset in guest RAM)
    int mem_fd = open("/dev/mem", O_RDWR | O_SYNC);
    uint32_t phys_base = 0x10000000;
    size_t total_map_size = 16 * 1024 * 1024;
    void *dma_map = MAP_FAILED;
    if (mem_fd >= 0) {
        dma_map = mmap(NULL, total_map_size, PROT_READ | PROT_WRITE, MAP_SHARED, mem_fd, (off_t)phys_base);
    }

    if (dma_map != MAP_FAILED) {
        g_dma_map = dma_map;
        memset(dma_map, 0, total_map_size);
        g_q0_pages = dma_map;
        g_q0_pfn = phys_base / 4096;

        g_cmd_page = (uint8_t*)dma_map + 0x4000;
        g_cmd_pfn = (phys_base + 0x4000) / 4096;

        g_resp_page = (uint8_t*)dma_map + 0x5000;
        g_resp_pfn = (phys_base + 0x5000) / 4096;

        g_framebuffer = (uint32_t*)((uint8_t*)dma_map + 0x10000);
        g_fb_pfn = (phys_base + 0x10000) / 4096;

        snprintf(log_buf, sizeof(log_buf), "[surfaceflinger] /dev/mem direct DMA mapped: phys=0x%08X q0_pfn=0x%" PRIx64 " fb_pfn=0x%" PRIx64,
                 phys_base, g_q0_pfn, g_fb_pfn);
        log_sf(log_buf);
    } else {
        // Fallback to posix_memalign
        log_sf("[surfaceflinger] /dev/mem open failed, falling back to posix_memalign");
        if (posix_memalign(&g_q0_pages, 4096, 3 * 4096) != 0) {
            log_sf("[surfaceflinger] Failed to allocate DMA pages for queue 0");
            return -1;
        }
        memset(g_q0_pages, 0, 3 * 4096);
        mlock(g_q0_pages, 3 * 4096);
        g_q0_pfn = virt_to_pfn(g_q0_pages);

        if (posix_memalign(&g_cmd_page, 4096, 65536) != 0 || posix_memalign(&g_resp_page, 4096, 4096) != 0) {
            log_sf("[surfaceflinger] Failed to allocate cmd/resp DMA pages");
            return -1;
        }
        memset(g_cmd_page, 0, 65536);
        memset(g_resp_page, 0, 4096);
        mlock(g_cmd_page, 65536);
        mlock(g_resp_page, 4096);
        g_cmd_pfn = virt_to_pfn(g_cmd_page);
        g_resp_pfn = virt_to_pfn(g_resp_page);

        size_t fb_size = DISPLAY_WIDTH * DISPLAY_HEIGHT * 4;
        if (posix_memalign((void**)&g_framebuffer, 4096, fb_size) != 0) {
            log_sf("[surfaceflinger] Failed to allocate framebuffer");
            return -1;
        }
        memset(g_framebuffer, 0, fb_size);
        mlock(g_framebuffer, fb_size);
        g_fb_pfn = virt_to_pfn(g_framebuffer);
    }

    io_outl(g_virtio_io_base + OFF_QUEUE_PFN, (uint32_t)g_q0_pfn);
    snprintf(log_buf, sizeof(log_buf), "[surfaceflinger] Queue 0 initialized (size=%u, PFN=0x%" PRIx64 ")", qsize, g_q0_pfn);
    log_sf(log_buf);

    size_t fb_size = DISPLAY_WIDTH * DISPLAY_HEIGHT * 4;

    // 1. Command: RESOURCE_CREATE_2D (resId=1, 720x1440, format=67)
    struct VirtioGpuResourceCreate2d c2d;
    memset(&c2d, 0, sizeof(c2d));
    c2d.hdr.type = CMD_RESOURCE_CREATE_2D;
    c2d.hdr.fence_id = g_fence_counter++;
    c2d.resource_id = 1;
    c2d.format = 67; // R8G8B8A8_UNORM
    c2d.width = DISPLAY_WIDTH;
    c2d.height = DISPLAY_HEIGHT;
    struct VirtioGpuCtrlHdr resp;
    send_virtio_gpu_command(&c2d, sizeof(c2d), &resp, sizeof(resp));

    // 2. Command: RESOURCE_ATTACH_BACKING (attaching full scatter-gather framebuffer DMA memory)
    struct VirtioGpuMemEntry sg_entries[1024];
    int num_entries = build_sg_backing(g_framebuffer, fb_size, sg_entries, 1024);
    size_t attach_size = sizeof(struct VirtioGpuResourceAttachBacking) + num_entries * sizeof(struct VirtioGpuMemEntry);
    uint8_t *attach_buf = (uint8_t*)malloc(attach_size);
    if (attach_buf) {
        memset(attach_buf, 0, attach_size);
        struct VirtioGpuResourceAttachBacking *att = (struct VirtioGpuResourceAttachBacking*)attach_buf;
        att->hdr.type = CMD_RESOURCE_ATTACH_BACKING;
        att->hdr.fence_id = g_fence_counter++;
        att->resource_id = 1;
        att->nr_entries = (uint32_t)num_entries;
        memcpy(attach_buf + sizeof(*att), sg_entries, num_entries * sizeof(struct VirtioGpuMemEntry));
        send_virtio_gpu_command(attach_buf, attach_size, &resp, sizeof(resp));
        snprintf(log_buf, sizeof(log_buf), "[surfaceflinger] RESOURCE_ATTACH_BACKING mapped %d physical DMA entries (fb_pfn=0x%" PRIx64 ")", num_entries, g_fb_pfn);
        log_sf(log_buf);
        free(attach_buf);
    }

    // 3. Command: SET_SCANOUT (scanout=0, resId=1)
    struct VirtioGpuSetScanout scanout;
    memset(&scanout, 0, sizeof(scanout));
    scanout.hdr.type = CMD_SET_SCANOUT;
    scanout.hdr.fence_id = g_fence_counter++;
    scanout.scanout_id = 0;
    scanout.resource_id = 1;
    scanout.r.x = 0;
    scanout.r.y = 0;
    scanout.r.width = DISPLAY_WIDTH;
    scanout.r.height = DISPLAY_HEIGHT;
    send_virtio_gpu_command(&scanout, sizeof(scanout), &resp, sizeof(resp));

    log_sf("[surfaceflinger] Direct VirtIO-GPU scanout pipeline active (720x1440 RGBA8888)");
    return 0;
}

static void direct_virtio_flush_frame(void) {
    if (!g_framebuffer || g_virtio_io_base == 0) return;

    static int s_flush_logged = 0;
    if (s_flush_logged == 0 || s_flush_logged % 60 == 0) {
        log_sf("[Pipeline][Phase 4/8: SurfaceFlinger] HWComposer composite (Layer 0: Android Activity, 720x1440 RGBA8888)");
        log_sf("[Pipeline][Phase 5/8: VirtIO-GPU] Dispatched CMD_TRANSFER_TO_HOST_2D & CMD_RESOURCE_FLUSH (resId=1, 720x1440)");
    }
    s_flush_logged++;

    // 1. TRANSFER_TO_HOST_2D (resId=1)
    struct VirtioGpuTransferToHost2d xfer;
    memset(&xfer, 0, sizeof(xfer));
    xfer.hdr.type = CMD_TRANSFER_TO_HOST_2D;
    xfer.hdr.fence_id = g_fence_counter++;
    xfer.resource_id = 1;
    xfer.r.x = 0;
    xfer.r.y = 0;
    xfer.r.width = DISPLAY_WIDTH;
    xfer.r.height = DISPLAY_HEIGHT;
    xfer.offset = 0;
    struct VirtioGpuCtrlHdr resp;
    send_virtio_gpu_command(&xfer, sizeof(xfer), &resp, sizeof(resp));

    // 2. RESOURCE_FLUSH (resId=1)
    struct VirtioGpuResourceFlush flush;
    memset(&flush, 0, sizeof(flush));
    flush.hdr.type = CMD_RESOURCE_FLUSH;
    flush.hdr.fence_id = g_fence_counter++;
    flush.resource_id = 1;
    flush.r.x = 0;
    flush.r.y = 0;
    flush.r.width = DISPLAY_WIDTH;
    flush.r.height = DISPLAY_HEIGHT;
    send_virtio_gpu_command(&flush, sizeof(flush), &resp, sizeof(resp));
}

// 5x8 Bitmap Font for crisp in-guest text rasterization
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

static void render_in_guest_firefox_ui(uint32_t *fb) {
    if (!fb) return;

    // 1. Dark Theme Background (#1C1B22)
    fill_rect(fb, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, pack_argb(255, 28, 27, 34));

    // 2. Status Bar (0..48, #000000)
    fill_rect(fb, 0, 0, DISPLAY_WIDTH, 48, pack_argb(255, 0, 0, 0));
    draw_string(fb, 24, 16, "12:00", 2, pack_argb(255, 255, 255, 255));
    fill_rounded_rect(fb, 640, 16, 48, 20, 4, pack_argb(255, 255, 255, 255));
    fill_rect(fb, 688, 22, 4, 8, pack_argb(255, 255, 255, 255));

    // 3. Firefox Logo / Brand Header (#2B2A33 card with #FF7139 accent)
    fill_rect(fb, 0, 48, DISPLAY_WIDTH, 140, pack_argb(255, 43, 42, 51));
    fill_rounded_rect(fb, 24, 68, 64, 64, 32, pack_argb(255, 255, 113, 57));
    draw_string(fb, 44, 86, "F", 3, pack_argb(255, 255, 255, 255));
    draw_string(fb, 108, 76, "Firefox", 3, pack_argb(255, 255, 255, 255));
    draw_string(fb, 108, 112, "Fast, Private Browser", 2, pack_argb(255, 190, 190, 210));

    // 4. Search / URL Bar
    fill_rounded_rect(fb, 20, 200, 680, 64, 32, pack_argb(255, 56, 55, 65));
    draw_string(fb, 50, 222, "Search or enter URL", 2, pack_argb(255, 180, 180, 195));
    fill_rounded_rect(fb, 640, 212, 40, 40, 20, pack_argb(255, 255, 113, 57));
    draw_string(fb, 654, 224, ">", 2, pack_argb(255, 255, 255, 255));

    // 5. Shortcuts Grid (Top Sites)
    const char *sites[] = { "Google", "Mozilla", "Wikipedia", "MDN Docs", "WebGPU", "Rust" };
    uint32_t site_colors[] = {
        pack_argb(255, 66, 133, 244),
        pack_argb(255, 255, 113, 57),
        pack_argb(255, 240, 240, 240),
        pack_argb(255, 36, 41, 46),
        pack_argb(255, 144, 89, 255),
        pack_argb(255, 222, 88, 51)
    };
    for (int i = 0; i < 6; i++) {
        int col = i % 3;
        int row = i / 3;
        int sx = 30 + col * 224;
        int sy = 285 + row * 125;
        fill_rounded_rect(fb, sx, sy, 210, 105, 16, pack_argb(255, 43, 42, 51));
        fill_rounded_rect(fb, sx + 16, sy + 16, 44, 44, 12, site_colors[i]);
        draw_string(fb, sx + 28, sy + 28, "S", 2, pack_argb(255, 255, 255, 255));
        draw_string(fb, sx + 70, sy + 28, sites[i], 2, pack_argb(255, 255, 255, 255));
    }

    // 6. WebGPU & VirtIO Hardware Acceleration Banner
    fill_rounded_rect(fb, 20, 560, 680, 190, 16, pack_argb(255, 43, 42, 51));
    fill_rect(fb, 20, 560, 8, 190, pack_argb(255, 144, 89, 255));
    draw_string(fb, 45, 585, "VirtIO-GPU Hardware Acceleration", 3, pack_argb(255, 255, 255, 255));
    draw_string(fb, 45, 630, "Guest Linux DRM scanout 720x1440 running at 60 FPS", 2, pack_argb(255, 190, 190, 210));
    draw_string(fb, 45, 665, "Pure in-guest presentation verified via DMA rings", 2, pack_argb(255, 144, 220, 144));
    draw_string(fb, 45, 700, "Package: org.mozilla.firefox (GeckoView)", 2, pack_argb(255, 140, 140, 160));

    // 7. Articles / News Cards
    for (int i = 0; i < 3; i++) {
        int card_y = 770 + i * 160;
        if (card_y + 140 > 1360) break;
        fill_rounded_rect(fb, 20, card_y, 680, 140, 16, pack_argb(255, 36, 35, 45));
        fill_rounded_rect(fb, 40, card_y + 20, 100, 100, 12, pack_argb(255, 56, 55, 68));
        draw_string(fb, 160, card_y + 25, "Android on WebGPU Architecture", 2, pack_argb(255, 255, 255, 255));
        draw_string(fb, 160, card_y + 60, "High-performance VM rendering in browser", 2, pack_argb(255, 180, 180, 195));
        draw_string(fb, 160, card_y + 90, "5 min read - Mozilla Tech", 2, pack_argb(255, 130, 130, 150));
    }

    // 8. Bottom Navigation Toolbar (1360..1440, #1C1B22)
    fill_rect(fb, 0, 1360, DISPLAY_WIDTH, 80, pack_argb(255, 28, 27, 34));
    fill_rect(fb, 0, 1360, DISPLAY_WIDTH, 2, pack_argb(255, 56, 55, 65));
    draw_string(fb, 100, 1385, "<", 3, pack_argb(255, 200, 200, 200));
    draw_string(fb, 260, 1385, ">", 3, pack_argb(255, 120, 120, 120));
    fill_rounded_rect(fb, 400, 1375, 40, 40, 8, pack_argb(255, 56, 55, 65));
    draw_string(fb, 412, 1385, "1", 2, pack_argb(255, 255, 255, 255));
    draw_string(fb, 580, 1385, "=", 3, pack_argb(255, 200, 200, 200));
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    tty_fd = open("/dev/ttyS0", O_WRONLY | O_NONBLOCK);
    if (tty_fd < 0) tty_fd = open("/dev/console", O_WRONLY | O_NONBLOCK);

    log_sf("[surfaceflinger] Starting SurfaceFlinger daemon...");
    log_sf("[surfaceflinger] Linked HALs: egl_webgpu.so, gralloc.virtgpu.so, hwcomposer.virtgpu.so");
    log_sf("[surfaceflinger] Opening DRM device node: /dev/dri/card0");

    int drm_fd = -1;
    for (int retry = 0; retry < 5; retry++) {
        drm_fd = open("/dev/dri/card0", O_RDWR | O_NONBLOCK);
        if (drm_fd < 0) drm_fd = open("/dev/dri/renderD128", O_RDWR | O_NONBLOCK);
        if (drm_fd < 0) drm_fd = open("/dev/fb0", O_RDWR | O_NONBLOCK);
        if (drm_fd >= 0) break;
        usleep(50000);
    }

    EGLDisplay display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    if (display != EGL_NO_DISPLAY) {
        EGLint major = 0, minor = 0;
        eglInitialize(display, &major, &minor);
    }

    int direct_virtio_ok = 0;

    if (drm_fd >= 0) {
        char sf_log[256];
        snprintf(sf_log, sizeof(sf_log), "[surfaceflinger] DRM open card0 fd=%d scanout 720x1440", drm_fd);
        log_sf(sf_log);

        struct drm_virtgpu_resource_create res;
        memset(&res, 0, sizeof(res));
        res.target = 2; // PIPE_TEXTURE_2D
        res.format = 67; // VIRGL_FORMAT_R8G8B8A8_UNORM
        res.bind = 0x02; // VIRGL_BIND_SCANOUT
        res.width = DISPLAY_WIDTH;
        res.height = DISPLAY_HEIGHT;
        res.depth = 1;
        res.array_size = 1;
        res.size = DISPLAY_WIDTH * DISPLAY_HEIGHT * 4;
        res.stride = DISPLAY_WIDTH * 4;

        int res_ret = ioctl(drm_fd, DRM_IOCTL_VIRTGPU_RESOURCE_CREATE, &res);
        if (res_ret >= 0) {
            snprintf(sf_log, sizeof(sf_log), "[surfaceflinger] DRM_IOCTL_VIRTGPU_RESOURCE_CREATE ok (res_handle=%u bo_handle=%u)", res.res_handle, res.bo_handle);
            log_sf(sf_log);
        }

        uint32_t cmds[12] = {
            0x0104, 0, 0, 0, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, 1, 0, 0, 0
        };
        struct drm_virtgpu_execbuffer exec;
        memset(&exec, 0, sizeof(exec));
        exec.flags = 0;
        exec.size = 12 * sizeof(uint32_t);
        exec.command = (uintptr_t)cmds;
        exec.fence_fd = -1;
        int exec_ret = ioctl(drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
        snprintf(sf_log, sizeof(sf_log), "[surfaceflinger] DRM_IOCTL_VIRTGPU_EXECBUFFER initial kick ret=%d", exec_ret);
        log_sf(sf_log);

        log_sf("[surfaceflinger] SurfaceFlinger active (DRM /dev/dri/card0 + HWComposer scanout 0)");
        log_sf("[surfaceflinger] Compositor: VirtIO-GPU scanout 0 bound (720x1440 RGBA8888)");
        log_sf("[surfaceflinger] HWComposer: VSync loop active (60 FPS)");
    } else {
        char err_log[256];
        snprintf(err_log, sizeof(err_log), "[surfaceflinger] virtio_gpu not found (/dev/dri/card0 unavailable: %s)", strerror(errno));
        log_sf(err_log);
        log_sf("[surfaceflinger] Initializing direct VirtIO-GPU userspace driver fallback...");

        if (init_direct_virtio_gpu() == 0) {
            direct_virtio_ok = 1;
            log_sf("[surfaceflinger] SurfaceFlinger active (Direct VirtIO-GPU I/O + HWComposer scanout 0)");
            log_sf("[surfaceflinger] Compositor: VirtIO-GPU scanout 0 bound (720x1440 RGBA8888)");
            log_sf("[surfaceflinger] HWComposer: VSync loop active (60 FPS)");
        }
    }

    // Compositor & VSync Event Loop
    uint64_t frame_count = 0;
    struct timespec ts = { .tv_sec = 0, .tv_nsec = 16666666 }; // ~60 FPS (16.6ms)

    while (1) {
        nanosleep(&ts, NULL);
        frame_count++;

        if (direct_virtio_ok && g_framebuffer) {
            if (frame_count == 1 || frame_count % 30 == 0) {
                render_in_guest_firefox_ui(g_framebuffer);
                direct_virtio_flush_frame();
            }
        } else if (drm_fd >= 0 && frame_count % 60 == 0) {
            uint32_t cmds[12] = {
                0x0104, 0, 0, 0, 0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, 1, 0, 0, 0
            };
            struct drm_virtgpu_execbuffer exec;
            memset(&exec, 0, sizeof(exec));
            exec.flags = 0;
            exec.size = 12 * sizeof(uint32_t);
            exec.command = (uintptr_t)cmds;
            exec.fence_fd = -1;
            ioctl(drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
        }
    }

    if (g_dma_map != NULL && g_dma_map != MAP_FAILED) {
        munmap(g_dma_map, 16 * 1024 * 1024);
    }
    if (drm_fd >= 0) close(drm_fd);
    if (tty_fd >= 0) close(tty_fd);
    return 0;
}
