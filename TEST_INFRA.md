# E2E Test Infra: androidwebgpu

## Test Philosophy
- Opaque-box, requirement-driven. No dependency on implementation internals.
- Methodology: Category-Partition + Boundary Value Analysis (BVA) + Pairwise Combinatorial + Real-World Workload Testing.

## Feature Inventory
| # | Feature | Source (requirement) | Tier 1 | Tier 2 | Tier 3 | Tier 4 |
|---|---------|---------------------|:------:|:------:|:------:|:------:|
| 1 | Real v86 Boot & Lifecycle | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 2 | Server Security Headers | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 3 | Structured Debug Logging | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 4 | In-UI Logcat Streaming | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 5 | Virtio-GPU Framebuffer Bridge | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 6 | Synthetic Placeholder Removal | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 7 | WebGPU Compositor Live Pixels | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |

## Test Architecture
- Test runner: `tests/run_e2e_tests.mjs`
- Invocation: `node tests/run_e2e_tests.mjs`
- Test case format: ESM test modules with structured assertion runners and pass/fail exit code semantics.
- Directory layout:
  - `tests/e2e/tier1_feature_coverage.mjs`
  - `tests/e2e/tier2_boundary_corner.mjs`
  - `tests/e2e/tier3_cross_feature.mjs`
  - `tests/e2e/tier4_real_world.mjs`

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Cold Boot to Shell with Logcat Streaming | F1, F2, F3, F4 | Medium |
| 2 | Virtio-GPU Framebuffer Scanout & WebGPU Damage Scissoring | F1, F3, F5, F7 | High |
| 3 | Full Android Framework & SurfaceFlinger Composition | F1, F3, F5, F6, F7 | High |
| 4 | Logcat Filter & Circular Buffer Stress under Live Boot | F1, F3, F4 | Medium |
| 5 | Long-Running VM Execution & Memory Stability | F1, F5, F7 | High |

## Coverage Thresholds
- Tier 1: >=35 test cases (5 per feature * 7 features)
- Tier 2: >=35 test cases (5 per feature * 7 features)
- Tier 3: >=7 cross-feature interaction test cases
- Tier 4: >=5 realistic application scenarios
- Total minimum: >=82 test cases
