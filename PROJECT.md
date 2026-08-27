# Project: Android WebGPU Authentic View System

## Architecture
The Android WebGPU Authentic View System eliminates all simulated HTML DOM screens and synthetic application mockups. It establishes an authentic Android UI runtime pipeline:
1. **APK Extraction & Resource Resolution**: `ApkZipReader` and `ArscDecoder` decode `resources.arsc` with full `ResTable_config` matching and `TypedValue` unit decoding (dp, sp, px, hex color, integer, style bags).
2. **Binary XML Layout Inflation**: `LayoutInflater` parses binary XML (`res/layout/*.xml`) and constructs live in-memory `View` / `ViewGroup` trees (`FrameLayout`, `LinearLayout` with weights, `RelativeLayout`, `ConstraintLayout`, `ScrollView`, `RecyclerView`, `TextView`, `ImageView`, `Button`).
3. **WebGPU Hardware Rasterization**: `ViewHierarchyRasterizer` paints measured/laid out View trees to `OffscreenCanvas` / WebGPU texture buffers with Material Design 3 styling, text metrics, background tints, vector drawables, and rounded corners. Pixel buffers are submitted to `SurfaceFlinger` / `VirtioGpuBridge` via `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH`.
4. **Guest VM & Window Stacking**: `wms_rs` and `SurfaceComposerService` composite Guest VM scanouts (z=0), App Activity windows (z=1..100), and SystemUI (z=1000) inside `WebGpuCompositor` directly to `<canvas id="screen">`.
5. **Input & Navigation Dispatch**: Canvas pointer/touch and keyboard events route through `inputflinger_rs` to `ViewRootImpl`, traversing the in-memory View tree via reverse-Z hit-testing and dispatching navigation keys (`KEYCODE_BACK`) to `ams_rs` Activity lifecycle stack.

```
       ┌───────────────────────────────┐
       │   APK (e.g. F-Droid.apk)      │
       └──────────────┬────────────────┘
                      │
           ┌──────────▼──────────┐
           │ resources.arsc      │
           │ (ArscDecoder)       │
           └──────────┬──────────┘
                      │ Resource IDs (@string, @color, @dimen, @layout)
           ┌──────────▼──────────┐
           │ Binary XML Layouts  │
           │ (LayoutInflater)    │
           └──────────┬──────────┘
                      │
           ┌──────────▼──────────────────────────────┐
           │ In-Memory View / ViewGroup Hierarchy    │
           │ (FrameLayout, LinearLayout, Relative...) │
           └──────────┬──────────────────────────────┘
                      │ Measure -> Layout -> Draw
           ┌──────────▼──────────────────────────────┐
           │ ViewHierarchyRasterizer (OffscreenCanvas)│
           └──────────┬──────────────────────────────┘
                      │ RGBA Pixel Buffers
           ┌──────────▼──────────────────────────────┐
           │ VirtioGpuBridge / SurfaceFlinger        │
           └──────────┬──────────────────────────────┘
                      │
           ┌──────────▼──────────────────────────────┐
           │ WebGpuCompositor (WGSL Multi-Plane)     │
           │ Guest VM Scanout (z=0) + App (z=1..100) │
           └──────────┬──────────────────────────────┘
                      │
           ┌──────────▼──────────────────────────────┐
           │ <canvas id="screen"> (Zero DOM Mockups) │
           └─────────────────────────────────────────┘
```

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | APK Zip & Binary XML Layout Extraction | Extract `res/layout/*.xml` and obfuscated layout paths from APKs | M1 | ORIGINAL_REQUEST §R1 |
| 2 | ARSC String Pool & Config Matching | Parse `resources.arsc` with `ResTable_config` locale/density filtering | M1 | ORIGINAL_REQUEST §R1 |
| 3 | Resource Identifier Resolution | Resolve `@string/*`, `@drawable/*`, `@color/*`, `@dimen/*`, `@layout/*`, `@id/*`, `@style/*` | M1 | ORIGINAL_REQUEST §R1 |
| 4 | TypedValue Unit Decoding | Decode dimension units (dp, sp, px), hex colors, integers, and dimension calculations | M1 | ORIGINAL_REQUEST §R1 |
| 5 | View & ViewGroup Hierarchy | Implement `View`, `ViewGroup`, `LayoutParams`, `MeasureSpec` with child-parent relationships | M1 | ORIGINAL_REQUEST §R1 |
| 6 | Layout Container Types | Implement `FrameLayout`, `LinearLayout` (weight), `RelativeLayout`, `ConstraintLayout`, `ScrollView`, `RecyclerView` | M1 | ORIGINAL_REQUEST §R1 |
| 7 | Core Widget Types | Implement `TextView`, `ImageView`, `Button` with attributes and state | M1 | ORIGINAL_REQUEST §R1 |
| 8 | Complete Synthetic DOM Elimination | Delete 13 synthetic HTML DOM functions (1,353 lines) in `android_runtime.js` and `#screen-app` mocks | M2 | ORIGINAL_REQUEST §R2 |
| 9 | OffscreenCanvas Material Design 3 Rasterizer | Render View tree to OffscreenCanvas with text metrics, tints, vector paths, rounded corners | M2 | ORIGINAL_REQUEST §R2 |
| 10 | WebGPU / SurfaceFlinger Hardware Submission | Submit rasterized buffers through `VirtioGpuBridge` / `SurfaceFlinger` to WebGPU swapchain | M2 | ORIGINAL_REQUEST §R2 |
| 11 | Guest VM Scanout Presentation | Render Linux kernel DRM/fbcon scanouts directly to WebGPU without DOM overlays | M3 | ORIGINAL_REQUEST §R3 |
| 12 | WindowManager Surface Stacking & Switching | Stack Guest VM (z=0) and App Activity (z=1..100) via `wms_rs` and `SurfaceComposerService` | M3 | ORIGINAL_REQUEST §R3 |
| 13 | Input Event Injection & Routing | Route canvas mouse/touch/key events through `inputflinger_rs` to `ViewRootImpl` | M4 | ORIGINAL_REQUEST §R4 |
| 14 | In-Memory View Tree Hit-Testing & Bubbling | Reverse-Z top-down hit testing, `onTouchEvent`, `onClick`, `onScroll` in View hierarchy | M4 | ORIGINAL_REQUEST §R4 |
| 15 | Hardware Navigation & Activity Backstack | Dispatch Back/Home/Recents keys to `ams_rs` lifecycle and pop Activity stack | M4 | ORIGINAL_REQUEST §R4 |
| 16 | E2E Test Suite (Tiers 1-4) | Comprehensive opaque-box test suite for all features | T1 | ORIGINAL_REQUEST §Acceptance |
| 17 | Adversarial Hardening (Tier 5) | White-box adversarial testing and edge case verification | M5 | ORIGINAL_REQUEST §Acceptance |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| T1 | E2E Testing Track: Opaque-Box Test Suite | Build test infrastructure and Tiers 1-4 test cases (≥11×N tests), publish `TEST_INFRA.md` and `TEST_READY.md` | none | IN_PROGRESS |
| M1 | Authentic Resource Resolution & Binary XML Inflation | Implement `src/apk_resource_resolver.js`, `src/view_hierarchy.js`, `LayoutInflater`, typed value decoding, and resource resolution | none | IN_PROGRESS |
| M2 | WebGPU Hardware View Rasterization & Mock Deletion | Delete all synthetic DOM generators in `src/android_runtime.js`, implement `src/view_rasterizer.js`, submit to WebGPU swapchain | M1 | PLANNED |
| M3 | Guest VM Framebuffer Integration & Surface Stacking | Integrate guest DRM scanout with `wms_rs` surface stack and seamless switching | M2 | PLANNED |
| M4 | Touch, Gesture & Key Dispatch to View Trees | Implement canvas input routing via `inputflinger_rs`, reverse-Z hit testing, and `ams_rs` backstack | M1, M2, M3 | PLANNED |
| M5 | Final Milestone: 100% E2E Pass & Adversarial Hardening | Phase 1: Pass 100% Tiers 1-4 E2E tests. Phase 2: Tier 5 adversarial coverage hardening | T1, M1, M2, M3, M4 | PLANNED |

