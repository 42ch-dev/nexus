/**
 * Regression guard: setup wizard, auth recovery and daemon identity copy must
 * resolve through the locale catalogs. Scans the production sources for CJK
 * text and for literal English in JSX text or copy-bearing props, so a
 * hardcoded string fails here instead of shipping untranslated.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const SOURCES = [
  './step-identity.tsx',
  './step-agent.tsx',
  './step-agent-workspace.tsx',
  './step-first-world.tsx',
  './wizard-shell.tsx',
  './agent-selection.tsx',
  '../access/auth-recovery.tsx',
  '../../lib/client-context.tsx',
] as const;

const CJK = /[\u3400-\u9fff\uff00-\uffef]/u;
const JSX_TEXT = /(?<![=-])>\s*([^<>{}\s][^<>{}]*?)\s*</g;
const COPY_PROP =
  /\b(?:aria-label|placeholder|title|label|retryLabel|description)=(["'])([^"']*[A-Za-z][^"']*)\1/g;

function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
}

function hardcodedCopy(source: string): string[] {
  const code = stripComments(source);
  const found: string[] = [];
  for (const line of code.split('\n')) {
    if (CJK.test(line)) found.push(line.trim());
  }
  for (const match of code.matchAll(JSX_TEXT)) {
    const text = match[1]!;
    if (/[A-Za-z]/.test(text) && !/[=;()]/.test(text)) found.push(text);
  }
  for (const match of code.matchAll(COPY_PROP)) found.push(match[0]);
  return found;
}

describe('setup and access copy goes through i18n', () => {
  it.each(SOURCES)('%s has no hardcoded user-facing copy', (path) => {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    expect(hardcodedCopy(source)).toEqual([]);
  });

  it('flags the hardcoded shapes it guards against', () => {
    expect(hardcodedCopy('<p>你的 AI 搭档</p>')).toContain('你的 AI 搭档');
    expect(hardcodedCopy('<Button>Try again</Button>')).toEqual(['Try again']);
    expect(hardcodedCopy('<ErrorState title="Could not verify" />')).toHaveLength(1);
    expect(hardcodedCopy("<Label>{t('wizard.nav.next')}</Label>")).toEqual([]);
  });
});
