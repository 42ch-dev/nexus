/**
 * Desktop capabilities tests (compass §5 #1/#8; desktop-shell.md §5/§9).
 *
 * Pins the contract between the SPA and the Tauri custom commands:
 *   - `openWith` / `revealInFinder` call the `open_with` / `reveal_in_finder`
 *     commands with the path payload.
 *   - A Rust `PathGuardError` (`{ code: 'path_outside_workspace', message }`) is
 *     unwrapped into the structured `DesktopCapabilityError` shape so the toast
 *     layer reads it uniformly.
 *   - `getDaemonStatus` / `startDaemon` / `stopDaemon` invoke the P1 sidecar
 *     lifecycle commands and return/pass through the status payload.
 *   - When `window.__TAURI__` is absent (browser build, or invoked outside the
 *     shell), invoking a native method fails fast with `invoke_failed`.
 */
import { describe, expect, it, vi } from 'vitest';

import { TauriDesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

/** Install a fake Tauri global capturing the command + args. */
function mockTauri(invoke: (cmd: string, args?: Record<string, unknown>) => unknown) {
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: { invoke: vi.fn(invoke) },
  };
  return (window as unknown as { __TAURI__: { core: { invoke: ReturnType<typeof vi.fn> } } })
    .__TAURI__.core.invoke;
}

function restoreTauri() {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
}

describe('TauriDesktopCapabilities', () => {
  it('openWith invokes the open_with command with the path payload', async () => {
    const invoke = mockTauri(() => Promise.resolve(undefined));
    const caps = new TauriDesktopCapabilities();
    await caps.openWith('Works/WRK/Stories/ch01.md');
    expect(invoke).toHaveBeenCalledWith('open_with', { path: 'Works/WRK/Stories/ch01.md' });
    restoreTauri();
  });

  it('revealInFinder invokes the reveal_in_finder command with the path payload', async () => {
    const invoke = mockTauri(() => Promise.resolve(undefined));
    const caps = new TauriDesktopCapabilities();
    await caps.revealInFinder('Works/WRK/Stories/ch01.md');
    expect(invoke).toHaveBeenCalledWith('reveal_in_finder', { path: 'Works/WRK/Stories/ch01.md' });
    restoreTauri();
  });

  it('unwraps a Rust path_outside_workspace rejection into the structured error', async () => {
    // Mirrors the Rust PathGuardError serialized shape ({ code, message }).
    mockTauri(() =>
      Promise.reject({ code: 'path_outside_workspace', message: 'Path not opened. The file is outside the active workspace.' }),
    );
    const caps = new TauriDesktopCapabilities();
    await expect(caps.openWith('/etc/passwd')).rejects.toMatchObject({
      code: 'path_outside_workspace',
      message: 'Path not opened. The file is outside the active workspace.',
    });
    restoreTauri();
  });

  it('collapses a non-envelope invoke failure into invoke_failed', async () => {
    mockTauri(() => Promise.reject('string error'));
    const caps = new TauriDesktopCapabilities();
    await expect(caps.revealInFinder('x')).rejects.toMatchObject({ code: 'invoke_failed' });
    restoreTauri();
  });

  it('getDaemonStatus invokes get_daemon_status and returns the status payload', async () => {
    mockTauri(() => Promise.resolve({ state: 'running', version: '1.0.0', port: 8420 }));
    const caps = new TauriDesktopCapabilities();
    const status = await caps.getDaemonStatus();
    expect(status).toMatchObject({ state: 'running', version: '1.0.0', port: 8420 });
    restoreTauri();
  });

  it('startDaemon invokes start_daemon', async () => {
    const invoke = mockTauri(() => Promise.resolve(undefined));
    const caps = new TauriDesktopCapabilities();
    await caps.startDaemon();
    expect(invoke).toHaveBeenCalledWith('start_daemon', undefined);
    restoreTauri();
  });

  it('stopDaemon invokes stop_daemon', async () => {
    const invoke = mockTauri(() => Promise.resolve(undefined));
    const caps = new TauriDesktopCapabilities();
    await caps.stopDaemon();
    expect(invoke).toHaveBeenCalledWith('stop_daemon', undefined);
    restoreTauri();
  });

  it('fails fast when the Tauri global is absent (browser build defensive path)', async () => {
    restoreTauri(); // ensure no __TAURI__
    const caps = new TauriDesktopCapabilities();
    await expect(caps.openWith('x')).rejects.toMatchObject({ code: 'invoke_failed' });
  });
});
