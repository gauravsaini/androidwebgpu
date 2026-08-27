# Project: Android x86 Direct Kernel Boot, Serial Milestones & BinderFS Standup

## Architecture
The AndroidWebGPU system boots an authentic 32-bit x86 Linux kernel (`guest/build/bzImage`) with an Android initial ramdisk (`guest/build/initrd.img`) inside the v86 WebAssembly PC hypervisor:
1. **Direct Kernel Boot**: `SystemBootstrap` and `V86GuestManager` configure v86 to load `bzImage` (Linux boot protocol 2.15) and `initrd.img` into RAM directly without legacy CD-ROM/ISO emulation.
2. **Serial Telemetry & Logcat Streaming**: UART 8250 (`/dev/ttyS0`) emits early kernel dmesg and init logs. `V86GuestManager` buffers serial output, forwards to `globalLogcat` under tag `[v86Guest]`, and feeds the reactive state machine.
3. **VM Lifecycle & Milestones**: Boot milestones (`BIOS_POST` -> `KERNEL_BOOT` -> `KERNEL_UNCOMPRESS` -> `KERNEL_READY` -> `BINDERFS_MOUNT` -> `INIT_USERSPACE` -> `SERVICEMANAGER_READY` -> `RUST_SERVICES_READY` -> `SYSTEM_BOOT_COMPLETED` -> `RUNNING`) advance exclusively based on parsed serial milestones with zero synthetic timeouts.
4. **BinderFS & Native Services**: Guest PID 1 `/init` mounts `/dev/binderfs`, establishes `/dev/binder`, `/dev/hwbinder`, `/dev/vndbinder` links, starts `servicemanager` (Handle 0 via `BINDER_SET_CONTEXT_MGR`), and launches native Rust services (`pms_rs`, `ams_rs`, `wms_rs`, `inputflinger_rs`).

```
  ┌────────────────────────────────────────────────────────┐
  │ v86 PC Hypervisor (x86 32-bit WASM Engine)             │
  │  - guest/build/bzImage (Linux 5.10.0-android-x86)       │
  │  - guest/build/initrd.img (cpio newc rootfs)           │
  └──────────────────────────┬─────────────────────────────┘
                             │ /dev/ttyS0 (UART 8250)
                             ▼
  ┌────────────────────────────────────────────────────────┐
  │ V86GuestManager / SystemBootstrap                      │
  │  - Serial Stream Bufferer                              │
  │  - Tag [v86Guest] Logcat Streamer                      │
  │  - Reactive Boot Milestone State Machine               │
  └──────────────────────────┬─────────────────────────────┘
                             │ VM States & Dmesg Events
                             ▼
  ┌────────────────────────────────────────────────────────┐
  │ Android AOSP System Services (Guest Userspace)         │
  │  - BinderFS (/dev/binderfs -> /dev/binder)             │
  │  - ServiceManager (Handle 0)                           │
  │  - pms_rs / ams_rs / wms_rs / inputflinger_rs          │
  └────────────────────────────────────────────────────────┘
```

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Direct Kernel Boot Configuration | Boot v86 from `bzImage` and `initrd.img` with explicit cmdline and zero ISO reliance | M1 | ORIGINAL_REQUEST §R1 |
| 2 | Serial Console Stream Capture & Logcat Forwarding | Capture `/dev/ttyS0` characters/bytes and forward to Logcat under tag `[v86Guest]` | M1 | ORIGINAL_REQUEST §R2 |
| 3 | Reactive Boot Milestone State Machine | Advance 9 VM states through 11 real serial-parsed milestones with zero artificial delays | M1 | ORIGINAL_REQUEST §R2 |
| 4 | BinderFS Mount & Character Device Setup | Mount `/dev/binderfs` and create `/dev/binder` nodes with 0666 permissions | M2 | ORIGINAL_REQUEST §R3 |
| 5 | ServiceManager Root Context Standup | Start `servicemanager` and register handle 0 via `BINDER_SET_CONTEXT_MGR` | M2 | ORIGINAL_REQUEST §R3 |
| 6 | Native Rust System Services Standup | Launch `pms_rs`, `ams_rs`, `wms_rs`, `inputflinger_rs`, and virtual HAL daemons | M2 | ORIGINAL_REQUEST §R3 |
| 7 | Full E2E & Boot Test Suite Certification | Pass all automated E2E tests (`run_e2e_tests.mjs`, `test_v86_guest_boot.mjs`) | M3 | ORIGINAL_REQUEST §Acceptance |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Direct Kernel Boot & Serial Milestone Engine | Configure `SystemBootstrap` and `V86GuestManager` direct boot, align cmdline, purge legacy ISO references, ensure reactive serial milestone parsing | none | DONE |
| M2 | BinderFS & Native AOSP Services Standup | Verify/implement `wms_rs` standalone binary entry, check `/dev/binderfs` mount, handle 0 servicemanager, and native service initialization | M1 | DONE |
| M3 | Full E2E Test Suite Pass & Adversarial Hardening | Run all E2E test suites (`run_e2e_tests.mjs`, `test_v86_guest_boot.mjs`, `cargo test --workspace`), challenge with adversarial tests, and run Forensic Integrity Audit | M1, M2 | DONE |

## Interface Contracts
### `src/v86_guest_manager.js`
- `class V86GuestManager`:
  - `constructor(config: V86Config)`
  - `start(): Promise<void>`
  - `stop(): Promise<void>`
  - `handleSerialLine(line: string): void`
  - `feedSerial(chunk: string): void`
  - `getState(): string`
  - `getMilestones(): string[]`
  - `onStateChange(cb: (state: string) => void): void`
  - `onMilestone(cb: (milestone: string) => void): void`

### `src/system_bootstrap.js`
- `class SystemBootstrap extends EventEmitter`:
  - `constructor(options: BootstrapOptions)`
  - `bootGuest(screenContainer?: HTMLElement): Promise<V86GuestManager>`
  - `sendSerialCommand(cmd: string): void`
  - `getGuestManager(): V86GuestManager`

### `crates/wms_rs/src/bin/wms_rs.rs`
- Standalone executable binary connecting to `/dev/binder`, registering `"window"` service with ServiceManager, and entering IPC looper.

## Code Layout
- `src/v86_guest_manager.js`: v86 hypervisor lifecycle manager, direct bzImage/initrd loading, serial streaming, milestone parser.
- `src/system_bootstrap.js`: Hypervisor and graphics bridge bootstrap orchestrator.
- `src/logger.js`: Logcat ring buffer and structured logger.
- `guest/initrd/init`: Guest OS PID 1 init script (BinderFS mount, device links, daemon launches).
- `guest/kernel/android_x86_defconfig`: Linux kernel 32-bit x86 defconfig.
- `crates/guest_servicemanager/`: ServiceManager handle 0 daemon.
- `crates/pms_rs/`: Package Manager Service.
- `crates/ams_rs/`: Activity Manager Service.
- `crates/wms_rs/`: Window Manager Service.
- `crates/inputflinger_rs/`: Input Manager Service.
- `tests/test_v86_guest_boot.mjs`: Direct boot & milestone unit and integration tests.
- `tests/run_e2e_tests.mjs`: Multi-tier E2E test suite.
