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
#include <sys/syscall.h>
#include <dirent.h>

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

    // echo command (with $(basename ...) expansion for init module logs)
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
        char *bpat = strstr(clean_msg, "$(basename");
        if (bpat) {
            char *ps = strchr(bpat, '"');
            if (!ps) ps = strchr(bpat, '\'');
            if (ps) {
                ps++;
                char *pe = strchr(ps, '"');
                if (!pe) pe = strchr(ps, '\'');
                if (!pe) pe = strchr(ps, ')');
                if (pe) {
                    *pe='\0';
                    char *sl = strrchr(ps, '/');
                    const char *base = sl ? sl+1 : ps;
                    char exp[512];
                    size_t pre = bpat - clean_msg;
                    strncpy(exp, clean_msg, pre);
                    exp[pre]='\0';
                    strncat(exp, base, sizeof(exp)-strlen(exp)-1);
                    char *after = strchr(pe+1, ')');
                    if (!after) after = pe+1; else after++;
                    strncat(exp, after, sizeof(exp)-strlen(exp)-1);
                    strncpy(clean_msg, exp, sizeof(clean_msg)-1);
                }
            }
        }
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

    // device nodes - conditional fallback with logging (don't hide ENODEV)
    if (strstr(line, "mknod") || strstr(line, "/dev/dri") || strstr(line, "/dev/fb0")) {
        struct stat st;
        if (stat("/dev/dri/card0", &st) != 0) {
            log_line("[sh] WARNING: /dev/dri/card0 missing after driver load - creating dummy fallback node (ENODEV if driver not bound)");
            mknod("/dev/dri/card0", S_IFCHR | 0666, makedev(226, 0));
        } else {
            log_line("[sh] /dev/dri/card0 exists (driver/devtmpfs) - skip dummy mknod");
        }
        if (stat("/dev/dri/renderD128", &st) != 0) mknod("/dev/dri/renderD128", S_IFCHR | 0666, makedev(226, 128));
        if (stat("/dev/fb0", &st) != 0) mknod("/dev/fb0", S_IFCHR | 0666, makedev(29, 0));
        mknod("/dev/ttyS0", S_IFCHR | 0666, makedev(4, 64));
    }

    // insmod/modprobe via direct init_module syscall (no external binary in initrd) + verbose logs
    // [VERBOSE FIX 2026-08-29] skip echo lines containing "insmod" substring (prevents false "attempting: ..." noise)
    // and handle multiple insmod tokens per line (e.g. "insmod A || insmod B")
    if ((strstr(line, "insmod ") || strstr(line, "modprobe "))) {
        // Skip if line is an echo statement that merely mentions insmod (e.g. echo "... insmod ...")
        char *echo_chk = strstr(line, "echo ");
        char *ins_chk = strstr(line, "insmod ");
        if (echo_chk && ins_chk && echo_chk < ins_chk) {
            // echo line mentioning insmod -> treat as log_line already handled, not real insmod
            char lb_skip[512];
            snprintf(lb_skip,sizeof(lb_skip),"[sh][VERBOSE] skip insmod parse on echo line: %.120s", line);
            log_line(lb_skip);
        } else {
            // Loop over all insmod/modprobe occurrences in line (handles "insmod A || insmod B")
            char *search = line;
            int mod_idx=0;
            while (search) {
                char *found_ins = strstr(search, "insmod ");
                char *found_mod = strstr(search, "modprobe ");
                char *ms=NULL;
                char keyword[16]={0};
                if (found_ins && (!found_mod || found_ins < found_mod)) { ms=found_ins+7; strcpy(keyword,"insmod"); search=found_ins+7; }
                else if (found_mod) { ms=found_mod+9; strcpy(keyword,"modprobe"); search=found_mod+9; }
                else break;
                // skip spaces/quotes
                while (*ms==' '||*ms=='"'||*ms=='\'') ms++;
                char mod_path[256]={0};
                int mi=0;
                while (ms[mi] && ms[mi]!=' '&&ms[mi]!='"'&&ms[mi]!='\''&&ms[mi]!=';'&&ms[mi]!='&'&&ms[mi]!='>'&&ms[mi]!='|'&&mi<255){mod_path[mi]=ms[mi];mi++;}
                mod_path[mi]='\0';
                int ml=strlen(mod_path);
                while(ml>0&&(mod_path[ml-1]=='"'||mod_path[ml-1]=='\'')) mod_path[--ml]='\0';
                if (ml==0) continue;
                // Skip placeholder $MODDIR or "..." or variable remnants
                if (mod_path[0]=='$' || strstr(mod_path, "...") || strcmp(mod_path,"...")==0) {
                    char lb_var[512];
                    snprintf(lb_var,sizeof(lb_var),"[sh][VERBOSE] skip variable/placeholder path: %s (sh.c no $ expansion)",mod_path);
                    log_line(lb_var);
                    continue;
                }
                // Extract module parameters (e.g. force_legacy=1)
                char mod_args[256] = {0};
                char *arg_pos = ms + mi;
                while (*arg_pos && *arg_pos != ';' && *arg_pos != '&' && *arg_pos != '|' && *arg_pos != '>' && *arg_pos != '<') {
                    while (*arg_pos == ' ') arg_pos++;
                    if (!*arg_pos || *arg_pos == ';' || *arg_pos == '&' || *arg_pos == '|' || *arg_pos == '>' || *arg_pos == '<') break;
                    char token[128] = {0};
                    int ti = 0;
                    while (arg_pos[ti] && arg_pos[ti] != ' ' && arg_pos[ti] != ';' && arg_pos[ti] != '&' && arg_pos[ti] != '|' && arg_pos[ti] != '>' && arg_pos[ti] != '<' && ti < 127) {
                        token[ti] = arg_pos[ti];
                        ti++;
                    }
                    token[ti] = '\0';
                    arg_pos += ti;
                    if (strcmp(token, "2") == 0 && (*arg_pos == '>' || *arg_pos == '<')) break;
                    if (strchr(token, '>') || strchr(token, '<')) break;
                    if (ti > 0 && strcmp(token, "2") != 0 && strcmp(token, "1") != 0) {
                        if (strlen(mod_args) > 0 && strlen(mod_args) + ti + 2 < sizeof(mod_args)) {
                            strcat(mod_args, " ");
                            strcat(mod_args, token);
                        } else if (ti < sizeof(mod_args)) {
                            strcpy(mod_args, token);
                        }
                    }
                }

                // Automatically provide force_legacy=1 for virtio_pci.ko
                if (strstr(mod_path, "virtio_pci.ko")) {
                    if (strlen(mod_args) == 0) {
                        strcpy(mod_args, "force_legacy=1");
                    } else if (!strstr(mod_args, "force_legacy")) {
                        strcat(mod_args, " force_legacy=1");
                    }
                }

                mod_idx++;
                char lb[512];
                snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] %s attempting: %s (args: '%s')", mod_idx, keyword, mod_path, mod_args);
                log_line(lb);
                int fd=open(mod_path,O_RDONLY);
                if (fd>=0) {
                    struct stat st;
                    if (fstat(fd,&st)==0) {
                        void *buf=malloc(st.st_size);
                        if (buf) {
                            ssize_t r=read(fd,buf,st.st_size);
                            close(fd);
                            if (r==st.st_size) {
                                long ret=syscall(128, buf, r, mod_args);
                                if (ret==0) { snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] %s %s -> OK (%ld bytes) [OK]",mod_idx, keyword, mod_path,r); log_line(lb); }
                                else { 
                                    snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] init_module %s -> ret=%ld errno=%d (%s) [FAIL]",mod_idx, mod_path,ret,errno,strerror(errno)); 
                                    log_line(lb); 
                                    char kbuf[2048];
                                    long sz = syscall(103, 3, kbuf, sizeof(kbuf) - 1);
                                    if (sz > 0) {
                                        kbuf[sz] = '\0';
                                        char *tail = sz > 300 ? kbuf + (sz - 300) : kbuf;
                                        for (int i = 0; tail[i]; i++) if (tail[i] == '\n') tail[i] = ' ';
                                        char lb_k[512];
                                        snprintf(lb_k, sizeof(lb_k), "[sh][VERBOSE] dmesg tail: %s", tail);
                                        log_line(lb_k);
                                    }
                                }
                            } else { snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] read %s failed %ld",mod_idx, mod_path,r); log_line(lb); close(fd); }
                            free(buf);
                        } else { close(fd); snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] malloc fail for %s",mod_idx, mod_path); log_line(lb); }
                    } else { snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] fstat %s fail %d",mod_idx, mod_path,errno); log_line(lb); close(fd); }
                } else {
                    snprintf(lb,sizeof(lb),"[sh][VERBOSE][%d] open %s fail errno=%d (%s) [FAIL]",mod_idx, mod_path,errno,strerror(errno));
                    log_line(lb);
                }
            }
        }
    }

    // Diagnostic handlers for pure-guest debugging (ls, cat, dmesg, lsmod, lspci, pci rescan)
    // These provide host-visible logs via log_line since bare binaries don't exist in initrd
    if (strstr(line, "cat /proc/bus/pci/devices") || strstr(line, "cat /proc/bus/pci/")) {
        int fd=open("/proc/bus/pci/devices", O_RDONLY);
        if (fd>=0) { char buf[4096]; ssize_t r=read(fd,buf,sizeof(buf)-1); if (r>0){buf[r]='\0'; char lb2[8192]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /proc/bus/pci/devices (%ld bytes):\n%s",r,buf); log_line(lb2);} close(fd);} else { log_line("[sh][DIAG] cat /proc/bus/pci/devices: open fail"); }
    }
    if (strstr(line, "ls -la /sys/class/drm") || strstr(line, "ls -A /sys/class/drm")) {
        // Directly list /sys/class/drm via opendir for reliable diagnostics
        DIR *d=opendir("/sys/class/drm");
        if (d){ char lb2[2048]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/class/drm entries:"); struct dirent *e; while((e=readdir(d))){ if(e->d_name[0]=='.') continue; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); } closedir(d); log_line(lb2); } else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] opendir /sys/class/drm fail errno=%d",errno); log_line(lb2); }
    }
    if (strstr(line, "ls -la /dev/dri") ) {
        DIR *d=opendir("/dev/dri");
        if (d){ char lb2[2048]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /dev/dri entries:"); struct dirent *e; while((e=readdir(d))){ if(e->d_name[0]=='.') continue; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); } closedir(d); log_line(lb2); } else { log_line("[sh][DIAG] opendir /dev/dri fail"); }
    }
    if (strstr(line, "ls -la /sys/bus/pci/devices")) {
        DIR *d=opendir("/sys/bus/pci/devices");
        if (d){ char lb2[4096]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/bus/pci/devices:"); struct dirent *e; int c=0; while((e=readdir(d)) && c<20){ if(e->d_name[0]=='.') continue; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); c++; } closedir(d); log_line(lb2); } else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] opendir /sys/bus/pci fail errno=%d",errno); log_line(lb2); }
    }
    if (strstr(line, "cat /proc/ioports") || strstr(line, "/proc/ioports")) {
        int fd=open("/proc/ioports", O_RDONLY);
        if (fd>=0){ char buf[8192]; ssize_t r=read(fd,buf,sizeof(buf)-1); if(r>0){buf[r]='\0'; log_line("[sh][DIAG] /proc/ioports start"); int off=0; while(off<r){ int chunk=400; if(off+chunk>r) chunk=r-off; char piece[512]; strncpy(piece, buf+off, chunk); piece[chunk]='\0'; for(int i=0;i<chunk;i++) if(piece[i]=='\n') piece[i]='|'; char lb2[600]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] ioports chunk %d: %s", off/400, piece); log_line(lb2); off+=chunk; } log_line("[sh][DIAG] /proc/ioports end");} else {log_line("[sh][DIAG] /proc/ioports empty");} close(fd);} else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /proc/ioports open fail errno=%d",errno); log_line(lb2); }
    }
    if (strstr(line, "cat /sys/bus/pci/devices/0000:00:06.0/resource") || strstr(line, "/sys/bus/pci/devices/0000:00:06.0/resource")) {
        int fd=open("/sys/bus/pci/devices/0000:00:06.0/resource", O_RDONLY);
        if (fd>=0){ char buf[4096]; ssize_t r=read(fd,buf,sizeof(buf)-1); if(r>0){buf[r]='\0'; log_line("[sh][DIAG] resource start"); int off=0; while(off<r){ int chunk=400; if(off+chunk>r) chunk=r-off; char piece[512]; strncpy(piece, buf+off, chunk); piece[chunk]='\0'; for(int i=0;i<chunk;i++) if(piece[i]=='\n') piece[i]='|'; char lb2[600]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] resource chunk %d: %s", off/400, piece); log_line(lb2); off+=chunk; } log_line("[sh][DIAG] resource end");} else {log_line("[sh][DIAG] resource empty");} close(fd);} else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] resource open fail errno=%d",errno); log_line(lb2); }
    }
    if (strstr(line, "ls -la /sys/bus/virtio") || strstr(line, "/sys/bus/virtio")) {
        DIR *d=opendir("/sys/bus/virtio/devices");
        if (d){ char lb2[2048]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/bus/virtio/devices entries:"); struct dirent *e; int c=0; while((e=readdir(d))){ if(e->d_name[0]=='.') continue; if(c++>20) break; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); } closedir(d); if(c==0) strncat(lb2," (empty)",sizeof(lb2)-strlen(lb2)-1); log_line(lb2);} else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] opendir /sys/bus/virtio/devices fail errno=%d",errno); log_line(lb2); }
        DIR *d2=opendir("/sys/bus/virtio/drivers");
        if (d2){ char lb2[2048]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/bus/virtio/drivers entries:"); struct dirent *e; int c=0; while((e=readdir(d2))){ if(e->d_name[0]=='.') continue; if(c++>20) break; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); } closedir(d2); if(c==0) strncat(lb2," (empty)",sizeof(lb2)-strlen(lb2)-1); log_line(lb2); } else { log_line("[sh][DIAG] opendir /sys/bus/virtio/drivers fail"); }
        DIR *d3=opendir("/sys/bus/pci/drivers/virtio-pci");
        if (d3){ char lb2[2048]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/bus/pci/drivers/virtio-pci entries:"); struct dirent *e; int c=0; while((e=readdir(d3))){ if(e->d_name[0]=='.') continue; if(c++>20) break; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); } closedir(d3); if(c==0) strncat(lb2," (empty) -> driver not bound!",sizeof(lb2)-strlen(lb2)-1); log_line(lb2);} else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] opendir /sys/bus/pci/drivers/virtio-pci fail errno=%d",errno); log_line(lb2); }
        DIR *d4=opendir("/sys/bus/pci/drivers/virtio_gpu");
        if (d4){ char lb2[2048]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/bus/pci/drivers/virtio_gpu entries:"); struct dirent *e; int c=0; while((e=readdir(d4))){ if(e->d_name[0]=='.') continue; if(c++>20) break; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); } closedir(d4); if(c==0) strncat(lb2," (empty)",sizeof(lb2)-strlen(lb2)-1); log_line(lb2);} else { /* not an error, virtio_gpu may be under virtio bus */ }
    }
    if (strstr(line, "ls -la /sys/bus/pci/devices/0000:00:06.0/driver") || strstr(line, "/sys/bus/pci/devices/0000:00:06.0/driver")) {
        char link[512]; ssize_t lr=readlink("/sys/bus/pci/devices/0000:00:06.0/driver", link, sizeof(link)-1);
        if (lr>0){ link[lr]='\0'; char lb2[1024]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] /sys/bus/pci/devices/0000:00:06.0/driver -> %s",link); log_line(lb2); } else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] driver symlink FAIL errno=%d -> device not bound (ENODEV root cause)",errno); log_line(lb2); }
        DIR *d=opendir("/sys/bus/pci/devices/0000:00:06.0"); if(d){ char lb2[4096]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] 00:06.0 entries:"); struct dirent *e; int c=0; while((e=readdir(d)) && c<30){ if(e->d_name[0]=='.') continue; strncat(lb2," ",sizeof(lb2)-strlen(lb2)-1); strncat(lb2,e->d_name,sizeof(lb2)-strlen(lb2)-1); c++; } closedir(d); log_line(lb2); } else { log_line("[sh][DIAG] opendir 00:06.0 fail"); }
        int fd2=open("/sys/bus/pci/devices/0000:00:06.0/enable", O_RDONLY); if(fd2>=0){ char b2[32]; ssize_t r2=read(fd2,b2,sizeof(b2)-1); if(r2>0){b2[r2]='\0'; char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] 00:06.0 enable=%s",b2); log_line(lb2);} close(fd2);} 
        int fd3=open("/sys/bus/pci/devices/0000:00:06.0/resource", O_RDONLY); if(fd3>=0){ char b3[512]; ssize_t r3=read(fd3,b3,sizeof(b3)-1); if(r3>0){b3[r3]='\0'; char lb2[1024]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] 00:06.0 resource inline: %.400s", b3); log_line(lb2);} close(fd3);} 
        int fd_mod=open("/sys/bus/pci/devices/0000:00:06.0/modalias", O_RDONLY); if(fd_mod>=0){ char b_mod[128]; ssize_t r_mod=read(fd_mod,b_mod,sizeof(b_mod)-1); if(r_mod>0){ b_mod[r_mod]='\0'; char lb_mod[256]; snprintf(lb_mod,sizeof(lb_mod),"[sh][DIAG] 00:06.0 modalias=%s",b_mod); log_line(lb_mod); } close(fd_mod); }
        int fd_irq=open("/proc/interrupts", O_RDONLY); if(fd_irq>=0){ char b_irq[2048]; ssize_t r_irq=read(fd_irq,b_irq,sizeof(b_irq)-1); if(r_irq>0){ b_irq[r_irq]='\0'; char lb_irq[2048]; snprintf(lb_irq,sizeof(lb_irq),"[sh][DIAG] /proc/interrupts: %.1000s",b_irq); log_line(lb_irq); } close(fd_irq); }
    }
    if (strstr(line, "lspci") ) {
        // Emulate lspci via /proc/bus/pci/devices hex dump
        int fd=open("/proc/bus/pci/devices", O_RDONLY);
        if (fd>=0){ char buf[4096]; ssize_t r=read(fd,buf,sizeof(buf)-1); if(r>0){buf[r]='\0'; char lb2[8192]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] lspci (via /proc/bus/pci/devices):\n%s",buf); log_line(lb2);} close(fd);} else { log_line("[sh][DIAG] lspci: no pci devices file"); }
    }
    if (strstr(line, "lsmod")) {
        int fd=open("/proc/modules", O_RDONLY);
        if (fd>=0){ char buf[8192]; ssize_t r=read(fd,buf,sizeof(buf)-1); if(r>0){buf[r]='\0'; char lb2[8192]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] lsmod (/proc/modules %ld bytes):\n%s",r,buf); log_line(lb2);} else {log_line("[sh][DIAG] lsmod: /proc/modules empty");} close(fd);} else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] lsmod open fail errno=%d",errno); log_line(lb2); }
    }
    if (strstr(line, "dmesg") ) {
        // Use syslog syscall (klogctl) to fetch kernel log - type 3 = read all, large buffer
        char kbuf[32768]; long sz=syscall(103, 3, kbuf, sizeof(kbuf)-1); // SYS_syslog
        if (sz>0){ kbuf[sz]='\0'; // log in chunks to avoid truncated log_line (512 limit)
            log_line("[sh][DIAG] dmesg klogctl start");
            // Chunk into 400 char pieces for log_line limit
            int off=0;
            while(off<sz){
                int chunk=400;
                if(off+chunk>sz) chunk=sz-off;
                char piece[512]; strncpy(piece, kbuf+off, chunk); piece[chunk]='\0';
                // escape newlines for single line log?
                for(int i=0;i<chunk;i++) if(piece[i]=='\n') piece[i]=' ';
                char lb2[600]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] dmesg chunk %d: %s", off/400, piece);
                log_line(lb2);
                off+=chunk;
            }
            log_line("[sh][DIAG] dmesg klogctl end");
        } else {
            int fd=open("/dev/kmsg", O_RDONLY | O_NONBLOCK);
            if (fd>=0){ char buf[8192]; ssize_t r=read(fd,buf,sizeof(buf)-1); if(r>0){buf[r]='\0'; char lb2[8192]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] dmesg /dev/kmsg (%ld bytes tail):\n%.*s",r,4000,buf + (r>4000?r-4000:0)); log_line(lb2);} else {log_line("[sh][DIAG] dmesg /dev/kmsg empty");} close(fd);}
            else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] dmesg fail klogctl ret=%ld errno=%d",sz,errno); log_line(lb2); }
        }
    }
    if (strstr(line, "/sys/bus/pci/rescan")) {
        int fd=open("/sys/bus/pci/rescan", O_WRONLY);
        if (fd>=0){ ssize_t w=write(fd,"1",1); char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] pci rescan write 1 -> %ld errno=%d",w,errno); log_line(lb2); close(fd);} else { char lb2[256]; snprintf(lb2,sizeof(lb2),"[sh][DIAG] pci rescan open fail errno=%d",errno); log_line(lb2); }
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
    // dummy DRM nodes deferred to /init after driver load (avoid hiding ENODEV)
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
