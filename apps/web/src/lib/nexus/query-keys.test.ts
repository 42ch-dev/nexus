import { describe, expect, it } from 'vitest';

import { queryKeys } from './query-keys';

describe('queryKeys', () => {
  describe('characters', () => {
    const charA = 'char-a';
    const charB = 'char-b';
    const query = { cursor: 'c1' };

    it('builds list/detail/bindings keys with the character id before any query object', () => {
      expect(queryKeys.characters.list(query)).toEqual(['characters', 'list', query]);
      expect(queryKeys.characters.detail(charA)).toEqual(['characters', 'detail', charA]);
      expect(queryKeys.characters.bindings.list(charA, query)).toEqual([
        'characters',
        'bindings',
        'list',
        charA,
        query,
      ]);
      expect(queryKeys.characters.bindings.detail(charA, 'b1')).toEqual([
        'characters',
        'bindings',
        'detail',
        charA,
        'b1',
      ]);
    });

    it('produces different keys for different character ids on every family', () => {
      const families: Array<[string, (id: string) => readonly unknown[]]> = [
        ['detail', (id) => queryKeys.characters.detail(id)],
        ['bindings.list', (id) => queryKeys.characters.bindings.list(id, query)],
        ['memory.pendingList', (id) => queryKeys.characters.memory.pendingList(id, query)],
        ['memory.count', (id) => queryKeys.characters.memory.count(id, {})],
        ['memory.fragments', (id) => queryKeys.characters.memory.fragments(id, query)],
        ['soul.narrative', (id) => queryKeys.characters.soul.narrative(id)],
        ['tom.list', (id) => queryKeys.characters.tom.list(id, query)],
        ['knowledge.list', (id) => queryKeys.characters.knowledge.list(id, query)],
        ['knowledge.detail', (id) => queryKeys.characters.knowledge.detail(id, 'e1')],
      ];

      for (const [name, build] of families) {
        const keyA = build(charA);
        const keyB = build(charB);
        expect(keyA, name).not.toEqual(keyB);
        // Fails loudly if the character id is ever dropped from the key.
        expect(keyA, name).toContain(charA);
      }
    });

    it('keeps character memory keys disjoint from creator memory keys', () => {
      expect(queryKeys.characters.memory.pendingList(charA, query)).not.toEqual(
        queryKeys.memory.pendingList(charA, query),
      );
      expect(queryKeys.characters.memory.fragments(charA, query)).not.toEqual(
        queryKeys.memory.fragments(charA, query),
      );
      expect(queryKeys.characters.soul.narrative(charA)).not.toEqual(
        queryKeys.memory.soulNarrative(charA),
      );
    });
  });

  describe('agentHost.scan', () => {
    it('includes the filter and registry_refresh flag in the key', () => {
      expect(queryKeys.agentHost.scan({ filter: 'all', registry_refresh: true })).toEqual([
        'agentHost',
        'scan',
        'all',
        true,
      ]);
    });

    it('produces different keys for different filter values', () => {
      const allKey = queryKeys.agentHost.scan({ filter: 'all' });
      const installedKey = queryKeys.agentHost.scan({ filter: 'installed' });

      expect(allKey).toEqual(['agentHost', 'scan', 'all', false]);
      expect(installedKey).toEqual(['agentHost', 'scan', 'installed', false]);
      expect(allKey).not.toEqual(installedKey);
    });

    it('produces different keys for different registry_refresh values', () => {
      const noRefreshKey = queryKeys.agentHost.scan({ filter: 'installed', registry_refresh: false });
      const refreshKey = queryKeys.agentHost.scan({ filter: 'installed', registry_refresh: true });

      expect(noRefreshKey).toEqual(['agentHost', 'scan', 'installed', false]);
      expect(refreshKey).toEqual(['agentHost', 'scan', 'installed', true]);
      expect(noRefreshKey).not.toEqual(refreshKey);
    });

    it('uses defaults when no request is provided', () => {
      expect(queryKeys.agentHost.scan()).toEqual(['agentHost', 'scan', 'all', false]);
    });
  });

  describe('strategies', () => {
    it('scopes the approval list by strategy id, apart from the applied-status key', () => {
      expect(queryKeys.strategies.approvals('user/demo')).toEqual([
        'strategies',
        'approvals',
        'user/demo',
      ]);
      expect(queryKeys.strategies.approvals('user/demo')).not.toEqual(
        queryKeys.strategies.applied('user/demo'),
      );
      expect(queryKeys.strategies.approvals('user/other')).not.toEqual(
        queryKeys.strategies.approvals('user/demo'),
      );
    });
  });
});
