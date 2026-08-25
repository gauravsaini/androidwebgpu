# Project: Real-World F-Droid Ingestion & Execution on AndroidWebGPU

## Architecture

AndroidWebGPU executes real-world Android applications directly against native Rust system services without requiring a Java `system_server` runtime. The architecture spans four integrated layers:

```
┌────────────────────────────────────────────────────────────────────────┐
│               Real-World Android APK (`F-Droid.apk`)                   │
│   org.fdroid.fdroid.views.main.MainActivity (singleTop Launcher)       │
│   ApkFileProvider / FileProvider | DownloaderService / SwapService     │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │ Binder RPC / Unix Socketpair
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   Native System Services (Guest/Host)                  │
│  ┌──────────────────┐  ┌───────────────────┐  ┌─────────────────────┐  │
│  │   PMS (pms_rs)   │  │   AMS (ams_rs)    │  │    WMS (wms_rs)     │  │
│  │  - AXML/ARSC     │  │  - Lifecycle      │  │  - WindowSession    │  │
│  │  - Provider Reg  │  │  - AppThread bind │  │  - SurfaceBridge    │  │
│  │  - Queries (AIDL)│  │  - Provider / Svc │  │  - Insets / Layout  │  │
│  └─────────┬────────┘  └─────────┬─────────┘  └──────────┬──────────┘  │
│            │                     │                       │             │
│            │                     ▼                       │             │
│            │           ┌───────────────────┐             │             │
│            │           │   Zygote Client   │             │             │
│            │           │  - Process Tracker│             │             │
│            │           │  - PID Forking    │             │             │
│            │           └───────────────────┘             │             │
└────────────┼─────────────────────────────────────────────┼─────────────┘
             │                                             │
             ▼                                             ▼
┌────────────────────────────────────────┐   ┌───────────────────────────┐
│        InputFlinger & Channels         │   │   SurfaceFlinger WebGPU   │
│  - InputManagerService (AIDL)          │   │  - SurfaceComposerService │
│  - Socketpair InputChannel             │   │  - BufferQueue & Textures │
│  - Publisher/Consumer Touch & Key Flow │   │  - WebGpuCompositor Read  │
└────────────────────────────────────────┘   └───────────────────────────┘
```

---

## Feature Inventory

| # | Feature | Description | Milestone | Source |
|---|---|---|---|---|
| 1 | F-Droid Binary APK Ingestion | Ingest `/Users/ektasaini/Desktop/androidwebgpu/F-Droid.apk` (12.4 MB), parse `AndroidManifest.xml` & `resources.arsc` | M1 | Survey (Explorer 1) |
| 2 | AXML `<provider>` & Permission Parser | Parse `<provider>`, `<uses-permission-sdk-23>`, and reference string formatting | M1 | Survey (Explorer 1) |
| 3 | Provider & Service Indexing | Index `ProviderInfo` by semicolon-separated authorities and `ComponentName` | M1 | Survey (Explorer 1) |
| 4 | Extended `IPackageManager` AIDL | Implement `resolveContentProvider`, `getPackageInfo`, `getApplicationInfo`, `queryIntentActivities` | M1 | Survey (Explorer 1) |
| 5 | Zygote Process Forking | Fork process for package `org.fdroid.fdroid`, track PID in `ProcessTracker` | M2 | Survey (Explorer 2) |
| 6 | AMS Lifecycle State Transitions | `start_activity` -> `attach_application` -> `bind_application` -> `onCreate` -> `onStart` -> `onResume` | M2 | Survey (Explorer 2) |
| 7 | ContentProvider IPC & Cursor Transport | `acquireProvider` / `get_content_provider` returning `IContentProvider` and `CursorData` | M2 | Survey (Explorer 2) |
| 8 | Service Connection Plumbing | Basic `start_service` and `bind_service` tracking with `IServiceConnection` | M2 | Survey (Explorer 2) |
| 9 | Fullscreen SurfaceControl Allocation | WMS `open_session` -> `add_to_display` -> `relayout` returning `SurfaceControl` for `MainActivity` | M3 | Survey (Explorer 3) |
| 10 | WebGPU SurfaceFlinger Composition | Present catalog layers through `GraphicBufferProducer` and `WebGpuCompositor` | M3 | Survey (Explorer 3) |
| 11 | InputChannel Touch Event Dispatch | Dispatch touch down, up, move, and scroll messages over socketpair to `ViewRootImpl` | M3 | Survey (Explorer 3) |
| 12 | Deterministic E2E Verification | End-to-end multi-service tests in `crates/tests_e2e_system_services` for F-Droid | M4 | Survey (Explorer 3) |
| 13 | GATES.md Update & Verification | Update `GATES.md` with runnable check commands and expected outputs | M4 | User Request R4 |
| 14 | Workspace Test Pass 100% | Clean run of `cargo test --workspace` across all crates with 0 failures | M4 | User Request R4 |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|---|---|---|---|
| M1 | PMS F-Droid Ingestion & Extended Queries | Parse `F-Droid.apk`, `AndroidManifest.xml` (providers, services, permissions), `resources.arsc` labels, implement `resolveContentProvider` & query APIs | none | DONE |
| M2 | AMS Lifecycle, ContentProvider & Service Plumbing | Process fork for `org.fdroid.fdroid`, `ApplicationThread` binding, lifecycle transitions to `RESUMED`, `acquireProvider` cursor transport, service tracking | M1 | IN_PROGRESS |
| M3 | WMS SurfaceControl & Touch Dispatch | Fullscreen window allocation, WebGPU SurfaceFlinger layer transactions, `InputChannel` socketpair touch injection | M2 | PLANNED |
| M4 | E2E Integration Suite, GATES.md & 100% Test Pass | Deterministic E2E integration tests in `crates/tests_e2e_system_services`, update `GATES.md`, verify `cargo test --workspace` | M1, M2, M3 | PLANNED |

