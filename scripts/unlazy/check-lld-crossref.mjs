// Unlazy CHECK oracle: every `pub struct/enum` named in LLD.md §0 rust fences
// must exist in contracts/src/*.rs. Prints the success marker only when the
// LLD type set is a subset of the implemented type set.
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const lld = readFileSync(join(root, 'docs/architecture/LLD.md'), 'utf8');

const fences = [...lld.matchAll(/```rust([\s\S]*?)```/g)].map((m) => m[1]).join('\n');
const lldTypes = new Set(
  [...fences.matchAll(/pub\s+(?:struct|enum)\s+(\w+)/g)].map((m) => m[1]),
);

const srcDir = join(root, 'contracts', 'src');
let rsSrc = '';
for (const f of readdirSync(srcDir)) {
  if (f.endsWith('.rs')) rsSrc += readFileSync(join(srcDir, f), 'utf8') + '\n';
}
const rsTypes = new Set(
  [...rsSrc.matchAll(/pub\s+(?:struct|enum)\s+(\w+)/g)].map((m) => m[1]),
);

const missing = [...lldTypes].filter((t) => !rsTypes.has(t));
if (missing.length > 0) {
  console.error('missing contract types: ' + missing.join(', '));
  process.exit(1);
}
console.log(`lld crossref passed (${lldTypes.size} types)`);
