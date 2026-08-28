# Project: Android WebGPU Authentic F-Droid Repository Ingestion & View Hierarchy Rendering

## Architecture
The system consists of:
1. **Repository Ingestion & Decoder (`src/fdroid_index_parser.js` / `src/android_network.js`)**:
   - Ingests authentic F-Droid index (`index-v1.jar`, `index-v1.json`, or live F-Droid V1/V2 metadata).
   - Uses `ApkZipReader` / RFC 1951 DEFLATE to unpack `index-v1.json` from `index-v1.jar`.
   - Parses repository metadata into structured records: `packageName`, `name` / `applicationLabel`, `versionName`, `summary`, `description`, `icon`, `categories`.
2. **Dynamic Layout & Adapter Data Binding (`src/android_runtime.js`)**:
   - Eliminates hardcoded mock package arrays (`fdroidRepoApps`).
   - Inflates authentic `res/v9.xml` (`activity_main`) to construct `RelativeLayout` with `RecyclerView` (ID `2131296621`).
   - Inflates authentic `res/Kt.xml` (`app_list_item`) for each authentic repository index item.
   - Binds repository records to widget IDs: `app_name` (`2131296365`), `status`/summary (`2131296872`), `icon` (`2131296574`), `action_button` (`2131296316`).
   - Mounts items to `RecyclerView` with accurate layout parameters.
3. **View Hierarchy & Rasterization (`src/view_hierarchy.js`, `src/view_rasterizer.js`)**:
   - Manages View tree layout, positioning, measurement, and relative coordinate rendering during `ViewGroup.dispatchDraw`.
   - Renders 720x1440 portrait buffer to WebGPU VirtIO scanout / canvas.
4. **Verification & Testing (`validate_browser.mjs`, `tests/`)**:
   - Headless browser validation verifying `org.fdroid.fdroid` launch, 720x1440 canvas rendering, and Shannon entropy $H \ge 1.0$.
   - Offline screenshot audit (`screenshot.png`) and comprehensive test suite (`pnpm test`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Archive & Index Ingestion | Pure-JS extraction of `index-v1.json` from `index-v1.jar` using `ApkZipReader` | M1 | Survey / R1 |
| 2 | Index V1/V2 Schema Parser | Parse repository metadata (`repo`, `apps`, `packages`), extracting real package definitions | M1 | Survey / R1 |
| 3 | Static Array Elimination | Remove `fdroidRepoApps` static mock array from `src/android_runtime.js` | M2 | Survey / R1 / R2 |
| 4 | Binary Layout Inflation | Inflate authentic `res/v9.xml` (`activity_main`) and `res/Kt.xml` (`app_list_item`) | M2 | Survey / R2 |
| 5 | Dynamic Data Binding | Bind authentic repository items to `res/Kt.xml` view IDs (`app_name`, `summary`, `icon`) inside `RecyclerView` | M2 | Survey / R2 |
| 6 | ViewGroup Coordinate Dispatch | Fix nested coordinate translation in `ViewGroup.dispatchDraw` to prevent child view clipping | M3 | Survey / R3 |
| 7 | WebGPU Scanout & Canvas Rasterization | Render edge-to-edge 720x1440 view hierarchy to WebGPU scanout 0 / canvas | M3 | Survey / R3 |
| 8 | Headless Browser Validation | `validate_browser.mjs` passes with Shannon entropy $H \ge 1.0$ | M3 | Survey / R3 |
| 9 | Full Test Suite Execution | `pnpm test` passes all test suites with 0 failures | M3 | Survey / Acceptance Criteria |
| 10 | Screenshot Verification | `screenshot.png` captures edge-to-edge live repository catalog | M3 | Survey / Acceptance Criteria |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Authentic F-Droid Index Ingestion | Implement `FdroidIndexParser` to decode `index-v1.jar`, `index-v1.json`, and V2 metadata, extracting package records | none | DONE |
| M2 | Dynamic Layout & Adapter Data Binding | Eliminate `fdroidRepoApps` mock array in `src/android_runtime.js`, inflate `res/v9.xml` and `res/Kt.xml`, and bind parsed index items | M1 | PLANNED |
| M3 | WebGPU Scanout & Visual Validation | Fix `ViewGroup` coordinate clipping, verify 720x1440 WebGPU scanout, pass `validate_browser.mjs` ($H \ge 1.0$), and pass `pnpm test` | M2 | PLANNED |

## Interface Contracts
### FdroidIndexParser ↔ AndroidRuntime
```typescript
interface FdroidRepoApp {
    packageName: string;
    name: string;
    applicationLabel: string;
    summary: string;
    description: string;
    icon: string;
    versionName: string;
    versionCode: number;
    color?: string;
    categories?: string[];
}

interface FdroidIndexResult {
    repo: {
        name: string;
        timestamp: number;
        icon?: string;
    };
    apps: FdroidRepoApp[];
}

class FdroidIndexParser {
    static parseIndexJar(jarBuffer: ArrayBuffer | Uint8Array): FdroidIndexResult;
    static parseIndexJson(jsonTextOrObj: string | object): FdroidIndexResult;
}
```

### Layout Inflation & View Binding
```typescript
// res/v9.xml (activity_main)
const ID_RECYCLER_VIEW = 2131296621; // 0x7f09016d (main_view_pager)
const ID_BOTTOM_NAV = 2131296392;    // 0x7f090088 (bottom_navigation)

// res/Kt.xml (app_list_item)
const ID_APP_NAME = 2131296365;      // 0x7f09006d (app_name)
const ID_APP_SUMMARY = 2131296872;   // 0x7f090268 (status)
const ID_APP_ICON = 2131296574;      // 0x7f09013e (icon)
const ID_ACTION_BUTTON = 2131296316; // 0x7f09003c (action_button)
```

## Code Layout
- `src/fdroid_index_parser.js`: Authentic F-Droid index ingestion & parser.
- `src/android_runtime.js`: Android runtime lifecycle, layout inflation, dynamic item binding (no static mock arrays).
- `src/view_hierarchy.js`: Android View, ViewGroup, RecyclerView, TextView, ImageView classes & dispatchDraw logic.
- `src/view_rasterizer.js`: 720x1440 ViewRootImpl & ViewHierarchyRasterizer software renderer.
- `src/apk_client_parser.js`: Zero-dependency ZIP/JAR reader & AXML decoder.
- `validate_browser.mjs`: Headless browser validation runner.
- `tests/`: Automated test suite.
