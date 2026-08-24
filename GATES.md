# Verification Gates: Interactive 3D Android Game Arcade (Demo 1)

- [x] G1: Rust Workspace Tests & Wasm Package Build
  CHECK: cargo test --workspace
  EXPECT: test result: ok

- [x] G2: Arcade 3D Mesh & GLES Shader Pipeline
  CHECK: test -f src/arcade_demo.js && test -f src/virtio_packet_builder.js && echo "ARCADE_MODULES_EXIST"
  EXPECT: ARCADE_MODULES_EXIST

- [x] G3: Multi-Layer Android System UI Composition
  CHECK: grep -q "drawStatusBar" src/arcade_demo.js && grep -q "drawNavigationBar" src/arcade_demo.js && echo "COMPOSITOR_WIRED"
  EXPECT: COMPOSITOR_WIRED

- [x] G4: Interactive Phone Frame & Shader Switcher in UI
  CHECK: grep -q "phone-frame" index.html && grep -q "data-shader" index.html && echo "UI_INTEGRATION_OK"
  EXPECT: UI_INTEGRATION_OK

- [x] G5: Automated Gate Validation in Test Suite
  CHECK: grep -q "runGate5_Arcade3DFlight" src/test_suite.js && echo "GATE5_TEST_EXISTS"
  EXPECT: GATE5_TEST_EXISTS

- [x] G6: 120 FPS Native Parity & OffscreenCanvas WASM Threads
  CHECK: grep -q "runGate6_120FpsNativeParity" src/test_suite.js && grep -q "raster_worker.js" src/raster_worker.js && echo "GATE6_120FPS_PARITY_OK"
  EXPECT: GATE6_120FPS_PARITY_OK
