# Authentic Android View System: E2E Test Infrastructure

## 1. Test Architecture & Philosophy

The AndroidWebGPU Authentic View System E2E test suite validates the complete Android UI runtime pipeline without synthetic HTML DOM overlays or simulated screen mockups.

The test infrastructure enforces:
1. **Opaque-Box Requirement Verification**: Tests derive expected behavior directly from `ORIGINAL_REQUEST.md`, `PROJECT.md`, and official Android framework specifications (`AOSP`).
2. **Zero-Mock Verification**: Tests verify that 100% of application pixels render through in-memory `View` hierarchies, `OffscreenCanvas` / WebGPU texture rasterization, and `SurfaceFlinger` / `VirtioGpuBridge` wire protocols. All synthetic HTML DOM generators (`renderFdroidActivity`, `renderSettingsActivity`, `renderBrowserActivity`, `#screen-app`) are verified as eliminated.
3. **Progressive Testability & Isolation**: Every test case is self-contained, deterministic, and runnable independently in Node.js with zero external dependencies.

```
┌────────────────────────────────────────────────────────┐
│               APK Archive (F-Droid.apk)                │
└───────────────────────────┬────────────────────────────┘
                            │
              ┌─────────────▼─────────────┐
              │  resources.arsc Decoder   │
              │  (Locale / Config Match)  │
              └─────────────┬─────────────┘
                            │
              ┌─────────────▼─────────────┐
              │ Binary XML Layout Parser  │
              │ (LayoutInflater AST Tree) │
              └─────────────┬─────────────┘
                            │
              ┌─────────────▼─────────────┐
              │ In-Memory View Hierarchy  │
              │ (Measure -> Layout Pass)  │
              └─────────────┬─────────────┘
                            │
              ┌─────────────▼─────────────┐
              │ Material Design 3 Painter │
              │ (1280x720 RGBA Rasterizer)│
              └─────────────┬─────────────┘
                            │
              ┌─────────────▼─────────────┐
              │ SurfaceFlinger & VirtIO   │
              │ (Hardware Swapchain Pres) │
              └─────────────┬─────────────┘
                            │
              ┌─────────────▼─────────────┐
              │ WebGPU Multi-Plane Canvas │
              │ (Zero DOM Overlay Invar)  │
              └───────────────────────────┘
```

---

## 2. Test Design Methodologies

### 2.1 Category-Partition Testing (Tier 1)
Partitions the functional space of the 17 core features from `PROJECT.md` into discrete input equivalence classes:
- Resource types: `@string`, `@color`, `@dimen`, `@layout`, `@id`, `@style`.
- Dimension units: `dp`, `sp`, `px`, `pt`, `in`, `mm`, `MATCH_PARENT` (`-1`), `WRAP_CONTENT` (`-2`).
- Layout managers: `FrameLayout`, `LinearLayout` (horizontal/vertical, weight), `RelativeLayout` (anchor rules), `ConstraintLayout` (anchor chains), `ScrollView`, `RecyclerView`.
- Core widgets: `TextView` (typography, line wrapping, gravity), `ImageView` (scale types, vector paths), `Button` (click state, corner radii).
- Hardware pipelines: `VirtioGpuBridge` (`TRANSFER_TO_HOST_2D`, `RESOURCE_FLUSH`), `SurfaceComposerService` (z-order sorting), `InputDispatcher` (`MotionEvent`, `KeyEvent`, reverse-Z hit-testing).

### 2.2 Boundary Value Analysis & Corner Cases (Tier 2)
Tests boundary conditions and extreme corner cases:
- 0-byte and truncated binary XML buffers.
- Deeply nested View trees (depth 10 to 20 levels).
- Extreme layout bounds: 0px, negative margins, large DP values (`100,000dp`).
- Missing and unresolvable resource identifiers with graceful fallback.
- Rapid input event dispatch (5,000 synchronous clicks and touch sequences).
- Scroll offset clamping beyond viewport limits.
- Zero-size damage rectangles and 1x1 minimum canvas rasterization.

### 2.3 Pairwise Cross-Feature Interactions (Tier 3)
Tests combinations across distinct architectural boundaries:
- APK extraction + ARSC string pool + Binary XML layout inflation.
- In-memory View tree + Measure/Layout pass + MD3 OffscreenCanvas rasterizer.
- Rasterized buffer + VirtIO wire packets + WebGPU hardware presentation.
- Canvas pointer input + InputChannel + ViewRootImpl + Reverse-Z hit-testing + Button onClick.
- Hardware `KEYCODE_BACK` + ActivityManagerService (`ams_rs`) backstack popping + View restoration.
- Guest Linux VM scanout (z=0) + APK Activity surface (z=1) + WindowManager surface switching.

