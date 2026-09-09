#!/usr/bin/env node
/**
 * @nexus/design-tokens — generate the three derived outputs from the DESIGN pair.
 *
 * Reads repo-root DESIGN.md / DESIGN.dark.md, compiles via project-tokens.mjs,
 * and writes (only) these three deterministic outputs:
 *   - tooling/design-tokens/src/tokens.css
 *   - packages/nexus-ui/theme.css
 *   - packages/nexus-ui/src/generated-brand.ts
 *
 * This command is the ONLY writer of those artifacts. It never touches source
 * during HMR (the Vite plugin transforms in memory instead).
 */
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const TOOL_ROOT = resolve(HERE, '..');
// default repo root: three directories up from scripts/ (tooling/design-tokens/scripts -> repo root)
const REPO_ROOT = resolve(TOOL_ROOT, '..', '..');

const { loadDesignPair, projectDesign } = await import('./project-tokens.mjs');

/** Output targets (relative to repo root). */
const OUTPUTS = [
  { rel: 'tooling/design-tokens/src/tokens.css', field: 'css' },
  { rel: 'packages/nexus-ui/theme.css', field: 'brandCss' },
  { rel: 'packages/nexus-ui/src/generated-brand.ts', field: 'brandTokens' },
];

let exitCode = 0;

try {
  const repoRoot = process.env.NEXUS_DESIGN_TOKENS_ROOT
    ? resolve(process.env.NEXUS_DESIGN_TOKENS_ROOT)
    : REPO_ROOT;
  const pair = await loadDesignPair(repoRoot);
  const out = projectDesign(pair);
  for (const { rel, field } of OUTPUTS) {
    const target = join(repoRoot, rel);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, out[field], 'utf8');
    console.log(`wrote ${rel} (${out[field].length} bytes)`);
  }
} catch (err) {
  console.error(`generate-tokens.mjs: ${err.message}`);
  exitCode = 1;
}

process.exit(exitCode);
