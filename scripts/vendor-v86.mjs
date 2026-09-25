/**
 * vendor-v86.mjs - W1: pin + fetch the v86 x86 emulator bundle.
 * Usage:
 *   node scripts/vendor-v86.mjs --check   verify pin + presence + hashes
 *   node scripts/vendor-v86.mjs           download pinned tarball, verify
 *                                         SHA-256, unpack into vendor/v86/
 *
 * Pin (exact commit, immutable tarball URL + SHA-256):
 *   fork:   copy/v86
 *   commit: 5f9a90f2be01243dd0ea4fe014cce12686cf3ced
 *   sha256: 05723b91af25922cb8c0bd6ab49816fd4f1e690047c598bacebf81936c61b277
 *
 * After unpack, built outputs (libv86.js, v86.wasm) still require the v86
 * build lane (`make -C vendor/v86 build`: Rust wasm32 target + Java closure
 * compiler). Until they exist, V86Adapter reports V86_NOT_PRESENT and G1
 * stays BLOCKED. Hashes of present files are locked in
 * vendor/v86/SHA256SUMS.json (TOFU on first fetch, verified thereafter).
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

const PIN = {
  fork: 'copy/v86',
  commit: '5f9a90f2be01243dd0ea4fe014cce12686cf3ced',
  sha256: '05723b91af25922cb8c0bd6ab49816fd4f1e690047c598bacebf81936c61b277',
  url: 'https://github.com/copy/v86/archive/5f9a90f2be01243dd0ea4fe014cce12686cf3ced.tar.gz',
  builtFiles: ['vendor/v86/libv86.js', 'vendor/v86/v86.wasm', 'vendor/v86/bios.bin'],
  lock: 'vendor/v86/SHA256SUMS.json',
};
const root = new URL('..', import.meta.url);
const at = (p) => new URL(p, root);

function shaFile(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

if (process.argv.includes('--check')) {
  console.log(`V86_PIN:${PIN.fork}:${PIN.commit}`);
  const built = PIN.builtFiles.filter((f) => existsSync(at(f)));
  console.log(`VENDORED_BUILT:${built.length}/${PIN.builtFiles.length}:${built.join(',') || 'none'}`);
  console.log(`VENDORED_SOURCE:${existsSync(at('vendor/v86/Makefile')) ? 'yes' : 'no'}`);
  let lockOk = false;
  try {
    const lock = JSON.parse(readFileSync(at(PIN.lock), 'utf8'));
    lockOk = Object.entries(lock)
      .filter(([k]) => !k.startsWith('_'))
      .every(([k, v]) => !v || (existsSync(at(k)) && shaFile(at(k)) === String(v).toLowerCase()));
  } catch (_e) { lockOk = false; }
  console.log(`HASH_LOCK:${lockOk ? 'ok' : 'missing-or-stale'}`);
  if (built.length !== PIN.builtFiles.length) {
    console.log('STATUS:V86_NOT_PRESENT (source vendored; run `make -C vendor/v86 build` for libv86.js + v86.wasm)');
    process.exit(1);
  }
  console.log(lockOk ? 'STATUS:V86_VENDORED' : 'STATUS:V86_HASH_LOCK_MISSING');
  process.exit(lockOk ? 0 : 1);
}

// Fetch: download tarball to a temp dir, verify SHA-256, unpack.
const tmpDir = process.env.TMPDIR || '/tmp';
const tmpTar = `${tmpDir}/v86-${PIN.commit}.tar.gz`;
try {
  execFileSync('curl', ['-sSL', '--max-time', '570', '-o', tmpTar, PIN.url], { stdio: 'inherit' });
} catch (err) {
  console.error(`V86_FETCH_FAILED:${err.message}`);
  process.exit(2);
}
const actual = shaFile(tmpTar);
if (actual !== PIN.sha256) {
  console.error(`V86_HASH_MISMATCH:got ${actual} want ${PIN.sha256}`);
  process.exit(2);
}
mkdirSync(at('vendor/v86'), { recursive: true });
execFileSync('tar', ['-xzf', tmpTar, '-C', at('vendor/v86').pathname, '--strip-components=1'], { stdio: 'inherit' });
// Refresh bios copies + hash lock (preserve existing built-file entries).
try {
  execFileSync('cp', [at('vendor/v86/bios/seabios.bin').pathname, at('vendor/v86/bios.bin').pathname]);
  execFileSync('cp', [at('vendor/v86/bios/vgabios.bin').pathname, at('vendor/v86/vgabios.bin').pathname]);
} catch (_e) {}
let lock = {};
try { lock = JSON.parse(readFileSync(at(PIN.lock), 'utf8')); } catch (_e) {}
lock['_tarball'] = { commit: PIN.commit, sha256: PIN.sha256, url: PIN.url };
for (const f of PIN.builtFiles) {
  if (existsSync(at(f))) lock[f] = shaFile(at(f));
}
writeFileSync(at(PIN.lock), JSON.stringify(lock, null, 2) + '\n');
console.log(`V86_VENDORED_SOURCE:${PIN.commit}`);
console.log('NEXT: `make -C vendor/v86 build` for libv86.js + v86.wasm (needs Rust wasm32 + Java closure).');
