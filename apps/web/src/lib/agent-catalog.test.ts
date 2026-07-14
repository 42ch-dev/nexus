import { describe, expect, it } from 'vitest';
import {
  resolveInstallUrl,
  isHiddenFromDefault,
  resolveAgentKey,
  resolveCatalogItem,
  defaultGridEntries,
  moreAgentsEntries,
  prioritizeInstalled,
} from '@/lib/agent-catalog';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

function makeAgent(overrides: Partial<AgentScanEntry> = {}): AgentScanEntry {
  return {
    name: 'test-agent',
    installed: false,
    ...overrides,
  };
}

describe('resolveInstallUrl', () => {
  it('returns URL for whitelisted keys', () => {
    expect(resolveInstallUrl('claude-native')).toBe('https://claude.ai/code');
    expect(resolveInstallUrl('codex-native')).toBe('https://openai.com/codex/');
  });

  it('returns null for non-whitelisted keys', () => {
    expect(resolveInstallUrl('unknown-agent')).toBeNull();
    expect(resolveInstallUrl('')).toBeNull();
  });
});

describe('isHiddenFromDefault', () => {
  it('returns true for ACP wrappers', () => {
    expect(isHiddenFromDefault('claude-acp')).toBe(true);
    expect(isHiddenFromDefault('codex-acp')).toBe(true);
  });

  it('returns false for native agents', () => {
    expect(isHiddenFromDefault('claude-native')).toBe(false);
    expect(isHiddenFromDefault('codex-native')).toBe(false);
  });

  it('returns false for unknown keys', () => {
    expect(isHiddenFromDefault('unknown')).toBe(false);
  });
});

describe('resolveAgentKey', () => {
  it('prefers registry_agent_id when set', () => {
    expect(
      resolveAgentKey(makeAgent({ registry_agent_id: 'claude-acp', launch_command: 'claude' })),
    ).toBe('claude-acp');
  });

  it('maps claude launch_command to claude-native', () => {
    expect(
      resolveAgentKey(makeAgent({ registry_agent_id: null, launch_command: 'claude' })),
    ).toBe('claude-native');
  });

  it('maps codex launch_command to codex-native', () => {
    expect(
      resolveAgentKey(makeAgent({ registry_agent_id: null, launch_command: 'codex' })),
    ).toBe('codex-native');
  });

  it('maps full-path launch_command via basename (production daemon shape) — QC1 C1', () => {
    // The daemon PATH-scan emits the full resolved binary path.
    expect(
      resolveAgentKey(
        makeAgent({
          registry_agent_id: null,
          launch_command: '/usr/local/bin/claude',
          name: 'claude (native CLI)',
        }),
      ),
    ).toBe('claude-native');

    expect(
      resolveAgentKey(
        makeAgent({
          registry_agent_id: null,
          launch_command: '/opt/homebrew/bin/codex',
          name: 'codex (native CLI)',
        }),
      ),
    ).toBe('codex-native');
  });

  it('maps launch_command with trailing args via basename', () => {
    expect(
      resolveAgentKey(
        makeAgent({ registry_agent_id: null, launch_command: '/usr/local/bin/claude --foo' }),
      ),
    ).toBe('claude-native');
  });

  it('falls back to name when no registry id or native signal', () => {
    expect(
      resolveAgentKey(makeAgent({ registry_agent_id: null, launch_command: null, name: 'custom-agent' })),
    ).toBe('custom-agent');
  });
});

