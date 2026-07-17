/**
 * Unit tests for {@link launchCommandMatches} — the Verify Agent probe matcher
 * (V1.108 FB-UI-008 PR-review fix).
 *
 * The matcher is a pure function; the hook wrapper is exercised through the
 * setup-step-agent component tests. These cases pin the normalization rules
 * (trim, exact, basename-of-first-token case-insensitive) so the leniency
 * does not silently regress to substring false positives.
 */
import { describe, expect, it } from 'vitest';

import { filterVisibleSessions, launchCommandMatches } from './queries';

describe('launchCommandMatches', () => {
  it('matches exact equality (preserves original FB-UI-008 behavior)', () => {
    expect(launchCommandMatches('codex', 'codex')).toBe(true);
    expect(
      launchCommandMatches('/usr/local/bin/my-agent', '/usr/local/bin/my-agent'),
    ).toBe(true);
  });

  it('returns false when either side is empty after trim', () => {
    expect(launchCommandMatches('', 'codex')).toBe(false);
    expect(launchCommandMatches('codex', '')).toBe(false);
    expect(launchCommandMatches('codex', undefined)).toBe(false);
    expect(launchCommandMatches('   ', 'codex')).toBe(false);
  });

  it('trims surrounding whitespace before comparing', () => {
    expect(launchCommandMatches('  codex  ', 'codex')).toBe(true);
    expect(launchCommandMatches('codex', '  codex\n')).toBe(true);
  });

  it('matches a full path against the short form via basename', () => {
    expect(launchCommandMatches('/usr/local/bin/my-agent', 'my-agent')).toBe(true);
    expect(launchCommandMatches('my-agent', '/usr/local/bin/my-agent')).toBe(true);
  });

  it('matches case-insensitively on the basename', () => {
    expect(launchCommandMatches('/usr/local/bin/My-Agent', 'my-agent')).toBe(true);
    expect(launchCommandMatches('CODEX', 'codex')).toBe(true);
  });

  it('ignores trailing arguments by comparing only the first token', () => {
    expect(launchCommandMatches('/usr/local/bin/my-agent --foo', 'my-agent')).toBe(true);
    expect(launchCommandMatches('my-agent --foo --bar', 'my-agent --baz')).toBe(true);
  });

  it('does NOT admit substring false positives', () => {
    // `code` must not match `codex` even though one contains the other.
    expect(launchCommandMatches('code', 'codex')).toBe(false);
    expect(launchCommandMatches('codex', 'code')).toBe(false);
    // Different binaries sharing a prefix must not match.
    expect(launchCommandMatches('my-agent-cli', 'my-agent')).toBe(false);
  });
});

// AD-P0-2c (V1.120 P2 / F3): defensive client filter behind `useSessions`.
describe('filterVisibleSessions', () => {
  const session = (preset_id: string) => ({ preset_id });

  it('drops _system.* rows and keeps author rows', () => {
    const items = [session('_system.maintenance'), session('novel-writing'), session('_system.health')];
    expect(filterVisibleSessions(items)).toEqual([session('novel-writing')]);
  });

  it('returns an empty list when every row is _system.* (idle daemon)', () => {
    expect(filterVisibleSessions([session('_system.maintenance')])).toEqual([]);
  });

  it('keeps everything when no _system.* rows exist', () => {
    const items = [session('novel-writing'), session('essay')];
    expect(filterVisibleSessions(items)).toEqual(items);
  });

  it('only matches the exact _system. prefix (system. without underscore stays)', () => {
    const items = [session('system.user-preset'), session('_systematic')];
    expect(filterVisibleSessions(items)).toEqual(items);
  });
});
