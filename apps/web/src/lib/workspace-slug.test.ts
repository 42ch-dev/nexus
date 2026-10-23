import { describe, expect, it } from 'vitest';

import { deriveWorkspaceSlug } from '@/lib/workspace-slug';

describe('deriveWorkspaceSlug', () => {
  it('lowercases and collapses non-alphanumeric runs to single dashes', () => {
    expect(deriveWorkspaceSlug('  Novel  ')).toBe('novel');
    expect(deriveWorkspaceSlug('My  Epic: Book-2!')).toBe('my-epic-book-2');
    expect(deriveWorkspaceSlug('A_B')).toBe('a-b');
  });

  it('keeps leading/trailing separators out of the slug', () => {
    // No ASCII alphanumerics at all → the whole (trimmed) name is hashed.
    expect(deriveWorkspaceSlug('--《剑与魔法》--')).toBe('ws-15daca83');
    expect(deriveWorkspaceSlug('  ·  hello  · ')).toBe('hello');
  });

  it('falls back to a stable ASCII hash for pure CJK / symbol names', () => {
    expect(deriveWorkspaceSlug('星际拓荒志')).toBe('ws-748324af');
    expect(deriveWorkspaceSlug('我的小说')).toBe('ws-20029f24');
    expect(deriveWorkspaceSlug('·—·')).toBe('ws-79c1da63');
    // Deterministic for the same name, and the hash covers leading/trailing
    // whitespace only after trimming.
    expect(deriveWorkspaceSlug('  星际拓荒志 ')).toBe('ws-748324af');
  });

  it('is deterministic for an empty name (the submit gate blocks it upstream)', () => {
    expect(deriveWorkspaceSlug('')).toBe('ws-811c9dc5');
    expect(deriveWorkspaceSlug('   ')).toBe('ws-811c9dc5');
  });
});
