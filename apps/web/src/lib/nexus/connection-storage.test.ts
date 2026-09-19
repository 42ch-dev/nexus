import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createConnectionStorage,
  endpointLabel,
  normalizeEndpointUrl,
  type ConnectionConfig,
} from '@/lib/nexus/connection-storage';
import type { DesktopBridge } from '@/lib/nexus/desktop-bridge';

/** Test seam: a structurally valid version-1 bridge with a plain invoke stub. */
function mockBridge(invoke: (operation: string, payload?: unknown) => Promise<unknown>): void {
  const bridge = {
    version: 1,
    runtime: { localEndpoint: 'http://localhost:8420' },
    invoke,
    onStatusChanged: () => () => {},
  } as unknown as DesktopBridge;
  (window as unknown as { nexusDesktop?: DesktopBridge }).nexusDesktop = bridge;
}

function restoreBridge(): void {
  delete (window as unknown as { nexusDesktop?: unknown }).nexusDesktop;
}

describe('normalizeEndpointUrl', () => {
  it('trims whitespace and trailing slashes', () => {
    expect(normalizeEndpointUrl('  https://example.com:8420/  ')).toBe('https://example.com:8420');
    expect(normalizeEndpointUrl('https://example.com:8420///')).toBe('https://example.com:8420');
  });

  it('returns empty string for empty input', () => {
    expect(normalizeEndpointUrl('')).toBe('');
  });
});

describe('endpointLabel', () => {
  it('returns hostname from a valid URL', () => {
    expect(endpointLabel('https://192.168.1.42:8420')).toBe('192.168.1.42');
  });

  it('returns fallback for invalid URL', () => {
    expect(endpointLabel('not-a-url')).toBe('Remote daemon');
  });

  it('accepts a custom fallback', () => {
    expect(endpointLabel('bad', 'Custom fallback')).toBe('Custom fallback');
  });
});

describe('WebConnectionStorage', () => {
  it('round-trips a config through localStorage', async () => {
    const storage = createConnectionStorage();
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example:8420',
      apiKey: 'secret',
      pinnedFingerprint: 'SHA256:aa:bb:cc',
      label: 'Home server',
      active: true,
    };
    await storage.save(config);
    const loaded = await storage.load();
    expect(loaded).toEqual(config);
  });

  it('returns null when no config is saved', async () => {
    window.localStorage.clear();
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
  });

  it('returns null for malformed JSON', async () => {
    window.localStorage.setItem('nexus-connection-config-v1', '{bad json');
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
  });

  it('clears the saved config', async () => {
    const storage = createConnectionStorage();
    await storage.save({ endpointUrl: 'https://x', apiKey: 'k' });
    await storage.clear();
    expect(await storage.load()).toBeNull();
  });

  it('clears a corrupt localStorage entry and returns null', async () => {
    window.localStorage.setItem(
      'nexus-connection-config-v1',
      JSON.stringify({ endpointUrl: 123, apiKey: null, pinnedFingerprint: false }),
    );
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
    expect(window.localStorage.getItem('nexus-connection-config-v1')).toBeNull();
  });

  it('clears an entry with an invalid pinned fingerprint', async () => {
    window.localStorage.setItem(
      'nexus-connection-config-v1',
      JSON.stringify({ endpointUrl: 'https://x', apiKey: 'k', pinnedFingerprint: 123 }),
    );
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
    expect(window.localStorage.getItem('nexus-connection-config-v1')).toBeNull();
  });

  it('clears an entry missing required fields and returns null', async () => {
    window.localStorage.setItem(
      'nexus-connection-config-v1',
      JSON.stringify({ apiKey: 'k' }),
    );
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
    expect(window.localStorage.getItem('nexus-connection-config-v1')).toBeNull();
  });

  it('clears an active remote entry with a missing API key (never loads keyless remote mode)', async () => {
    // Regression: the desktop redacted shape makes apiKey optional, but the
    // web backend must never load an active remote record without the actual
    // key — that would build an unauthenticated BrowserClient.
    window.localStorage.setItem(
      'nexus-connection-config-v1',
      JSON.stringify({ endpointUrl: 'https://remote.example:8420', active: true, hasApiKey: true }),
    );
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
    expect(window.localStorage.getItem('nexus-connection-config-v1')).toBeNull();
  });
});

describe('DesktopConnectionStorage (Electron redacted store)', () => {
  afterEach(() => {
    restoreBridge();
  });

  it('loads the public projection through the bridge (never the API key — D-18)', async () => {
    const invoke = vi.fn(() =>
      Promise.resolve({
        endpointUrl: 'https://t',
        label: 'Home',
        active: true,
        pinnedFingerprint: 'SHA256:aa',
        hasApiKey: true,
      }),
    );
    mockBridge(invoke);

    const storage = createConnectionStorage();
    const loaded = await storage.load();
    expect(invoke).toHaveBeenCalledWith('get_connection_config');
    expect(loaded).toEqual({
      endpointUrl: 'https://t',
      label: 'Home',
      active: true,
      pinnedFingerprint: 'SHA256:aa',
      hasApiKey: true,
      apiKey: '',
    });
  });

  it('returns null when no desktop config is saved', async () => {
    const invoke = vi.fn(() => Promise.resolve(null));
    mockBridge(invoke);
    const storage = createConnectionStorage();
    expect(await storage.load()).toBeNull();
  });

  it('converts an omitted key to a keep credential update', async () => {
    const invoke = vi.fn((operation: string, payload?: unknown) =>
      Promise.resolve(
        operation === 'set_connection_config' ? (payload as { config: unknown }).config : null,
      ),
    );
    mockBridge(invoke);

    const storage = createConnectionStorage();
    await storage.save({ endpointUrl: 'https://t', hasApiKey: true, active: true });
    expect(invoke).toHaveBeenCalledWith('set_connection_config', {
      config: {
        endpointUrl: 'https://t',
        label: undefined,
        active: true,
        pinnedFingerprint: undefined,
        hasApiKey: true,
      },
      credential: { action: 'keep' },
    });
  });

  it('converts a present key to a replace credential update', async () => {
    const invoke = vi.fn((operation: string, payload?: unknown) =>
      Promise.resolve(
        operation === 'set_connection_config' ? (payload as { config: unknown }).config : null,
      ),
    );
    mockBridge(invoke);

    const storage = createConnectionStorage();
    await storage.save({ endpointUrl: 'https://t', apiKey: 'fresh-key', active: true });
    expect(invoke).toHaveBeenCalledWith('set_connection_config', {
      config: {
        endpointUrl: 'https://t',
        label: undefined,
        active: true,
        pinnedFingerprint: undefined,
        hasApiKey: true,
      },
      credential: { action: 'replace', value: 'fresh-key' },
    });
  });

  it('clears via delete_connection_config', async () => {
    const invoke = vi.fn(() => Promise.resolve(null));
    mockBridge(invoke);
    const storage = createConnectionStorage();
    await storage.clear();
    expect(invoke).toHaveBeenCalledWith('delete_connection_config');
  });
});
