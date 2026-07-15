import { describe, expect, it } from 'vitest';

import {
  lastPathSegment,
  replaceLastPathSegment,
  slugProfileSegment,
} from './workspace-profile-slug';

/**
 * Unit tests for the V1.119 P1 Profile path-segment slug helper.
 *
 * Spec (normative): `.mstar/iterations/v1.119/specs/setup-workspace-profile-path.md`
 * § Slug rule + § Architecture contract → Slug helper. The slug applies to the
 * **path last segment only**; the Profile display name is kept verbatim.
 */
describe('slugProfileSegment', () => {
  it.each([
    ['default', 'default'],
    ['Alice', 'Alice'],
    // CJK is preserved after NFKC (no romanization).
    ['我的空间', '我的空间'],
    // Leading/trailing trim + internal whitespace run → single dash.
    ['  foo  bar  ', 'foo-bar'],
    // All-illegal input collapses to empty → `default`.
    ['///', 'default'],
    // Windows reserved device names get a `-profile` suffix.
    ['CON', 'CON-profile'],
    ['PRN', 'PRN-profile'],
    ['COM1', 'COM1-profile'],
    // Empty display name → `default` (Continue stays valid).
    ['', 'default'],
    // Illegal segment chars (`/`, `:`) are stripped — not replaced.
    ['foo/bar:baz', 'foobarbaz'],
    // Dash-only input collapses to empty → `default`.
    ['---', 'default'],
  ])('slugProfileSegment(%j) → %j', (input, expected) => {
    expect(slugProfileSegment(input)).toBe(expected);
  });

  it('preserves the display name and only slugifies when called explicitly', () => {
    // The helper is one-way; callers keep the raw name and pass it through
    // unchanged. This test documents that `slugProfileSegment` does not mutate
    // its input and that the raw name survives a round-trip.
    const raw = '  foo  bar  ';
    const slug = slugProfileSegment(raw);
    expect(raw).toBe('  foo  bar  ');
    expect(slug).toBe('foo-bar');
  });
});

describe('lastPathSegment', () => {
  it('returns the last segment of a POSIX path', () => {
    expect(lastPathSegment('~/Documents/nexus/default')).toBe('default');
  });

  it('returns the last segment of a Windows path', () => {
    expect(lastPathSegment('C:\\Users\\bibi\\nexus\\default')).toBe('default');
  });

  it('returns the path verbatim when no separator is present', () => {
    expect(lastPathSegment('default')).toBe('default');
  });

  it('returns empty string for an empty path', () => {
    expect(lastPathSegment('')).toBe('');
  });
});

describe('replaceLastPathSegment', () => {
  it('replaces the last POSIX segment, preserving the parent and separator', () => {
    expect(replaceLastPathSegment('~/Documents/nexus/old', 'alice')).toBe('~/Documents/nexus/alice');
  });

  it('replaces the last Windows segment, preserving the parent and separator', () => {
    expect(replaceLastPathSegment('C:\\Users\\bibi\\nexus\\old', 'alice')).toBe(
      'C:\\Users\\bibi\\nexus\\alice',
    );
  });

  it('returns the new segment verbatim when no separator is present', () => {
    expect(replaceLastPathSegment('old', 'alice')).toBe('alice');
  });

  it('returns the new segment for an empty path', () => {
    expect(replaceLastPathSegment('', 'alice')).toBe('alice');
  });

  it('round-trips with slugProfileSegment to sync a name into a path', () => {
    // Mirrors the SetupStepWorkspace onChange path-sync (AC-P1-3).
    const root = '/home/alice/Documents/nexus/default';
    const synced = replaceLastPathSegment(root, slugProfileSegment('Alice'));
    expect(synced).toBe('/home/alice/Documents/nexus/Alice');
  });
});
