#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <glib.h>
#include <qemu-plugin.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static FILE *out;
static struct qemu_plugin_register *handles[64];
static char names[64][32];
static size_t nregs;
static uint64_t executed;
static uint64_t window_start = 39990;
static uint64_t window_end = 55000;

struct insn_ref { uint64_t pc; uint32_t word; };

static int want(const char *s) {
    if (!strcmp(s, "sp") || !strcmp(s, "pc") || !strcmp(s, "pstate") || !strcmp(s, "cpsr")) return 1;
    if (!strcmp(s, "SCTLR_EL1") || !strcmp(s, "TCR_EL1") || !strcmp(s, "TTBR0_EL1") || !strcmp(s, "TTBR1_EL1") || !strcmp(s, "MAIR_EL1") || !strcmp(s, "SP_EL0") || !strcmp(s, "TPIDR_EL1") || !strcmp(s, "TPIDR_EL0") || !strcmp(s, "TPIDRRO_EL0") || !strcmp(s, "DAIF") || !strcmp(s, "VBAR_EL1") || !strcmp(s, "ELR_EL1") || !strcmp(s, "SPSR_EL1")) return 1;
    if (s[0] == 'x' && s[1] >= '0' && s[1] <= '9') {
        char *end = NULL;
        long n = strtol(s + 1, &end, 10);
        return end && *end == '\0' && n >= 0 && n <= 30;
    }
    return 0;
}

static uint64_t read_reg(struct qemu_plugin_register *h) {
    GByteArray *b = g_byte_array_new();
    uint64_t v = 0;
    if (qemu_plugin_read_register(h, b) && b->len) {
        size_t n = b->len < sizeof(v) ? b->len : sizeof(v);
        memcpy(&v, b->data, n);
    }
    g_byte_array_unref(b);
    return v;
}

static void write_state(uint64_t step, uint64_t pc, uint32_t word) {
    fprintf(out, "I step=%" PRIu64 " insn_pc=0x%016" PRIx64 " word=0x%08" PRIx32, step, pc, word);
    for (size_t i = 0; i < nregs; i++) {
        fprintf(out, " %s=0x%016" PRIx64, names[i], read_reg(handles[i]));
    }
    fputc('\n', out);
    fflush(out);
}

static void exec_cb(unsigned int vcpu, void *opaque) {
    (void)vcpu;
    struct insn_ref *insn = (struct insn_ref *)opaque;
    uint64_t step = ++executed;
    if (step >= window_start && step <= window_end) {
        write_state(step, insn->pc, insn->word);
    }
    if (step > window_end) {
        if (out) {
            fflush(out);
            fclose(out);
            out = NULL;
        }
        exit(0);
    }
}

static void tb_trans(struct qemu_plugin_tb *tb, void *opaque) {
    (void)opaque;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        struct insn_ref *ref = (struct insn_ref *)calloc(1, sizeof(*ref));
        uint8_t bytes[4] = {0};
        ref->pc = qemu_plugin_insn_vaddr(insn);
        if (qemu_plugin_insn_data(insn, bytes, 4) == 4) {
            ref->word = (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
                        ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
        }
        qemu_plugin_register_vcpu_insn_exec_cb(insn, exec_cb, QEMU_PLUGIN_CB_R_REGS, ref);
    }
}

static void vcpu_init(unsigned int cpu, void *opaque) {
    (void)cpu;
    (void)opaque;
    GArray *r = qemu_plugin_get_registers();
    if (!r) return;
    for (guint i = 0; i < r->len; i++) {
        qemu_plugin_reg_descriptor d = g_array_index(r, qemu_plugin_reg_descriptor, i);
        if (d.name && want(d.name) && nregs < G_N_ELEMENTS(handles)) {
            handles[nregs] = d.handle;
            snprintf(names[nregs], sizeof(names[nregs]), "%s", d.name);
            nregs++;
        }
    }
    g_array_free(r, TRUE);
}

static void at_exit(void *opaque) {
    (void)opaque;
    if (out) {
        fflush(out);
        fclose(out);
        out = NULL;
    }
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    (void)argc;
    (void)argv;
    if (!info || !info->system_emulation) return 1;
    const char *out_path = getenv("QEMU_TRACE_OUT");
    if (!out_path) out_path = "/Users/Shared/agyx-logs/track574-qemu-40k-55k.log";
    const char *start_str = getenv("QEMU_TRACE_START");
    if (start_str) window_start = strtoull(start_str, NULL, 10);
    const char *end_str = getenv("QEMU_TRACE_END");
    if (end_str) window_end = strtoull(end_str, NULL, 10);
    out = fopen(out_path, "w");
    if (!out) return 1;
    fprintf(out, "TRACE format=sampled-instruction-state-v2 count=one-based snapshot_period=1 exact_window=%" PRIu64 "..%" PRIu64 " target=%s api=%d vcpus=%d\n",
            window_start, window_end, info->target_name, info->version.cur, info->system.smp_vcpus);
    fflush(out);
    qemu_plugin_register_vcpu_init_cb(id, vcpu_init, NULL);
    qemu_plugin_register_vcpu_tb_trans_cb(id, tb_trans, NULL);
    qemu_plugin_register_atexit_cb(id, at_exit, NULL);
    return 0;
}
