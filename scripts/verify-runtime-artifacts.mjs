import { existsSync, readFileSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';

const root = new URL('..', import.meta.url);
const required = [
  'pkg/android_vm.js',
  'pkg/android_vm_bg.wasm',
  'images/manifest.json',
  'images/kernel',
  'images/initrd.img',
  'images/system.img',
  'images/vendor.img',
  'images/product.img'
];
const missing = required.filter((path) => !existsSync(new URL(path, root)));
if (missing.length > 0) {
  console.error(`ANDROID_RUNTIME_BLOCKED:${missing.join(',')}`);
  process.exit(1);
}

function blocked(reason) {
  console.error(`ANDROID_RUNTIME_BLOCKED:${reason}`);
  process.exit(1);
}

const manifest = JSON.parse(readFileSync(new URL('images/manifest.json', root), 'utf8'));
if (!manifest.androidVersion || !manifest.abi || !manifest.sha256) {
  blocked('ANDROID_IMAGE_MANIFEST_INCOMPLETE');
}

// Per-file hash + size verification against the pinned manifest (E03).
// A large fake image with a mismatched hash, or a manifest with rewritten
// hashes but bootstrap content, cannot pass: sizes must additionally look
// like a real Android-x86 release, not fixtures.
const roles = ['kernel', 'initrd.img', 'system.img', 'vendor.img', 'product.img'];
const entryFor = (role) => manifest.images?.[role] || {};
for (const role of roles) {
  const entry = entryFor(role);
  const file = entry.file || (role === 'kernel' ? 'kernel' : role);
  const bytes = readFileSync(new URL(`images/${file}`, root));
  if (entry.size !== undefined && bytes.byteLength !== Number(entry.size)) {
    blocked(`IMAGE_SIZE_MISMATCH:${role}`);
  }
  const want = manifest.sha256?.[role] || entry.sha256;
  if (!want) blocked(`IMAGE_HASH_MISSING:${role}`);
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (actual !== String(want).toLowerCase()) blocked(`IMAGE_HASH_MISMATCH:${role}`);
}

// Bootstrap fixtures are deterministic placeholders: reject by marker and by
// release-scale sizes regardless of hash consistency.
const marker = JSON.stringify(manifest).toLowerCase();
if (marker.includes('bootstrap-fixture') || manifest.note?.toLowerCase().includes('bootstrap')) {
  blocked('BOOTSTRAP_FIXTURES');
}
const sysBytes = statSync(new URL('images/system.img', root)).size;
if (sysBytes < 50 * 1024 * 1024) blocked(`SYSTEM_IMAGE_TOO_SMALL:${sysBytes}`);

// WASM must be a valid module AND export the runtime entry points.
// The 8-byte empty bootstrap module validates but exports nothing.
const wasmBytes = readFileSync(new URL('pkg/android_vm_bg.wasm', root));
if (wasmBytes.length < 4 || wasmBytes[0] !== 0x00 || wasmBytes[1] !== 0x61 || wasmBytes[2] !== 0x73 || wasmBytes[3] !== 0x6d) {
  blocked('WASM_BAD_MAGIC');
}
if (wasmBytes.length < 1024) blocked(`WASM_BOOTSTRAP_ONLY:${wasmBytes.length}`);
if (typeof WebAssembly === 'undefined' || !WebAssembly.validate(wasmBytes)) {
  blocked('WASM_INVALID_MODULE');
}
const wasmExports = WebAssembly.Module.exports(new WebAssembly.Module(wasmBytes)).map((e) => e.name);
if (!wasmExports.includes('initWasm') && !wasmExports.includes('createAndroidRuntime')) {
  blocked(`WASM_NO_RUNTIME_EXPORTS:${wasmExports.join(',') || 'none'}`);
}
// The JS entry must re-export the same contract (plan section 8).
const entry = readFileSync(new URL('pkg/android_vm.js', root), 'utf8');
if (!entry.includes('createAndroidRuntime') || !entry.includes('runValidationLoop')) {
  blocked('RUNTIME_ENTRY_CONTRACT_MISSING');
}

// v86 x86 core must be vendored (versioned pin + hash lock when present).
const v86Files = ['vendor/v86/libv86.js', 'vendor/v86/v86.wasm', 'vendor/v86/bios.bin'];
const v86Missing = v86Files.filter((p) => !existsSync(new URL(p, root)));
if (v86Missing.length > 0) blocked(`V86_NOT_VENDORED:${v86Missing.join(',')}`);
try {
  const lock = JSON.parse(readFileSync(new URL('vendor/v86/SHA256SUMS.json', root), 'utf8'));
  for (const p of v86Files) {
    const bytes = readFileSync(new URL(p, root));
    const actual = createHash('sha256').update(bytes).digest('hex');
    if (lock[p] && actual !== String(lock[p]).toLowerCase()) blocked(`V86_HASH_MISMATCH:${p}`);
  }
} catch (err) {
  if (err.code !== 'ENOENT') throw err;
  blocked('V86_HASH_LOCK_MISSING:vendor/v86/SHA256SUMS.json');
}

console.log('ANDROID_RUNTIME_ACCEPTED');