---

## Interface Contracts

### 1. PMS (`pms_rs`) ↔ AMS (`ams_rs`)
- `IPackageManager::resolve_intent(intent, resolved_type, flags, user_id) -> AidlResult<Option<ResolveInfo>>`
- `IPackageManager::resolve_content_provider(authority, flags, user_id) -> AidlResult<Option<ProviderInfo>>`
- `IPackageManager::get_package_info(package_name, flags, user_id) -> AidlResult<Option<PackageInfo>>`
- `IPackageManager::get_application_info(package_name, flags, user_id) -> AidlResult<Option<ApplicationInfo>>`

### 2. AMS (`ams_rs`) ↔ App Process / `IApplicationThread`
- `IActivityManager::start_activity(caller, caller_pkg, intent, resolved_type, result_to, result_who, req_code, flags, options) -> AidlResult<i32>`
- `IActivityManager::attach_application(thread_binder, start_seq) -> AidlResult<()>`
- `IActivityManager::activity_resumed(token) -> AidlResult<()>`
- `IActivityManager::get_content_provider(caller, calling_package, name, user_id, stable) -> AidlResult<Option<SpIBinder>>`
- `IApplicationThread::bind_application(pkg_name, app_info, process_name) -> AidlResult<()>`
- `IApplicationThread::schedule_resume_activity(token, is_forward) -> AidlResult<()>`

### 3. WMS (`wms_rs`) ↔ SurfaceFlinger (`surfaceflinger_gpu_service`) ↔ Input (`inputflinger_rs`)
- `IWindowManager::open_session(callback) -> AidlResult<SpIBinder>` (`IWindowSession`)
- `IWindowSession::add_to_display(window, token, display_id, insets_state, input_channel) -> AidlResult<i32>`
- `IWindowSession::relayout(window, width, height, surface_control) -> AidlResult<i32>`
- `IWindowSession::finish_drawing(window, post_draw_transaction) -> AidlResult<()>`
- `IInputManager::inject_input_event(event, mode) -> AidlResult<bool>`
- `InputMessage` wire format (1024 bytes binary payload): `Key`, `Motion`, `Finished`.

---

## Code Layout

- `crates/pms_rs`:
  - `src/types.rs`: `ProviderInfo`, `PackageInfo`, `ServiceInfo`, `ReceiverInfo`, `Parcelable` implementations
  - `src/axml.rs`: Binary XML chunk decoder with `<provider>` and `<uses-permission-sdk-23>` parsing
  - `src/arsc.rs`: String resource table parser and `@res_id` resolver
  - `src/package_manager.rs`: In-memory package and provider authority registry
  - `src/service.rs`: `IPackageManager` AIDL trait and opcode dispatcher
- `crates/ams_rs`:
  - `src/activity_manager.rs`: `ActivityManagerService`, `IActivityManager`
  - `src/app_thread.rs`: `IApplicationThread`, `MockApplicationThread`
  - `src/provider.rs`: `IContentProvider`, `CursorData` tabular data transport
  - `src/lifecycle.rs`: `LifecycleManager`, `ActivityStack`
- `crates/wms_rs`:
  - `src/window_manager.rs`: `WindowManagerService`, `IWindowManager`
  - `src/window_session.rs`: `WindowSession`, `IWindowSession`
  - `src/surface_bridge.rs`: Bridge to `SurfaceComposerService`
- `crates/inputflinger_rs` & `crates/input_channel`:
  - `crates/input_channel/src/channel.rs`: `InputChannel` socketpair creation
  - `crates/input_channel/src/message.rs`: `InputMessage` wire serialization
  - `crates/inputflinger_rs/src/dispatcher.rs`: `InputDispatcher`
- `crates/tests_e2e_system_services`:
  - `tests/test_fdroid_e2e_ingestion_and_lifecycle.rs`: Comprehensive deterministic integration suite
- `GATES.md`: Verification gates and validation commands
