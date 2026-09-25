import { readdirSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const sourceDir = new URL('../src/', import.meta.url);
const sourceFiles = readdirSync(sourceDir)
  .filter((name) => name.endsWith('.js'))
  .map((name) => new URL(`../src/${name}`, import.meta.url));

for (const file of sourceFiles) {
  const result = spawnSync(process.execPath, ['--check', file.pathname], { encoding: 'utf8' });
  if (result.status !== 0) {
    process.stderr.write(result.stderr || result.stdout || `JS_PARSE_FAILED:${file.pathname}\n`);
    process.exit(result.status || 1);
  }
}

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const match = html.match(/<script type="module">([\s\S]*?)<\/script>/);
if (!match) throw new Error('INLINE_MODULE_MISSING');
const inline = spawnSync(process.execPath, ['--input-type=module', '--check'], {
  input: match[1],
  encoding: 'utf8'
});
if (inline.status !== 0) {
  process.stderr.write(inline.stderr || inline.stdout || 'INLINE_JS_PARSE_FAILED\n');
  process.exit(inline.status || 1);
}

console.log('JS_CONTRACT_OK');
