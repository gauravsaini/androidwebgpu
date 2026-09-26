// Unlazy CHECK oracle: no unit crate may import another unit crate's internals.
// Only `pathn_contracts` (frozen contracts) may be shared. Prints success marker
// only when zero cross-imports are found.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const unitsDir = join(root, 'units');
if (!existsSync(unitsDir)) {
  console.log('no cross imports');
  process.exit(0);
}

const crates = readdirSync(unitsDir).filter((d) =>
  existsSync(join(unitsDir, d, 'Cargo.toml')),
);
const libNames = crates.map((c) => c.replace(/-/g, '_'));
const violations = [];

function walk(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else if (e.name.endsWith('.rs')) {
      const src = readFileSync(p, 'utf8');
      const own = dir
        .replace(unitsDir + '/', '')
        .split('/')[0]
        .replace(/-/g, '_');
      for (const lib of libNames) {
        if (lib === own) continue;
        const re = new RegExp(`\\buse\\s+${lib}::`);
        if (re.test(src)) violations.push(`${p}: imports ${lib}`);
      }
    }
  }
}
walk(unitsDir);

if (violations.length > 0) {
  console.error('cross-import violations:\n' + violations.join('\n'));
  process.exit(1);
}
console.log('no cross imports');
