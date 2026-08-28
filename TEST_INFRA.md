# E2E Test Infra: androidwebgpu

## Test Philosophy
- Opaque-box, requirement-driven validation based directly on `ORIGINAL_REQUEST.md`.
- No reliance on mock synthetic shortcuts when verifying in-guest execution.
- Systematic 4-tier testing: Category-Partition (Tier 1), Boundary Value Analysis (Tier 2), Pairwise Interaction (Tier 3), Real-World Application Scenarios (Tier 4).

## Feature Inventory & Test Mapping
| # | Feature | Requirement | Tier 1 (Count) | Tier 2 (Count) | Tier 3 (Pairwise) | Tier 4 (Scenario) |
|---|---------|-------------|:--------------:|:--------------:|:-----------------:|:-----------------:|
| 1 | BinderFS & Virtual FS Init | R1 | 5 | 5 | ✓ | ✓ |
| 2 | ServiceManager Handle 0 | R1 | 5 | 5 | ✓ | ✓ |
| 3 | SurfaceFlinger DRM Linkage | R1 | 5 | 5 | ✓ | ✓ |
| 4 | Native System Daemons | R1 | 5 | 5 | ✓ | ✓ |
| 5 | Zygote Daemon & Boot Assets | R1 | 5 | 5 | ✓ | ✓ |
| 6 | F-Droid APK Deployment | R2 | 5 | 5 | ✓ | ✓ |
| 7 | Zygote Fork IPC | R2 | 5 | 5 | ✓ | ✓ |
| 8 | In-Guest ART / Dalvik VM Execution | R2 | 5 | 5 | ✓ | ✓ |
| 9 | ActivityThread Lifecycle | R2 | 5 | 5 | ✓ | ✓ |
| 10 | HWUI / Skia Rendering Pipeline | R3 | 5 | 5 | ✓ | ✓ |
| 11 | SurfaceFlinger DRM Composition | R3 | 5 | 5 | ✓ | ✓ |
| 12 | Host VirtioGpuDevice Virtqueue Bridge | R3 | 5 | 5 | ✓ | ✓ |
| 13 | Synthetic Injection Gating | R3 | 5 | 5 | ✓ | ✓ |
| 14 | WebGPU Canvas Presentation | R3 | 5 | 5 | ✓ | ✓ |
| 15 | Logcat & Verification Harnesses | R4 | 5 | 5 | ✓ | ✓ |

## Test Architecture
- **Rust Test Harness**: `cargo test --workspace` exercises 31 workspace member crates including Binder IPC, Zygote client, VirtIO GPU parser, and AMS lifecycle.
- **Node.js In-Guest Rendering Harness**: `node tests/test_real_guest_rendering.mjs` feeds VirtIO GPU virtqueue commands directly to `VirtioGpuBridge`, verifies `guestActive = true`, and measures scanout Shannon entropy $H \ge 1.0$.
- **Browser E2E Harness**: `node validate_browser.mjs` executes headless Chromium with WebGPU enabled, ingests F-Droid APK, boots userspace, verifies logcat lifecycle transitions, and captures `screenshot.png` with $H \ge 1.0$.
- **E2E Suite Runner**: `node tests/run_e2e_tests.mjs` runs full suite across all tiers.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Full Guest Boot & ServiceManager Discovery | F1, F2, F3, F4, F5 | High |
| 2 | F-Droid Package Staging & Zygote Forking | F6, F7, F8, F9 | High |
| 3 | ActivityThread Lifecycle (onCreate -> onStart -> onResume) | F7, F8, F9, F15 | High |
| 4 | HWUI / Skia Window Composition via VirtIO DRM | F10, F11, F12, F13 | High |
| 5 | End-to-End Canvas Presentation with Shannon Entropy Verification | F12, F13, F14, F15 | High |

## Coverage Thresholds
- Tier 1 (Feature Coverage): ≥ 75 test cases (5 per feature across 15 features)
- Tier 2 (Boundary & Corner): ≥ 75 test cases (buffer bounds, invalid PIDs, socket disconnects, unaligned DRM formats)
- Tier 3 (Cross-Feature Combinations): ≥ 15 pairwise interaction tests
- Tier 4 (Real-World Application Scenarios): ≥ 5 end-to-end workload test cases
- Total Test Cases: ≥ 170 tests across workspace and E2E runners
