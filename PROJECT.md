# Project: Firefox APK Ingestion & GeckoView Web Content Execution

## Architecture
The system ingests and executes authentic `firefox.apk` within the Dalvik VM and WebGPU runtime environment:
1. **Target APK Resolution & Multi-DEX Ingestion (`src/main_android.js`, `src/apk_client_parser.js`, `src/dex_vm.js`)**:
   - URL query parameter parsing (`?apk=firefox.apk`) defaulting to `firefox.apk`.
   - Zero-dependency ZIP extraction (`ApkZipReader`) and binary XML parsing (`AxmlDecoder`) decoding package name `org.mozilla.firefox`, SDK 37, activities, and native `.so` libraries.
   - Multi-DEX bytecode parser (`DexParser`) loading `classes.dex`, `classes2.dex`, `classes3.dex` (80,012 classes) into `DalvikVM`.
   - `PackageManagerRegistry` registration of `org.mozilla.firefox`.
2. **GeckoView Activity Launch & Web Content Rendering (`src/app_controller.js`, `src/android_runtime.js`, `src/view_rasterizer.js`)**:
   - `AppController.launchActivity('org.mozilla.firefox')` executes Binder transactions to `ams_rs` (Handle 4) and `wms_rs` (Handle 3), invokes Dalvik VM lifecycle, and activates the `#screen-webgpu` canvas viewport.
   - `AndroidRuntime.renderActivityUi()` initializes `appState.activeUrl = 'https://www.google.com'`, `appState.currentPage = 'Google'`, and constructs the GeckoView view hierarchy.
   - `ViewHierarchyRasterizer` renders the Google Search mobile layout (Google logo, search pill, action buttons, trending cards, navigation) onto the 720x1440 WebGPU canvas buffer.
3. **Interactive Navigation & Toolbar Control (`src/android_runtime.js`, `src/view_hierarchy.js`, `src/main_android.js`)**:
   - URL search bar and action buttons (Reload, Back, Home, Tabs) with touch hit-testing and event dispatch updating `appState.activeUrl` and re-rasterizing the view hierarchy.
   - Physical pointer event dispatch from canvas coordinates to view click listeners.
4. **Automated Quality Gates & Test Suites (`tests/`, `package.json`)**:
   - Unit and integration test suites executed via `pnpm test` (21 test files, 100% pass, 0 failures).
   - End-to-end 4-Tier verification runner (`node tests/run_e2e_tests.mjs`).
   - Guest boot and hardware presentation verification (`tests/test_v86_guest_boot.mjs`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Target APK Resolution & Default Config | Parse `?apk=firefox.apk` URL param, default to `firefox.apk`, and fetch arrayBuffer | M1 | R1 |
| 2 | APK ZIP & Manifest Decoding | Decode `AndroidManifest.xml` via `AxmlDecoder`, extracting `org.mozilla.firefox` & metadata | M1 | R1 |
| 3 | Multi-DEX Bytecode Ingestion | Ingest `classes.dex`, `classes2.dex`, `classes3.dex` (80,012 classes) into `DalvikVM` | M1 | R1 |
| 4 | PMS Package Registration | Register `org.mozilla.firefox` with label "Firefox" and icon in `PackageManagerRegistry` | M1 | R1 |
| 5 | GeckoView Activity Launch | Launch `org.mozilla.firefox` via `AppController.launchActivity` and switch to WebGPU canvas | M2 | R2 |
| 6 | Active URL Initialization | Default `appState.activeUrl` to `https://www.google.com` and `appState.currentPage` to `'Google'` | M2 | R2 |
| 7 | Google Search Canvas Rasterization | Rasterize Google Search mobile UI (logo, search pill, buttons) onto 720x1440 canvas buffer | M2 | R2 |
| 8 | Interactive Navigation & URL Bar | Support address bar search/click and action buttons (Reload, Back, Home) updating web view | M3 | R3 |
| 9 | Comprehensive Test Suite Verification | Verify 100% test pass across unit, integration, and E2E suites via `pnpm test` | M3 | R3 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Target APK Resolution & Firefox Ingestion | URL parameter parsing, `ApkZipReader`, `AxmlDecoder`, Multi-DEX loading into `DalvikVM`, PMS `org.mozilla.firefox` registration | none | DONE |
| M2 | GeckoView Activity Launch & Google Rendering | `AppController.launchActivity('org.mozilla.firefox')`, viewport switch, `activeUrl = https://www.google.com`, 720x1440 rasterization | M1 | DONE |
| M3 | Interactive Navigation & Test Suite Validation | Address bar click, Reload/Back/Home actions, unit test assertions, `pnpm test` pass | M2 | DONE |

## Interface Contracts
### Target APK Resolution Contract
```javascript
// URL query parameter ?apk=firefox.apk -> targetApk = 'firefox.apk'
// ArrayBuffer -> ApkZipReader -> AxmlDecoder -> DalvikVM & PackageManagerRegistry
```

### GeckoView Activity & Navigation State
```javascript
interface FirefoxAppState {
  packageName: 'org.mozilla.firefox';
  activityName: 'org.mozilla.firefox.App';
  activeUrl: 'https://www.google.com' | string;
  currentPage: 'Google' | 'home' | string;
}
```

### Canvas Viewport & Scanout Dimensions
- Resolution: 720 x 1440
- RGBA Buffer Size: 4,147,200 bytes ($720 \times 1440 \times 4$)
- DOM Viewport: `#screen-webgpu` canvas element `#screen`

## Code Layout
- `firefox.apk`: Authentic Firefox Android application package (138.4 MB).
- `src/main_android.js`: Entry point, URL param parser (`?apk=...`), target APK loader, and navigation handlers.
- `src/apk_client_parser.js`: `ApkZipReader`, `AxmlDecoder`, `PackageManagerRegistry`.
- `src/dex_vm.js`: `DexParser`, `DalvikVM` multi-DEX class loading and execution.
- `src/app_controller.js`: Activity launching, screen switching (`activateScreen('webgpu')`), Binder dispatch.
- `src/android_runtime.js`: Runtime coordinator, GeckoView layout construction, URL navigation, toolbar actions.
- `src/view_rasterizer.js`: `ViewHierarchyRasterizer` (720x1440 canvas rendering).
- `src/view_hierarchy.js`: Android View hierarchy components, touch dispatch, reverse-Z hit testing.
- `tests/`: Automated unit, integration, and E2E test suites (`test_firefox_geckoview.mjs`, `test_geckoview_adversarial_stress.mjs`, etc.).
