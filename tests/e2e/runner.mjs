#!/usr/bin/env node
import { readdirSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { run } from 'node:test';
import { spec } from 'node:test/reporters';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

function findTestFiles(dir, filterTier = null, filterFeature = null) {
  const files = [];
  const entries = readdirSync(dir);

  for (const entry of entries) {
    const fullPath = join(dir, entry);
    const stat = statSync(fullPath);

    if (stat.isDirectory()) {
      if (entry === 'harness') continue;
      if (filterTier) {
        if (filterTier === '1' && entry !== 'tier1-features') continue;
        if (filterTier === '2' && entry !== 'tier2-boundaries') continue;
        if (filterTier === '3' && entry !== 'tier3-pairwise' && entry !== 'tier3-combinations') continue;
        if (filterTier === '4' && entry !== 'tier4-scenarios') continue;
      }
      files.push(...findTestFiles(fullPath, null, filterFeature));
    } else if (entry.endsWith('.test.mjs')) {
      if (filterFeature) {
        const feat = filterFeature.toLowerCase().replace(/-/g, '_');
        if (!entry.toLowerCase().includes(feat)) continue;
      }
      files.push(fullPath);
    }
  }
  return files.sort();
}

async function main() {
  const args = process.argv.slice(2);
  let tier = null;
  let feature = null;

  for (const arg of args) {
    if (arg.startsWith('--tier=')) {
      tier = arg.split('=')[1];
    } else if (arg.startsWith('--feature=')) {
      feature = arg.split('=')[1];
    }
  }

  const testFiles = findTestFiles(__dirname, tier, feature);

  if (testFiles.length === 0) {
    console.error(`No test files matched criteria (tier=${tier}, feature=${feature})`);
    process.exit(1);
  }

  console.log(`[E2E Runner] Executing ${testFiles.length} test files (Tier filter: ${tier || 'all'}, Feature filter: ${feature || 'all'})...\n`);

  let hasFailures = false;
  const runner = run({ files: testFiles });

  runner.on('test:fail', () => {
    hasFailures = true;
  });

  runner.compose(spec).pipe(process.stdout);

  runner.on('end', () => {
    if (hasFailures) {
      console.error('\n❌ [E2E Runner] Some tests failed.');
      process.exit(1);
    } else {
      console.log('\n✅ [E2E Runner] All tests passed.');
      process.exit(0);
    }
  });
}

main().catch((err) => {
  console.error('Fatal runner error:', err);
  process.exit(1);
});
