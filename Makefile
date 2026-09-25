# AndroidWebGPU build contract (E01): one-command browser bundle.
.PHONY: web build verify test serve clean

web: build

build:
	node scripts/build-runtime.mjs

verify:
	node scripts/verify-gates.mjs
	node scripts/verify-plan.mjs
	node scripts/verify-index.mjs
	node scripts/verify-js.mjs
	node scripts/verify-http-entry.mjs
	node scripts/verify-runtime-artifacts.mjs

test:
	cargo test --workspace
	node tests/e2e/runner.mjs

.PHONY: web build verify test serve clean vendor-check

serve:
	node scripts/serve.mjs --serve

vendor-check:
	node scripts/vendor-v86.mjs --check
	node scripts/fetch-android-images.mjs --check

clean:
	rm -rf pkg dist
