# E2E Test Infra: Android WebGPU Rendering Pipeline

## Test Philosophy
- Opaque-box, requirement-driven. Direct end-to-end verification of authentic pixel flow and permanent host fallback lockout.
- Methodology: Category-Partition + Boundary Value Analysis (BVA) + Pairwise Combinatorial + Real-World Workload Testing.

## Feature Inventory & Test Mapping
| # | Feature | Source (Requirement) | Tier 1 (Coverage) | Tier 2 (Boundary) | Tier 3 (Pairwise) | Tier 4 (Real-World) |
|---|---------|----------------------|:-----------------:|:-----------------:|:-----------------:|:-------------------:|
| 1 | APK Ingestion & PMS | R1.1 | 5 | 5 | ✓ | ✓ |
| 2 | View Tree & Layout | R1.2 | 5 | 5 | ✓ | ✓ |
| 3 | HWUI & GraphicBuffers | R1.3 | 5 | 5 | ✓ | ✓ |
| 4 | SurfaceFlinger & DRM | R1.4 | 5 | 5 | ✓ | ✓ |
| 5 | VirtIO-GPU Virtqueues | R1.5 | 5 | 5 | ✓ | ✓ |
| 6 | Rust WASM Bridge | R1.6 | 5 | 5 | ✓ | ✓ |
| 7 | WebGPU Compositor & Canvas | R1.7, R1.8 | 5 | 5 | ✓ | ✓ |
| 8 | Host Fallback Lockout | R2 | 5 | 5 | ✓ | ✓ |

## Test Architecture
- **E2E Test Runner**: `tests/run_e2e_tests.mjs`
  - Invocation: `node tests/run_e2e_tests.mjs` or `pnpm test`
  - Pass/Fail semantics: All 82 test assertions must pass with exit code 0.
- **Headless Browser Validation**: `validate_browser.mjs`
  - Invocation: `node validate_browser.mjs` or `pnpm run test:browser`
  - Pass/Fail semantics: Live Puppeteer execution in Chrome; Shannon entropy $H \ge 2.0$, unique colors $\ge 50$, background dominance $\le 85\%$, 0 errors, exit code 0.
- **Rust Workspace Verification**: `cargo test --workspace`
  - Pass/Fail semantics: All 31 workspace crates compile and pass unit/integration tests with exit code 0.
- **Python Entropy Oracle**: `uv run python3 tests/verify_screenshot.py`
  - Pass/Fail semantics: Paeth/Sub/Up/Average unfiltering on `screenshot.png` verifying dimensions (720x1440), $H \ge 2.0$, and active spatial slices.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity | Target APK |
|---|----------|--------------------|------------|------------|
| 1 | F-Droid App Catalog Launch | APK Ingestion, View Tree, HWUI, SurfaceFlinger, VirtIO-GPU, Rust Bridge, WebGPU, Canvas | High | `F-Droid.apk` |
| 2 | Firefox Browser Home Launch | Complex multi-DEX Dalvik loading, GeckoView layout, Canvas presentation | High | `firefox.apk` |
| 3 | Material You Dynamic Theming | Theme attribute resolution, Canvas damage rect updates, Color swizzling | Medium | Multi-layer |
| 4 | Rapid Activity Re-layout & Scroll | BufferQueue cycling, dirty-rect blitting, VirtIO-GPU transfer burst | High | Recycler/List |
| 5 | Lockout Stress Under Host Contention | Concurrent guest flush + host injection attempt, ensuring zero synthetic leakage | High | Full pipeline |

## Coverage Thresholds
- Tier 1: 35 tests (5 per core feature area)
- Tier 2: 35 tests (5 boundary/corner tests per feature area)
- Tier 3: 7 pairwise integration tests
- Tier 4: 5 real-world application scenarios
- Total E2E Tests: 82 tests passing with 100% success rate.
