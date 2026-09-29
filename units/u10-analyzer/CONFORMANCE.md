# U10 Conformance Note: `apk_gpu_analyzer` vs Path N Contracts

- Date: 2026-09-30
- Unit: U10 `apk-pipeline` (`units/u10-analyzer`)
- Contract: `pathn_contracts::machine` (`ApkMeta`, `EngineKind`, `ApkError`)
- Upstream Crate: `crates/apk_gpu_analyzer` (`ApkGpuAnalyzer`, `ApkGpuProfile`, `EngineType`)
- Scope: Contract conformance verification only; **NO rewrite** of `apk_gpu_analyzer`.

See full architecture documentation in [`docs/architecture/CONFORMANCE-U10.md`](../../docs/architecture/CONFORMANCE-U10.md).

## Summary Table

| `apk_gpu_analyzer::EngineType` | `pathn_contracts::machine::EngineKind` | Notes |
| :--- | :--- | :--- |
| `EngineType::Unity` | `EngineKind::Unity` | Matches `libunity.so` or `libmain.so` |
| `EngineType::UnrealEngine` | `EngineKind::Unreal` | Matches `libUE4.so` or `libUnreal` |
| `EngineType::Godot` | `EngineKind::Godot` | Matches `libgodot_android.so` or `libgodot` |
| `EngineType::CustomNativeGles`| `EngineKind::Other` | Contract has no `Native` variant; mapped to `Other` |
| `EngineType::Unknown` | `EngineKind::Other` | Unclassified engines map to `Other` |

## Dropped / Uncovered Fields in `ApkMeta`

1. `requires_vulkan: bool`
2. `supported_texture_formats: Vec<String>`
3. `required_extensions: Vec<String>`
4. `native_libraries: Vec<String>`

## Sideload Integration

ADB wire protocol and mock-adb implementation live in:
- `src/sideload.rs`: `AdbChannel`, `AdbMessage`, `sideload_apk`, `InstallResult`
- `src/mock_adb.rs`: `MockAdbServer`, `AdbTargetFs`
- `tests/mock_adb.rs`: 5 integration tests against real APK fixtures (`unity_cube.apk`, `godot_gles2.apk`) and `guest_image::Rootfs`.
