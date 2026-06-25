/**
 * Desktop capabilities tests (compass §5 #1/#8; desktop-shell.md §5/§9).
 *
 * Pins the contract between the SPA and the Tauri custom commands:
 *   - `openWith` / `revealInFinder` call the `open_with` / `reveal_in_finder`
 *     commands with the path payload.
 *   - A Rust `PathGuardError` (`{ code: 'path_outside_workspace', message }`) is
 *     unwrapped into the structured `DesktopCapabilityError` shape so the toast
 *     layer reads it uniformly.
 *   - `getDaemonStatus` probes the shared HTTP health endpoint (P0 passive mode;
 *     P1 swaps in sidecar control).
 *   - When `window.__TAURI__` is absent (browser build, or invoked outside the
 *     shell), invoking a native method fails fast with `invoke_failed`.
 */
import { describe, expect, it, vi } from 'vitest';

import { TauriDesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

/** Install a fake Tauri global capturing the command + args. */
function mockTauri(invoke: (cmd: string, args: Record<string, unknown>) => unknown) {
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: { invoke: vi.fn(invoke) },
  };
  return (window as unknown as { __TAURI__: { core: { invoke: ReturnType<typeof vi.fn> } } })
    .__TAURI__.core.invoke;
}

function restoreTauri() {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
}

function makeCapabilities() {
  return new TauriDesktopCapabilities({
    daemonHealth: vi.fn().mockResolvedValue({ status: 'ok', version: '1.0.0' }),
    port: 8420,
  });
}

describe('TauriDesktopCapabilities', () => {
  it('openWith invokes the open_with command with the path payload', async () => {
    const invoke = mockTauri(() => Promise.resolve(undefined));
    const caps = makeCapabilities();
    await caps.openWith('Works/WRK/Stories/ch01.md');
    expect(invoke).toHaveBeenCalledWith('open_with', { path: 'Works/WRK/Stories/ch01.md' });
    restoreTauri();
  });

  it('revealInFinder invokes the reveal_in_finder command with the path payload', async () => {
    const invoke = mockTauri(() => Promise.resolve(undefined));
    const caps = makeCapabilities();
    await caps.revealInFinder('Works/WRK/Stories/ch01.md');
    expect(invoke).toHaveBeenCalledWith('reveal_in_finder', { path: 'Works/WRK/Stories/ch01.md' });
    restoreTauri();
  });

  it('unwraps a Rust path_outside_workspace rejection into the structured error', async () => {
    // Mirrors the Rust PathGuardError serialized shape ({ code, message }).
    mockTauri(() =>
      Promise.reject({ code: 'path_outside_workspace', message: 'Path not opened. The file is outside the active workspace.' }),
    );
    const caps = makeCapabilities();
    await expect(caps.openWith('/etc/passwd')).rejects.toMatchObject({
      code: 'path_outside_workspace',
      message: 'Path not opened. The file is outside the active workspace.',
    });
    restoreTauri();
  });

  it('collapses a non-envelope invoke failure into invoke_failed', async () => {
    mockTauri(() => Promise.reject('string error'));
    const caps = makeCapabilities();
    await expect(caps.revealInFinder('x')).rejects.toMatchObject({ code: 'invoke_failed' });
    restoreTauri();
  });

  it('getDaemonStatus reports running when the health probe is ok (P0 passive mode)', async () => {
    const caps = makeCapabilities();
    const status = await caps.getDaemonStatus();
    expect(status).toMatchObject({ state: 'running', version: '1.0.0', port: 8420 });
  });

  it('getDaemonStatus reports stopped when the health probe fails', async () => {
    const caps = new TauriDesktopCapabilities({
      daemonHealth: vi.fn().mockRejectedValue(new Error('transport')),
      port: 8420,
    });
    const status = await caps.getDaemonStatus();
    expect(status.state).toBe('stopped');
    expect(status.detail).toMatch(/Restart the daemon/i);
  });

  it('startDaemon/stopDaemon surface the V1.67 sidecar-not-wired copy (P0 stub)', async () => {
    const caps = makeCapabilities();
    await expect(caps.startDaemon()).rejects.toMatchObject({ code: 'invoke_failed' });
    await expect(caps.stopDaemon()).rejects.toMatchObject({ code: 'invoke_failed' });
  });

  it('fails fast when the Tauri global is absent (browser build defensive path)', async () => {
    restoreTauri(); // ensure no __TAURI__
    const caps = makeCapabilities();
    await expect(caps.openWith('x')).rejects.toMatchObject({ code: 'invoke_failed' });
  });
});
