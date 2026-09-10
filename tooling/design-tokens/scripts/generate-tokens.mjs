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
import { mkdir, rename, rm, writeFile } from 'node:fs/promises';
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
  // Write each artifact to a temp file in its target directory first; only
  // after ALL writes succeed, replace the targets (atomic as a set), so a
  // failure mid-loop cannot leave mixed-generation output.
  const staged = [];
  try {
    for (const { rel, field } of OUTPUTS) {
      const target = join(repoRoot, rel);
      await mkdir(dirname(target), { recursive: true });
      const tmp = `${target}.${process.pid}.tmp`;
      await writeFile(tmp, out[field], 'utf8');
      staged.push({ rel, field, target, tmp });
    }
    for (const { rel, field, target, tmp } of staged) {
      await rename(tmp, target);
      console.log(`wrote ${rel} (${out[field].length} bytes)`);
    }
  } finally {
    // Best-effort cleanup of any temp file that was not renamed.
    await Promise.all(staged.map(({ tmp }) => rm(tmp, { force: true })));
  }
} catch (err) {
  console.error(`generate-tokens.mjs: ${err.message}`);
  exitCode = 1;
}

process.exit(exitCode);
