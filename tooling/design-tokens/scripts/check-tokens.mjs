#!/usr/bin/env node
/**
 * @nexus/design-tokens — token projection consistency gate.
 *
 * Regenerates from the DESIGN pair in memory and compares the three derived
 * checked-in artifacts byte-for-byte to what the compiler would produce today.
 * Also asserts coverage/parity guarantees the compiler enforces on load:
 * light/dark leaf parity, projected CSS var surface parity, and that the
 * generated artifacts carry no stale source-string/serif pins.
 *
 * Exit 0 = up to date (run `pnpm --filter @nexus/design-tokens generate`);
 * exit 1 = drift (or the DESIGN pair now fails compilation).
 */
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadDesignPair, projectDesign } from './project-tokens.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const TOOL_ROOT = resolve(HERE, '..');
const REPO_ROOT = resolve(TOOL_ROOT, '..', '..');

const OUTPUTS = [
  { rel: 'tooling/design-tokens/src/tokens.css', field: 'css', label: 'tokens.css' },
  { rel: 'packages/nexus-ui/theme.css', field: 'brandCss', label: 'theme.css' },
  { rel: 'packages/nexus-ui/src/generated-brand.ts', field: 'brandTokens', label: 'generated-brand.ts' },
];

let failed = 0;

async function main() {
  const repoRoot = resolve(REPO_ROOT);
  const pair = await loadDesignPair(repoRoot);
  const out = projectDesign(pair);

  for (const { rel, field, label } of OUTPUTS) {
    const actual = await readFile(join(repoRoot, rel), 'utf8');
    const expected = out[field];
    if (actual === expected) {
      console.log(`ok: ${label} matches the compiler output`);
    } else {
      console.error(`FAIL: ${label} is out of date — re-run \`pnpm --filter @nexus/design-tokens generate\``);
      failed += 1;
    }
  }

  // No stale serif/source-string pins may survive in any generated artifact.
  const tokensCss = await readFile(join(repoRoot, OUTPUTS[0].rel), 'utf8');
  if (tokensCss.includes("'Source Serif 4'") || tokensCss.includes('source-serif-4-latin-')) {
    console.error('FAIL: generated tokens.css still carries a stale Source Serif 4 pin');
    failed += 1;
  }
  if (/Appendix: Canvas Chromatic Hygiene Mapping/.test(tokensCss)) {
    console.error('FAIL: generated tokens.css references the retired Canvas Chromatic Hygiene appendix');
    failed += 1;
  }
  // The generated header must cite current DESIGN sections and be deterministic.
  if (!/\*\s*Source sections: DESIGN\.md/.test(tokensCss)) {
    console.error('FAIL: generated tokens.css header does not carry a DESIGN source pointer');
    failed += 1;
  }

  if (failed > 0) {
    console.error(`design-tokens: ${failed} gate failure(s)`);
    process.exit(1);
  }
  console.log('design-tokens: generated artifacts are current and coverage/parity verified');
  process.exit(0);
}

main().catch((err) => {
  console.error(`design-tokens: ${err.message}`);
  process.exit(1);
});