### 2.4 Real-World Application Scenarios (Tier 4)
Executes production application workflows using real binary assets from `F-Droid.apk`:
- Complete cold start: APK load -> ARSC parse -> `activity_main.xml` (`res/v9.xml`) inflate -> measure -> layout -> rasterize to WebGPU texture.
- Catalog list rendering: `main_tab_latest.xml` (`res/u8.xml`) with `RecyclerView` binding `app_list_item.xml` (`res/Kt.xml`).
- App details navigation: Item click -> `app_details2.xml` (`res/Md.xml`) inflation & rendering.
- Interactive install flow: Click "Install" button (`0x7f09003c`) -> UI state mutation -> damage re-render.
- Multi-activity navigation and backstack popping with state preservation.
- Guest OS scanout multitasking and seamless foreground switching.

---

## 3. Feature Verification Matrix

| # | Feature Name | Description | Tier 1 | Tier 2 | Tier 3 | Tier 4 |
|---|---|---|:---:|:---:|:---:|:---:|
| 1 | APK Zip & Binary XML Layout Extraction | Extract and decode `res/layout/*.xml` from APKs | 5 | 5 | ✓ | ✓ |
| 2 | ARSC String Pool & Config Matching | Parse `resources.arsc` with `ResTable_config` matching | 5 | 5 | ✓ | ✓ |
| 3 | Resource Identifier Resolution | Resolve `@string`, `@color`, `@dimen`, `@layout`, `@id`, `@style` | 5 | 5 | ✓ | ✓ |
| 4 | TypedValue Unit Decoding | Decode dimension units (dp, sp, px), hex colors, integers | 5 | 5 | ✓ | ✓ |
| 5 | View & ViewGroup Base Hierarchy | Implement `View`, `ViewGroup`, `LayoutParams`, `MeasureSpec` | 5 | 5 | ✓ | ✓ |
| 6 | Layout Container Types | `FrameLayout`, `LinearLayout`, `RelativeLayout`, `ConstraintLayout`, `ScrollView`, `RecyclerView` | 7 | 5 | ✓ | ✓ |
| 7 | Core Widget Types | `TextView`, `ImageView`, `Button` with attributes and states | 5 | 5 | ✓ | ✓ |
| 8 | Complete Synthetic DOM Elimination | Verify 0 synthetic DOM mockups or `render*Activity` functions | 5 | 5 | ✓ | ✓ |
| 9 | MD3 OffscreenCanvas Rasterizer | Render View tree to 1280x720 RGBA buffer with MD3 styles | 5 | 5 | ✓ | ✓ |
| 10 | WebGPU / SurfaceFlinger Submission | `VirtioGpuBridge` wire protocol and BufferQueue submission | 5 | 5 | ✓ | ✓ |
| 11 | Guest VM Scanout Presentation | Render Linux DRM scanouts directly to WebGPU without DOM | 5 | 5 | ✓ | ✓ |
| 12 | WindowManager Surface Stacking | Multi-plane depth sorting (z=0 Guest VM, z=1..100 App, z=1000 SystemUI) | 5 | 5 | ✓ | ✓ |
| 13 | Input Event Injection & Routing | Route canvas pointer/touch/key events through `inputflinger_rs` | 5 | 5 | ✓ | ✓ |
| 14 | In-Memory Hit-Testing & Bubbling | Reverse-Z top-down hit-testing, `onTouchEvent`, `onClick`, `onScroll` | 5 | 5 | ✓ | ✓ |
| 15 | Hardware Navigation & Backstack | Dispatch `KEYCODE_BACK` / `KEYCODE_HOME` to `ams_rs` lifecycle | 5 | 5 | ✓ | ✓ |
| 16 | E2E Test Suite Architecture | Progressive test runner, reporting, and execution invariants | 5 | 5 | ✓ | ✓ |
| 17 | Adversarial Hardening & Robustness | Memory bounds, integer overflow prevention, high-stress input | 5 | 5 | ✓ | ✓ |

---

## 4. Test Execution & Coverage Targets

### 4.1 Commands
```bash
# Run Central E2E View System Test Suite (All 4 Tiers)
node tests/test_e2e_authentic_view_system.mjs

# Run Auxiliary APK & DEX Parser Suites
node tests/test_dex_vm_apk.mjs

# Run All Rust Crates Test Suite
cargo test --workspace
```

### 4.2 Coverage Thresholds
- **Tier 1 (Feature Coverage)**: ≥ 85 test cases (≥ 5 per feature across 17 features)
- **Tier 2 (Boundary & Corner Cases)**: ≥ 50 test cases (≥ 5 per area across 10 areas)
- **Tier 3 (Cross-Feature Combinations)**: ≥ 20 test cases
- **Tier 4 (Real-World Scenarios)**: ≥ 10 test cases
- **Total Assertions**: ≥ 170 test cases with 100% pass rate requirement (exit code 0).
