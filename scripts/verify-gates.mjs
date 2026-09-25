import { readFileSync } from 'node:fs';

const gates = readFileSync(new URL('../GATES.md', import.meta.url), 'utf8');
if (!/^OWNS:\s+.+$/m.test(gates)) throw new Error('GATES_OWNS_MISSING');
if (!/^Scope:\s+.+$/m.test(gates)) throw new Error('GATES_SCOPE_MISSING');

const blocks = gates.split(/\n(?=- \[[ x]\] P\d+:)/).filter((block) => /^- \[[ x]\] P\d+:/m.test(block));
if (blocks.length === 0) throw new Error('GATES_EMPTY');

const ids = new Set();
for (const block of blocks) {
  const id = block.match(/^- \[[ x]\] (P\d+):/m)?.[1];
  if (!id || ids.has(id)) throw new Error(`GATE_ID_INVALID_OR_DUPLICATE:${id || 'missing'}`);
  ids.add(id);
  if (!/^  CHECK:\s+\S.+$/m.test(block)) throw new Error(`GATE_CHECK_MISSING:${id}`);
  if (!/^  EXPECT:\s+\S.+$/m.test(block)) throw new Error(`GATE_EXPECT_MISSING:${id}`);
  if (!/^  EVIDENCE:\s+\S.+$/m.test(block)) throw new Error(`GATE_EVIDENCE_MISSING:${id}`);
}

if (gates.includes('DEMO_ONLY')) throw new Error('FORBIDDEN_STATUS:DEMO_ONLY');
console.log('GATES_FORMAT_OK');
