import { existsSync, readFileSync } from 'node:fs';

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

const manifest = JSON.parse(readFileSync(new URL('images/manifest.json', root), 'utf8'));
if (!manifest.androidVersion || !manifest.abi || !manifest.sha256) {
  throw new Error('ANDROID_IMAGE_MANIFEST_INCOMPLETE');
}
console.log('ANDROID_RUNTIME_ACCEPTED');
