# Gates: Android OS browser contract

OWNS: plan.md, index.html, GATES.md, scripts/**
Scope: make the browser entry truthful while the real Android runtime is built

- [x] P0: gate ledger syntax is valid
  CHECK: node scripts/verify-gates.mjs
  EXPECT: GATES_FORMAT_OK
  EVIDENCE: 2026-09-25 `GATES_FORMAT_OK`

- [x] P1: plan contract is complete
  CHECK: node scripts/verify-plan.mjs
  EXPECT: PLAN_CONTRACT_OK
  EVIDENCE: 2026-09-25 `PLAN_CONTRACT_OK`

- [x] P2: browser entry uses only the production runtime contract
  CHECK: node scripts/verify-index.mjs
  EXPECT: INDEX_CONTRACT_OK
  EVIDENCE: 2026-09-25 `INDEX_CONTRACT_OK`

- [x] P3: JavaScript sources parse
  CHECK: node scripts/verify-js.mjs
  EXPECT: JS_CONTRACT_OK
  EVIDENCE: 2026-09-25 `JS_CONTRACT_OK`

- [x] P4: Rust workspace tests pass
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: 2026-09-25 workspace tests passed; 21 non-doc tests, 0 failed

- [x] P5: HTTP entry has a fail-closed runtime load
  CHECK: node scripts/verify-http-entry.mjs
  EXPECT: HTTP_FAIL_CLOSED_OK
  EVIDENCE: 2026-09-25 `HTTP_FAIL_CLOSED_OK`; live page showed missing runtime as `BLOCKED`

- [ ] P6: full Android runtime and guest acceptance
  CHECK: node scripts/verify-runtime-artifacts.mjs
  EXPECT: ANDROID_RUNTIME_ACCEPTED
  EVIDENCE: 2026-09-25 blocked; missing `pkg/android_vm.js`, WASM, and pinned Android images
