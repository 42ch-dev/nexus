import { createRequire } from 'node:module';
import { readFileSync, readdirSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { dirname } from 'node:path';

const root = dirname(execSync('git rev-parse --path-format=absolute --git-common-dir', { encoding: 'utf8' }).trim());
const store = `${root}/node_modules/.pnpm`;
const babel = readdirSync(store).find((d) => d.startsWith('@babel+parser@'));
const { parse } = createRequire(`${store}/${babel}/node_modules/@babel/parser/package.json`)('./lib/index.js');
const DROP = new Set(['start', 'end', 'loc', 'range', 'comments', 'leadingComments', 'trailingComments', 'innerComments', 'tokens', 'parenStart', 'trailingComma', 'trailingCommaLoc']);
const plugins = (f) => (/\.(ts|mts|cts)$/.test(f) ? ['typescript'] : /\.tsx$/.test(f) ? ['typescript', 'jsx'] : ['jsx']);
const norm = (f, src) => JSON.stringify(
  parse(src, { sourceType: 'unambiguous', plugins: plugins(f), errorRecovery: false, allowReturnOutsideFunction: true }).program,
  (k, v) => (DROP.has(k) ? undefined : v));
const args = process.argv.slice(2);
let bad = 0, skipped = 0;
for (let i = 0; i < args.length; i += 2) {
  let a;
  try { a = norm(args[i + 1], readFileSync(args[i], 'utf8')); } catch { skipped++; process.stdout.write(`UNPARSEABLE-AT-HEAD ${args[i + 1]}\n`); continue; }
  let b;
  try { b = norm(args[i + 1], readFileSync(args[i + 1], 'utf8')); } catch (e) { bad++; process.stdout.write(`PARSE-FAIL ${args[i + 1]} ${e.message}\n`); continue; }
  if (a !== b) { bad++; process.stdout.write(`DIFF ${args[i + 1]}\n`); }
}
process.stdout.write(`js_ts_files=${args.length / 2} ast_mismatch=${bad} unparseable_at_head=${skipped}\n`);
process.exit(bad ? 1 : 0);
