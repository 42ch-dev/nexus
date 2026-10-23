/**
 * Browser agent-profile snapshot reader (v1220 T2 seam) — the localStorage
 * trust boundary. A polluted/foreign value must never reach callers as a
 * malformed snapshot or throw inside `launchCommand?.trim()`.
 */
import { beforeEach, describe, expect, it } from 'vitest';

import { readBrowserAgentProfile } from '@/api/queries';

const CREATOR_ID = 'creator-a';
const KEY = `nexus-agent-profile:${CREATOR_ID}`;

describe('readBrowserAgentProfile', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('returns the saved snapshot', () => {
    window.localStorage.setItem(
      KEY,
      JSON.stringify({ name: 'claude-code', launchCommand: '/usr/local/bin/claude' }),
    );

    expect(readBrowserAgentProfile(CREATOR_ID)).toEqual({
      name: 'claude-code',
      launchCommand: '/usr/local/bin/claude',
    });
  });

  it('drops a non-string launchCommand instead of exposing a polluted value', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ name: 'claude-code', launchCommand: 42 }));

    expect(readBrowserAgentProfile(CREATOR_ID)).toEqual({
      name: 'claude-code',
      launchCommand: undefined,
    });
  });

  it('rejects a snapshot without a usable name', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ name: 42 }));
    expect(readBrowserAgentProfile(CREATOR_ID)).toBeNull();

    window.localStorage.setItem(KEY, JSON.stringify({ name: '   ' }));
    expect(readBrowserAgentProfile(CREATOR_ID)).toBeNull();

    window.localStorage.setItem(KEY, JSON.stringify(['not', 'an', 'object']));
    expect(readBrowserAgentProfile(CREATOR_ID)).toBeNull();
  });

  it('returns null for malformed JSON and for a missing creator id', () => {
    window.localStorage.setItem(KEY, '{not json');
    expect(readBrowserAgentProfile(CREATOR_ID)).toBeNull();
    expect(readBrowserAgentProfile(null)).toBeNull();
    expect(readBrowserAgentProfile(undefined)).toBeNull();
  });
});
