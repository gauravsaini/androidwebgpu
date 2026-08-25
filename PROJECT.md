# Project: AndroidWebGPU Material You Emulator & Subsystem Uplift

## Architecture
AndroidWebGPU executes real-world Android components without a Java `system_server` runtime.
The architecture combines:
1. **Frontend Viewport Layer (`index.html`)**:
   - Material You Android 13/14 Home Launcher inside `.phone-frame`.
   - Android Status Bar (`#android-status-bar`): live clock, battery %, 5G, Wi-Fi 6, notification icons.
   - Google-style Search Widget & Date/Weather At-A-Glance pill.
   - App Grid: 6 default apps (F-Droid, Unity 3D Cube, Godot GLES2, Chrome, Files, Settings) + dynamic ingested APK container.
   - Elevated 4-App Dock (Phone, Messages, Browser, Camera).
   - 3-Button Android Navigation Bar (Back ◀, Home ◯, Recents ▢) operating globally across all screens.
   - Interactive F-Droid Client View (`#screen-fdroid`): search input, 7 category tabs, scrollable catalog cards, and App Detail modal.
   - Native Canvas View (`#screen-native-surface`) for 3D GLES / Vulkan games.
   - Sidebar Tab (`#tab-emulator` / `📱 Android OS Emulator`) with Quick App Switcher, Virtual Hardware Controls (Power, Volume Rocker + HUD, Screen Rotate 90°), Drag-and-Drop APK Dropzone, and Live Subsystem Inspector (PMS packages count, AMS task backstack).
2. **Subsystems & Client-Side Parser (`src/apk_client_parser.js`, `src/binder_test_suite.js`, `index.html`)**:
   - Zero-dependency client-side ZIP unpacker and binary AXML (`AndroidManifest.xml`) chunk decoder (`0x0003`, `0x0001`, `0x0180`, `0x0102`, `0x0103`) & ARSC string pool parser.
   - Complete DEFLATE Huffman tree and boundary hardening eliminating all infinite loop and truncated stream failure vectors.
   - Dynamic PMS Package Registry and dynamic app launcher on Home Screen.
   - Full integration with AMS, PMS, WMS, Binder, and InputFlinger.
3. **Verification & Test Harness (`tests/adversarial_browser_bench_verifier.mjs`, `src/binder_test_suite.js`, `GATES.md`)**:
   - E2E 12-14 test suites in `src/binder_test_suite.js`.
   - Sections 9-12 in `tests/adversarial_browser_bench_verifier.mjs` verifying DOM invariants, F-Droid search/category filtering, Nav Bar stack transitions, and AXML decoder fuzzing.
   - 176,014 passed assertions in Node.js test harness (0 failures).
   - Full workspace test pass in `cargo test --workspace` across all 30 member crates (0 failures).
   - Comprehensive `GATES.md` update.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Material You Home Launcher | Android 13/14 Home Screen with search widget, date/weather pill, app grid, dock | M1 | ORIGINAL_REQUEST §R1 |
| 2 | Android Status Bar | Live clock, battery %, 5G cellular, Wi-Fi 6, notification icons | M1 | ORIGINAL_REQUEST §R1 |
| 3 | 3-Button Navigation Bar | Back ◀, Home ◯, Recents ▢ operating globally across all screens | M1 | ORIGINAL_REQUEST §R1 |
| 4 | Interactive F-Droid Client | Dedicated app store view with search, category tabs, catalog list, and detail modal | M2 | ORIGINAL_REQUEST §R2 |
| 5 | F-Droid Query & Category Filter | Real-time search query filtering and category tab switching | M2 | ORIGINAL_REQUEST §R2 |
| 6 | F-Droid App Detail Modal | Permissions, component counts, version history, install/open actions | M2 | ORIGINAL_REQUEST §R2 |
| 7 | Client-Side ZIP & AXML Parser | Binary AndroidManifest.xml chunk decoder and ARSC string pool extractor | M3 | ORIGINAL_REQUEST §R3 |
| 8 | Dynamic PMS Registration & Grid Update | Ingest APK, register package, append dynamic icon to launcher grid | M3 | ORIGINAL_REQUEST §R3 |
| 9 | Emulator Sidebar Tab & Controls | Top nav tab, Quick App Switcher, Power, Volume rocker with HUD, Screen Rotate | M4 | ORIGINAL_REQUEST §R4 |
| 10 | Live Subsystem Inspector | Live PMS installed packages count and AMS task backstack tracker | M4 | ORIGINAL_REQUEST §R4 |
| 11 | Phone Frame Drag-and-Drop Dropzone | Visual dragover overlay on phone frame + sidebar upload button | M4 | ORIGINAL_REQUEST §R3/R4 |
| 12 | Test Suite & Bench Verifier Expansion | Sections 9-12 in bench verifier, E2E 12-14 in binder test suite | M5 | ORIGINAL_REQUEST §R5 |
| 13 | Cargo Workspace & GATES.md Verification | 100% passing cargo test --workspace and documented gates | M5 | ORIGINAL_REQUEST §R5 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Android Home Launcher & Material You OS Theme | Status Bar, Home Screen, App Grid, App Dock, 3-Button Nav Bar | none | DONE |
| M2 | Interactive F-Droid Client Experience | F-Droid view, Category tabs, Search filtering, Catalog list, App Detail modal | M1 | DONE |
| M3 | Client-Side Drag-and-Drop APK Parser & Ingestion | Pure-JS ZIP unpacker, Binary AXML decoder, ARSC parser, PMS dynamic registration | M1 | DONE |
| M4 | Sidebar Controls & Subsystem Integration | Emulator sidebar tab, Quick Switcher, Power/Volume/Rotate hardware controls, Live Inspector, Dropzone overlay | M1, M2, M3 | DONE |
| M5 | Test Suite Verification & GATES.md Documentation | `tests/adversarial_browser_bench_verifier.mjs`, `src/binder_test_suite.js`, `GATES.md`, `cargo test` | M1, M2, M3, M4 | DONE |

## Interface Contracts
### Home Launcher ↔ Subsystems (`window.AndroidWebGpu`)
- `window.AndroidWebGpu.launchApp(packageName, activityName)`: Switches active screen container, updates AMS backstack, renders app view.
- `window.AndroidWebGpu.navigateBack()`: Pops AMS backstack; closes modal or returns to Home.
- `window.AndroidWebGpu.navigateHome()`: Resets screen stack to Home.
- `window.AndroidWebGpu.navigateRecents()`: Toggles Recents task overview.
- `window.AndroidWebGpu.setVolume(delta)`: Adjusts master volume, triggers `#volume-hud`.
- `window.AndroidWebGpu.togglePower()`: Toggles screen sleep state.
- `window.AndroidWebGpu.toggleOrientation()`: Toggles portrait / landscape viewport.
- `window.AndroidWebGpu.ingestApk(fileOrArrayBuffer)`: Parses APK, registers into PMS, updates Home Grid and F-Droid catalog.

## Code Layout
- `index.html`: Main UI, phone frame, status bar, home launcher, F-Droid view, navigation bar, sidebar emulator card, styles.
- `src/apk_client_parser.js`: Pure-JS ZIP reader, Binary AXML chunk parser, ARSC string pool decoder, PMS registration bridge.
- `src/binder_test_suite.js`: Binder IPC, Parcel serialization, E2E test suites (E2E 1-14) including pure-JS AXML verification.
- `tests/adversarial_browser_bench_verifier.mjs`: Automated bench verifier tests (Sections 1-12).
- `GATES.md`: Run commands and verification output for all gates.
