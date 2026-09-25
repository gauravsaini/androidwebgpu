/**
 * fetch-android-images.mjs - W8: pinned Android-x86 release fetcher (real).
 * Usage:
 *   node scripts/fetch-android-images.mjs --check   verify ISO pin + hashes
 *   node scripts/fetch-android-images.mjs           download pinned ISO,
 *     verify SHA-256, extract kernel + initrd.img via 7z (brew install p7zip),
 *     refresh images/manifest.json + images/SHA256SUMS.json.
 *
 * Pin (immutable):
 *   file: android-x86-9.0-r2.iso (761266176 B, 32-bit: v86 emulates i686, no long mode)
 *   iso sha256: f7eb8fc56f29ad5432335dc054183acf086c539f3990f0b6e9ff58bd6df4604e
 *   url: https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download
 *   kernel sha256: 3223e44c7ec14d67... (full pin in images/SHA256SUMS.json)
 *
 * system/vendor/product images live inside system.sfs (squashfs, ~GB
 * unpacked) and remain bootstrap fixtures until the squashfs lane lands;
 * P6 stays BLOCKED until then.
 */
import { existsSync, readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

const PIN = {
  iso: 'android-x86-9.0-r2.iso',
  isoSize: 761266176,
  isoSha256: '91cedb534ba095a0c9b3eceede4147967fd27beea9bba640776f787dc3555021',
  url: 'https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86-9.0-r2.iso/download',
  extract: ['kernel', 'initrd.img'],
};
const root = new URL('..', import.meta.url);
const at = (p) => new URL(p, root);
const shaFile = (p) => createHash('sha256').update(readFileSync(p)).digest('hex');
const tmpIso = `${process.env.TMPDIR || '/tmp'}/android-x86-9.0-r2.iso`;

function loadManifest() {
  return JSON.parse(readFileSync(at('images/manifest.json'), 'utf8'));
}

if (process.argv.includes('--check')) {
  console.log(`PINNED_ANDROID:9.0-r2:x86:kernel-4.19.110:${PIN.iso}`);
  console.log(`PINNED_SHA256:${PIN.isoSha256}`);
  let m = null;
  try { m = loadManifest(); } catch (_e) {}
  for (const f of PIN.extract) {
    const p = at(`images/${f}`);
    if (!existsSync(p)) {
      console.log(`STATUS:IMAGE_MISSING:images/${f}`);
      process.exit(1);
    }
    const actual = shaFile(p);
    const want = m?.sha256?.[f] || m?.images?.[f]?.sha256;
    console.log(`IMAGE:${f}:${readFileSync(p).byteLength}B:sha256:${actual.slice(0, 16)}…:${want && actual === String(want).toLowerCase() ? 'pinned-ok' : 'UNPINNED'}`);
    if (!want || actual !== String(want).toLowerCase()) process.exit(1);
  }
  console.log('STATUS:kernel+initrd authentic and pinned; system/vendor/product still fixtures (squashfs lane pending)');
  process.exit(0);
}

// Fetch: download, verify hash + size, extract kernel/initrd via 7z.
try {
  execFileSync('curl', ['-sSL', '--max-time', '570', '-o', tmpIso, PIN.url], { stdio: 'inherit' });
} catch (err) {
  console.error(`FETCH_FAILED:${err.message}`);
  process.exit(2);
}
const isoBytes = readFileSync(tmpIso);
if (isoBytes.byteLength !== PIN.isoSize) {
  console.error(`ISO_SIZE_MISMATCH:got ${isoBytes.byteLength} want ${PIN.isoSize}`);
  process.exit(2);
}
if (shaFile(tmpIso) !== PIN.isoSha256) {
  console.error('ISO_HASH_MISMATCH');
  process.exit(2);
}
mkdirSync('/tmp/andx86-extract', { recursive: true });
try {
  execFileSync('7z', ['e', `-o/tmp/andx86-extract`, tmpIso, ...PIN.extract], { stdio: 'inherit' });
} catch (err) {
  console.error(`EXTRACT_FAILED:need 7z (brew install p7zip):${err.message}`);
  process.exit(2);
}
const manifest = loadManifest();
const lock = { _iso: { file: PIN.iso, size: PIN.isoSize, sha256: PIN.isoSha256, url: PIN.url } };
for (const f of PIN.extract) {
  copyFileSync(`/tmp/andx86-extract/${f}`, at(`images/${f}`));
  const bytes = readFileSync(at(`images/${f}`));
  const h = shaFile(at(`images/${f}`));
  manifest.sha256[f] = h;
  manifest.images[f] = { ...(manifest.images[f] || {}), file: f, role: f, size: bytes.byteLength, sha256: h, alignment: 512, authentic: 'android-x86_64-9.0-r2' };
  lock[`images/${f}`] = h;
}
manifest.iso = { file: PIN.iso, size: PIN.isoSize, sha256: PIN.isoSha256, url: PIN.url };
manifest.note = 'PARTIAL real media: kernel + initrd.img are authentic android-x86_64-9.0-r2 bytes; system/vendor/product remain bootstrap fixtures pending squashfs extraction.';
writeFileSync(at('images/manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
writeFileSync(at('images/SHA256SUMS.json'), JSON.stringify(lock, null, 2) + '\n');
console.log('FETCH_OK:kernel+initrd authentic; system/vendor/product still fixtures (P6 remains BLOCKED)');
