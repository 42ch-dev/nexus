// @vitest-environment node
/**
 * Public-export fixture coverage assertion (plan v1218-studio-fixtures, Task 2).
 *
 * Enumerates the runtime value exports of `@42ch/nexus-ui` (parsed from
 * packages/nexus-ui/src/index.ts) and asserts each one is either covered by
 * a Studio fixture (FIXTURE_COVERAGE → data-testid present in Studio source)
 * or explicitly allowlisted (FIXTURE_ALLOWLIST, brand/token utilities).
 * Fails when a new public export ships without a fixture entry, when an
 * entry names an export the package no longer ships, or when a registered
 * testid is missing from the Studio source. Deterministic + cheap: plain
 * file reads, no rendering.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  FIXTURE_ALLOWLIST,
  FIXTURE_COVERAGE,
} from '@/fixtures/fixture-coverage';

const REPO_ROOT = join(import.meta.dirname, '..', '..', '..', '..', '..');
const INDEX_TS = join(REPO_ROOT, 'packages/nexus-ui/src/index.ts');
const STUDIO_SRC = join(REPO_ROOT, 'apps/design-studio/src');

/** Parse `export { a, type B, ... } from '...'` value export names. */
function parseValueExports(indexSource: string): string[] {
  const names: string[] = [];
  for (const match of indexSource.matchAll(/export\s*\{([^}]+)\}/g)) {
    // Strip line comments inside the clause (e.g. promotion annotations).
    const clause = match[1].replace(/\/\/[^\n]*/g, '');
    for (const part of clause.split(',')) {
      const token = part.trim();
      if (token === '' || token.startsWith('type ')) continue;
      // `Foo as Bar` exports Bar.
      names.push(token.replace(/^.*\bas\s+/, '').trim());
    }
  }
  return names;
}

function collectSourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules' || entry.startsWith('.')) continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) files.push(...collectSourceFiles(full));
    else if (/\.(tsx?|jsx?)$/.test(entry)) files.push(full);
  }
  return files;
}

/** Collect static data-testid literals and template-literal prefixes. */
function collectTestIds(): string[] {
  const ids: string[] = [];
  for (const file of collectSourceFiles(STUDIO_SRC)) {
    const source = readFileSync(file, 'utf8');
    for (const match of source.matchAll(/data-testid=\{?["`]([^"`$]*)["`]/g)) {
      ids.push(match[1]);
    }
    // Template-literal testids: capture up to the closing backtick, then
    // keep the static prefix before the first interpolation.
    for (const match of source.matchAll(/data-testid=\{`([^`]*)`/g)) {
      ids.push(match[1].split('${')[0]);
    }
  }
  return ids;
}

describe('public-export fixture coverage', () => {
  const indexSource = readFileSync(INDEX_TS, 'utf8');
  const valueExports = parseValueExports(indexSource);
  const testIds = collectTestIds();

  it('parses the package index (sanity: known exports present)', () => {
    expect(valueExports).toEqual(expect.arrayContaining(['Button', 'Dialog', 'FormDialog']));
    expect(valueExports).not.toContain('ButtonProps');
  });

  it('every public value export has a fixture or an allowlist entry', () => {
    const uncovered = valueExports.filter(
      (name) => !(name in FIXTURE_COVERAGE) && !FIXTURE_ALLOWLIST.includes(name),
    );
    expect(uncovered).toEqual([]);
  });

  it('no stale registry entries (entry names a non-export)', () => {
    const stale = Object.keys(FIXTURE_COVERAGE).filter(
      (name) => !valueExports.includes(name),
    );
    expect(stale).toEqual([]);
  });

  it('no allowlist entries that actually have fixtures', () => {
    const redundant = FIXTURE_ALLOWLIST.filter((name) => name in FIXTURE_COVERAGE);
    expect(redundant).toEqual([]);
  });

  it('every registered fixture testid exists in the Studio source', () => {
    const missing = Object.entries(FIXTURE_COVERAGE).filter(([, testId]) => {
      const isPrefix = testId.endsWith('-');
      return !testIds.some((id) => (isPrefix ? id.startsWith(testId) : id === testId));
    });
    expect(missing).toEqual([]);
  });
});