## Interface Contracts
### `src/apk_resource_resolver.js`
- `class ArscDecoder`:
  - `decode(arrayBuffer: ArrayBuffer, locale?: string): ArscResourceTable`
- `class ArscResourceTable`:
  - `resolveString(resId: number): string | null`
  - `resolveColor(resId: number): string | null`
  - `resolveDimension(resId: number, density?: number): number | null`
  - `resolveLayoutPath(resId: number | string): string | null`
  - `resolveIdentifier(name: string, type: string, pkg?: string): number | null`

### `src/view_hierarchy.js`
- `class View`:
  - `id: number`, `layoutParams: LayoutParams`, `visibility: number`, `background: any`, `padding: [number, number, number, number]`
  - `measure(widthMeasureSpec: number, heightMeasureSpec: number): void`
  - `layout(left: number, top: number, right: number, bottom: number): void`
  - `draw(canvas: OffscreenCanvasRenderingContext2D): void`
  - `dispatchTouchEvent(event: MotionEvent): boolean`
- `class ViewGroup extends View`:
  - `children: View[]`, `addView(view: View, params?: LayoutParams): void`, `removeView(view: View): void`
- `class FrameLayout extends ViewGroup`, `class LinearLayout extends ViewGroup`, `class RelativeLayout extends ViewGroup`, `class ConstraintLayout extends ViewGroup`, `class ScrollView extends ViewGroup`, `class RecyclerView extends ViewGroup`
- `class TextView extends View`, `class ImageView extends View`, `class Button extends TextView`
- `class LayoutInflater`:
  - `inflate(xmlBuffer: Uint8Array | object, resourceResolver: ArscResourceTable, parent?: ViewGroup): View`

### `src/view_rasterizer.js`
- `class ViewHierarchyRasterizer`:
  - `rasterize(rootView: View, width: number, height: number): { width: number, height: number, rgbaData: Uint8Array }`
  - `submitToVirtioGpu(bridge: VirtioGpuBridge, resourceId: number, scanoutId: number, buffer: Uint8Array): void`

## Code Layout
- `src/apk_resource_resolver.js`: Authentic ARSC parsing, config matcher, typed values.
- `src/view_hierarchy.js`: Android View / ViewGroup hierarchy, LayoutParams, MeasureSpec, LayoutInflater.
- `src/view_rasterizer.js`: OffscreenCanvas Material Design 3 hardware rasterizer and SurfaceFlinger bridge submitter.
- `src/android_runtime.js`: Real Activity lifecycle, ContentView binding, zero synthetic DOM generation.
- `src/app_controller.js`: Application lifecycle, input routing from canvas to active ViewRootImpl.
- `src/virtio_gpu_device.js`: VirtIO GPU device emulation, guest DMA scanout blitting, WebGPU swapchain presentation.
- `tests/e2e_view_system.test.mjs`: E2E test harness for authentic view inflation, hardware rasterization, and input routing.
