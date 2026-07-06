import { describe, expect, it, vi } from 'vitest';

import {
  createConnectionStorage,
  endpointLabel,
  normalizeEndpointUrl,
  type ConnectionConfig,
} from '@/lib/nexus/connection-storage';

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
});

describe('DesktopConnectionStorage Tauri delegate', () => {
  it('invokes get_connection_config when __TAURI__ is present', async () => {
    const invoke = vi.fn().mockResolvedValue('{"endpointUrl":"https://t","apiKey":"k"}');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__TAURI__ = { core: { invoke } };

    const storage = createConnectionStorage();
    const loaded = await storage.load();
    expect(invoke).toHaveBeenCalledWith('get_connection_config', undefined);
    expect(loaded).toEqual({ endpointUrl: 'https://t', apiKey: 'k' });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).__TAURI__;
  });
});
