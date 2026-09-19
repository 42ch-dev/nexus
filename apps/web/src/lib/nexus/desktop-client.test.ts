/**
 * `DesktopClient` / desktop port resolution tests (compass §5 #3 LOCKED;
 * v1.192 P0-T8).
 *
 * Resolution order: explicit `port` argument → `window.nexusDesktop.runtime.
 * localEndpoint` (typed preload bridge, synchronous — parity row 27) →
 * `NEXUS_DAEMON_PORT` (valid u16) → `8420`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DesktopBridge } from '@/lib/nexus/desktop-bridge';
import {
  resolveDesktopBaseUrl,
  resolveDesktopPort,
  DesktopClient,
} from '@/lib/nexus/desktop-client';

/** Install a fake typed preload bridge with the given trusted runtime metadata. */
function mockBridge(localEndpoint: string): void {
  (window as unknown as { nexusDesktop?: DesktopBridge }).nexusDesktop = {
    version: 1,
    runtime: { localEndpoint },
    invoke: vi.fn(),
    onStatusChanged: vi.fn(() => () => {}),
  };
}

function restoreBridge(): void {
  delete (window as unknown as { nexusDesktop?: unknown }).nexusDesktop;
}

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
    restoreBridge();
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    restoreBridge();
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

  it('prefers the bridge localEndpoint (incl. nondefault port) over the env var', () => {
    vi.stubEnv('NEXUS_DAEMON_PORT', '8888');
    mockBridge('http://localhost:7777');
    expect(resolveDesktopPort()).toBe(7777);
  });

  it('falls through a malformed bridge localEndpoint to the env/default chain', () => {
    mockBridge('not-a-url');
    expect(resolveDesktopPort()).toBe(8420);
  });

  it('uses NEXUS_DAEMON_PORT when no explicit port or bridge is given', () => {
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

describe('DesktopClient', () => {
  const originalLocation = window.location;

  beforeEach(() => {
    restoreBridge();
    // Packaged / non-Vite origin — absolute localhost loopback.
    stubLocation({ hostname: 'localhost', port: '', protocol: 'http:' });
  });

  afterEach(() => {
    restoreBridge();
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: originalLocation,
    });
  });

  it('fixes the base URL to the resolved desktop loopback port', () => {
    const client = new DesktopClient({ port: 9001 });
    expect(client.port).toBe(9001);
  });

  it('consumes the bridge localEndpoint synchronously (incl. nondefault port)', async () => {
    mockBridge('http://localhost:9420');
    const fetchImpl = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 'ok', version: '1.0.0' }), { status: 200 }),
    );
    const client = new DesktopClient({ fetchImpl });
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
    const client = new DesktopClient({ port: 8420, fetchImpl });
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
    const client = new DesktopClient({ port: 8420, fetchImpl });
    expect(client.port).toBe(8420);
    await client.health();
    expect(fetchImpl).toHaveBeenCalledWith(
      '/v1/daemon/runtime/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});
