import { readFileSync } from 'node:fs';

const plan = readFileSync(new URL('../plan.md', import.meta.url), 'utf8');
const normalized = plan.replace(/\s+/g, ' ');
const required = [
  '### HLD goal',
  '## 3. HLD architecture',
  '## 5. Stitch and I/O contracts',
  '## 6. Parallel execution plan',
  '## 7. Validation matrix',
  '## 8. Browser validation loop contract',
  '## 9. Index refactor requirements',
  '## 12. Unlazy execution log',
  'Depth: tree 1',
  'OWNS:',
  'Runtime statuses are exactly `PENDING`, `RUNNING`, `PASSED`, `FAILED`, and `BLOCKED`.'
];

for (const token of required) {
  if (!(plan.includes(token) || normalized.includes(token))) {
    throw new Error(`PLAN_TOKEN_MISSING:${token}`);
  }
}

for (let index = 1; index <= 18; index += 1) {
  const id = `### E${String(index).padStart(2, '0')}.`;
  if (!plan.includes(id)) throw new Error(`LLD_ENTITY_MISSING:${id}`);
}

for (let index = 0; index <= 9; index += 1) {
  if (!plan.includes(`### G${index}:`)) throw new Error(`VALIDATION_GATE_MISSING:G${index}`);
}

if (plan.includes('DEMO_ONLY')) throw new Error('FORBIDDEN_STATUS:DEMO_ONLY');
console.log('PLAN_CONTRACT_OK');
