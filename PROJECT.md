# Project: Android WebGPU Frontend Architecture Refactor

## Architecture
- **DOM & Styling Layer**: Minimal semantic HTML skeleton (`android.html` 146 lines) with external CSS token architecture (`css/tokens.css`, `css/android.css`), zero inline script blocks, and zero inline styles.
- **Entry & Lifecycle Coordination Layer**: `src/main_android.js` binding DOM elements and user interactions to `SystemBootstrap` and `AppController`.
- **System Bootstrap & Hypervisor Layer**: `src/system_bootstrap.js` as the single source of truth for V86 guest lifecycle, asset loading/verification, serial listener attachment, dmesg milestone dispatch, and injectable `V86Class` mock support.
- **Application & IPC Controller Layer**: `src/app_controller.js` handling Activity lifecycle, backstack navigation, package manager (PMS), activity manager (AMS), window manager (WMS), and Binder IPC calls.
- **Pure State UI Rendering Layer**: `src/ui_render.js` providing stateless rendering functions for launcher app grid, dock, status bar, logcat HUD, metrics, and DOM updates.
- **Truthful Display & Compositor Layer**: WebGPU canvas (`#screen`) as sole display target, eliminating synthetic `#screen-app` DOM overlays and gating `arcade_demo.js` behind `?demo=1`.
- **Logging & Telemetry**: Structured log emitter (`src/logger.js`, `src/virtio_gpu_device.js`) with strict separation of high-frequency events (`[D]`) and discrete state transitions (`[I]`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | CSS Token & Style Extraction | Extract all inline CSS into `css/tokens.css` and `css/android.css`; remove all inline `style="..."` attributes | M1 | Survey 1, R1 |
| 2 | Minimal HTML Skeleton | Refactor `android.html` to <= 150 lines with zero inline `<script>` blocks and zero inline styles | M1 | Survey 1, R1 |
| 3 | Modular ES Boundaries | Implement `src/main_android.js`, `src/system_bootstrap.js`, `src/app_controller.js`, `src/ui_render.js` | M1 | Survey 2, R1 |
| 4 | Single Hypervisor Instantiation | Ensure only `src/system_bootstrap.js` (or `src/v86_guest_manager.js`) instantiates `V86Class` / `V86Starter` | M1 | Survey 2, R1 |
| 5 | Synthetic DOM Overlay Removal | Eliminate `#screen-app` container and remove `renderActivityUi` synthetic DOM overlays from primary render path | M2 | Survey 1/2, R2 |
| 6 | Arcade Demo Gating | Gate `arcade_demo.js` behind explicit `?demo=1` query parameter | M2 | Survey 1, R2 |
| 7 | Logging Level Demotion | Demote scanout damaged rect logs in `src/virtio_gpu_device.js` to `[D]`; eliminate idle `[I]` log spam | M3 | Survey 3, R3 |
| 8 | Automated Bootstrap Integration Test | Implement `tests/test_bootstrap.mjs` with injectable `V86Class` mock verifying serial listener, milestones, and single instance | M4 | Survey 3, R4 |
| 9 | Package & Test Suite Harmonization | Add root `package.json` with `pnpm test` script executing full test suite without regression | M4 | Survey 3, R4 |
| 10 | Multi-Axis Verification & Audit | Comprehensive review, adversarial stress testing, and Forensic Integrity Audit | M5 | R1-R4, Audit |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | HTML Skeleton & ES Modularization | `css/tokens.css`, `css/android.css`, `android.html`, `src/main_android.js`, `src/system_bootstrap.js`, `src/app_controller.js`, `src/ui_render.js`, single `V86Class` instantiation | None | DONE |
| M2 | Truthful Canvas & Fake UI Removal | Remove `#screen-app`, remove `renderActivityUi` from primary path, gate `arcade_demo.js` behind `?demo=1` | M1 | DONE |
| M3 | Logging Discipline & Hot-path Fix | Demote scanout damaged rect log to `[D]`, verify zero `[I]` spam per idle frame | M1 | DONE |
| M4 | Automated Integration Tests & Regression Suite | `tests/test_bootstrap.mjs` with mock `V86Class`, `package.json`, `pnpm test` pass | M1, M2, M3 | DONE |
| M5 | Multi-Axis Verification & Forensic Audit | Reviewers × 2, Challengers × 2, Forensic Auditor × 1, Gate verification | M1, M2, M3, M4 | DONE |

## Interface Contracts
### `main_android.js` ↔ `system_bootstrap.js`
- Bootstrap initialization: `const bootstrap = new SystemBootstrap(options)`
- Boot guest: `await bootstrap.init(domElements)`
- Subscriptions: `bootstrap.on('milestone', cb)`, `bootstrap.on('stateChange', cb)`, `bootstrap.on('serial', cb)`

### `main_android.js` ↔ `app_controller.js`
- Controller instantiation: `const appController = new AppController({ bootstrap, runtime, pms, ams, wms })`
- Activity launch: `await appController.launchActivity(packageName, activityName)`
- Back navigation: `appController.handleBackPress()`

### `main_android.js` / `app_controller.js` ↔ `ui_render.js`
- Pure render methods: `renderAppLauncherItem(container, pkg, onLaunch)`, `renderDockItems(container, dockItems, onLaunch)`, `renderLogcatList(container, logs, filter)`, `updateMetrics(container, metrics)`

### `system_bootstrap.js` ↔ `V86Class` Mock (for Testing)
- Injectable constructor: `new SystemBootstrap({ V86Class: MockV86Starter, ... })`
- Listener contract: `emulator.add_listener('serial0-output-char', fn)`
- Event emission: `emulator.emitSerialString(text)`

## Code Layout
- `css/`: Styling stylesheets (`tokens.css`, `android.css`)
- `src/`: JavaScript ES modules (`main_android.js`, `system_bootstrap.js`, `app_controller.js`, `ui_render.js`, `v86_guest_manager.js`, `virtio_gpu_device.js`, `android_runtime.js`, `logger.js`)
- `tests/`: Automated test suites (`test_bootstrap.mjs`, `run_e2e_tests.mjs`, `test_v86_guest_boot.mjs`, `test_logger_m2.mjs`, `test_v86_virtqueue_integration.mjs`, `test_m1_m5_adversarial_challenger.mjs`, `adversarial_virtio_logging_stress.mjs`)
- `android.html`: Minimal DOM skeleton (146 lines)
- `index.html`: Developer testbed with gated arcade demo (`?demo=1`)
- `package.json`: Project manifest with pnpm test scripts
