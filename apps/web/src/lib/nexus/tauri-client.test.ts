/**
 * `TauriClient` / desktop port resolution tests (compass §5 #3 LOCKED).
 *
 * Resolution order: explicit `port` argument → `NEXUS_DAEMON_PORT` (valid u16)
 * → `8420`.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import { resolveDesktopPort, TauriClient } from '@/lib/nexus/tauri-client';

describe('resolveDesktopPort', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
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

  it('uses NEXUS_DAEMON_PORT when no explicit port is given', () => {
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

describe('TauriClient', () => {
  it('fixes the base URL to the resolved desktop loopback port', () => {
    const client = new TauriClient({ port: 9001 });
    expect(client.port).toBe(9001);
  });

  it('accepts an injected fetch implementation for tests', async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 'ok', version: '1.0.0' }), { status: 200 }),
    );
    const client = new TauriClient({ port: 8420, fetchImpl });
    const health = await client.health();
    expect(health).toMatchObject({ status: 'ok', version: '1.0.0' });
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://127.0.0.1:8420/v1/local/runtime/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});
