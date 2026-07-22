/**
 * `TauriClient` / desktop port resolution tests (compass §5 #3 LOCKED).
 *
 * Resolution order: explicit `port` argument → `NEXUS_DAEMON_PORT` (valid u16)
 * → `8420`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  resolveDesktopBaseUrl,
  resolveDesktopPort,
  TauriClient,
} from '@/lib/nexus/tauri-client';

/** Pin `window.location` for origin-sensitive desktop base URL resolution. */
function stubLocation(partial: { hostname?: string; port?: string; protocol?: string }) {
  const current = window.location;
  Object.defineProperty(window, 'location', {
    configurable: true,
    value: {
      ...current,
      hostname: partial.hostname ?? current.hostname,
      port: partial.port ?? current.port,
      protocol: partial.protocol ?? current.protocol,
    },
  });
}

describe('resolveDesktopPort', () => {
  beforeEach(() => {
    delete (window as unknown as { __NEXUS_DAEMON_PORT__?: number }).__NEXUS_DAEMON_PORT__;
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    delete (window as unknown as { __NEXUS_DAEMON_PORT__?: number }).__NEXUS_DAEMON_PORT__;
  });

  it('returns the explicit port argument when valid', () => {
    expect(resolveDesktopPort(9000)).toBe(9000);
    expect(resolveDesktopPort('9000')).toBe(9000);
  });

  it('ignores invalid explicit ports and falls through', () => {
    vi.stubEnv('NEXUS_DAEMON_PORT', '8888');
    expect(resolveDesktopPort(70000)).toBe(8888);
    expect(resolveDesktopPort('abc')).toBe(8888);
  });

  it('prefers the injected Tauri global over env var', () => {
    vi.stubEnv('NEXUS_DAEMON_PORT', '8888');
    (window as unknown as { __NEXUS_DAEMON_PORT__: number }).__NEXUS_DAEMON_PORT__ = 7777;
    expect(resolveDesktopPort()).toBe(7777);
  });

  it('uses NEXUS_DAEMON_PORT when no explicit port or injected global is given', () => {
    vi.stubEnv('NEXUS_DAEMON_PORT', '8888');
    expect(resolveDesktopPort()).toBe(8888);
  });

  it('ignores invalid NEXUS_DAEMON_PORT and falls back to 8420', () => {
    vi.stubEnv('NEXUS_DAEMON_PORT', 'not-a-port');
    expect(resolveDesktopPort()).toBe(8420);
    vi.stubEnv('NEXUS_DAEMON_PORT', '70000');
    expect(resolveDesktopPort()).toBe(8420);
  });

  it('defaults to 8420 when no override is present', () => {
    expect(resolveDesktopPort()).toBe(8420);
  });
});

describe('resolveDesktopBaseUrl', () => {
  const originalLocation = window.location;

  afterEach(() => {
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: originalLocation,
    });
  });

  it('uses localhost loopback outside the Vite origin', () => {
    stubLocation({ hostname: 'localhost', port: '', protocol: 'http:' });
    expect(resolveDesktopBaseUrl(8420)).toBe('http://localhost:8420');
  });

  it('uses same-origin (empty baseUrl) on the Vite :5173 origin', () => {
    stubLocation({ hostname: 'localhost', port: '5173', protocol: 'http:' });
    expect(resolveDesktopBaseUrl(8420)).toBe('');
  });
});

describe('TauriClient', () => {
  const originalLocation = window.location;

  beforeEach(() => {
    delete (window as unknown as { __NEXUS_DAEMON_PORT__?: number }).__NEXUS_DAEMON_PORT__;
    // Packaged / non-Vite origin — absolute localhost loopback.
    stubLocation({ hostname: 'localhost', port: '', protocol: 'http:' });
  });

  afterEach(() => {
    delete (window as unknown as { __NEXUS_DAEMON_PORT__?: number }).__NEXUS_DAEMON_PORT__;
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: originalLocation,
    });
  });

  it('fixes the base URL to the resolved desktop loopback port', () => {
    const client = new TauriClient({ port: 9001 });
    expect(client.port).toBe(9001);
  });

  it('uses the injected Tauri global port when no explicit port is given', async () => {
    (window as unknown as { __NEXUS_DAEMON_PORT__: number }).__NEXUS_DAEMON_PORT__ = 9420;
    const fetchImpl = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 'ok', version: '1.0.0' }), { status: 200 }),
    );
    const client = new TauriClient({ fetchImpl });
    expect(client.port).toBe(9420);
    await client.health();
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://localhost:9420/v1/daemon/runtime/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('accepts an injected fetch implementation for tests', async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 'ok', version: '1.0.0' }), { status: 200 }),
    );
    const client = new TauriClient({ port: 8420, fetchImpl });
    const health = await client.health();
    expect(health).toMatchObject({ status: 'ok', version: '1.0.0' });
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://localhost:8420/v1/daemon/runtime/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('uses relative daemon paths when served from Vite :5173', async () => {
    stubLocation({ hostname: 'localhost', port: '5173', protocol: 'http:' });
    const fetchImpl = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 'ok', version: '1.0.0' }), { status: 200 }),
    );
    const client = new TauriClient({ port: 8420, fetchImpl });
    expect(client.port).toBe(8420);
    await client.health();
    expect(fetchImpl).toHaveBeenCalledWith(
      '/v1/daemon/runtime/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});
