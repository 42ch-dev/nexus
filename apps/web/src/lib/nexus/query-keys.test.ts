import { describe, expect, it } from 'vitest';

import { queryKeys } from './query-keys';

describe('queryKeys', () => {
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
});