describe('resolveCatalogItem', () => {
  it('merges displayName from overrides', () => {
    const item = resolveCatalogItem(
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude (Native)', installed: true }),
    );
    expect(item.displayName).toBe('Claude');
    expect(item.id).toBe('claude-native');
  });

  it('falls back to scan name when no override', () => {
    const item = resolveCatalogItem(
      makeAgent({ name: 'custom-agent', installed: true }),
    );
    expect(item.displayName).toBe('custom-agent');
  });

  it('assigns whitelist install URL for known keys', () => {
    const item = resolveCatalogItem(
      makeAgent({ registry_agent_id: 'opencode', name: 'OpenCode', installed: false }),
    );
    expect(item.installUrl).toBe('https://opencode.ai/download');
  });

  it('returns null installUrl for non-whitelisted keys', () => {
    const item = resolveCatalogItem(
      makeAgent({ name: 'unknown-agent', installed: false }),
    );
    expect(item.installUrl).toBeNull();
  });

  it('uses whitelist installUrl when override has no installUrl (PM minor fix)', () => {
    // claude-native override has displayName+priority but no installUrl.
    // The fix clarified: override.installUrl must be a whitelisted VALUE to
    // render; with no override.installUrl, the key's whitelist URL applies.
    const item = resolveCatalogItem(
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude' }),
    );
    expect(item.installUrl).toBe('https://claude.ai/code');
  });

  it('sets hiddenFromDefault for ACP wrappers', () => {
    const item = resolveCatalogItem(
      makeAgent({ registry_agent_id: 'claude-acp', name: 'Claude', installed: true }),
    );
    expect(item.hiddenFromDefault).toBe(true);
  });

  it('sets priority from overrides', () => {
    const item = resolveCatalogItem(
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude' }),
    );
    expect(item.priority).toBe(0);
  });

  it('prefers scan icon_url over override', () => {
    const item = resolveCatalogItem(
      makeAgent({
        registry_agent_id: 'claude-native',
        name: 'Claude',
        icon_url: 'https://example.com/icon.svg',
      }),
    );
    expect(item.iconUrl).toBe('https://example.com/icon.svg');
  });
});

describe('defaultGridEntries', () => {
  it('includes native agents and excludes hidden ACP wrappers', () => {
    const entries = defaultGridEntries([
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude', installed: true }),
      makeAgent({ registry_agent_id: 'claude-acp', name: 'Claude ACP', installed: true }),
      makeAgent({ registry_agent_id: 'codex-native', name: 'Codex', installed: true }),
    ]);
    const ids = entries.map((e) => e.id);
    expect(ids).toContain('claude-native');
    expect(ids).toContain('codex-native');
    expect(ids).not.toContain('claude-acp');
  });

  it('sorts by priority ascending', () => {
    const entries = defaultGridEntries([
      makeAgent({ registry_agent_id: 'codex-native', name: 'Codex', installed: true }),
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude', installed: true }),
    ]);
    expect(entries[0]!.id).toBe('claude-native');
    expect(entries[1]!.id).toBe('codex-native');
  });

  it('includes installed agents without priority after priority agents', () => {
    const entries = defaultGridEntries([
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude', installed: true }),
      makeAgent({ name: 'installed-custom', installed: true }),
    ]);
    expect(entries[0]!.id).toBe('claude-native');
    expect(entries[1]!.id).toBe('installed-custom');
  });
});

describe('moreAgentsEntries', () => {
  it('excludes default grid items', () => {
    const entries = moreAgentsEntries([
      makeAgent({ registry_agent_id: 'claude-native', name: 'Claude', installed: true }),
      makeAgent({ registry_agent_id: 'claude-acp', name: 'Claude ACP', installed: true }),
      makeAgent({ name: 'other-agent', installed: false }),
    ]);
    const ids = entries.map((e) => e.id);
    expect(ids).not.toContain('claude-native');
    expect(ids).toContain('claude-acp');
    expect(ids).toContain('other-agent');
  });

  it('sorts installed first then alphabetically', () => {
    const entries = moreAgentsEntries([
      makeAgent({ name: 'z-agent', installed: false }),
      makeAgent({ registry_agent_id: 'claude-acp', name: 'Claude ACP', installed: true }),
      makeAgent({ name: 'b-agent', installed: false }),
    ]);
    expect(entries[0]!.id).toBe('claude-acp');
    expect(entries[1]!.id).toBe('b-agent');
    expect(entries[2]!.id).toBe('z-agent');
  });
});

describe('prioritizeInstalled', () => {
  it('sorts installed items first', () => {
    const items = prioritizeInstalled([
      { id: 'b', installed: false, name: 'b', displayName: 'B' },
      { id: 'a', installed: true, name: 'a', displayName: 'A' },
    ] as Parameters<typeof prioritizeInstalled>[0]);
    expect(items[0]!.id).toBe('a');
    expect(items[1]!.id).toBe('b');
  });
});