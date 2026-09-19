/**
 * Desktop capabilities tests (compass §5 #1/#8; desktop-shell.md §5/§9;
 * v1.192 P0-T8 Electron typed bridge).
 *
 * Pins the contract between the SPA and the main-owned desktop operations
 * (frozen operation union via `window.nexusDesktop.invoke`):
 *   - `openWith` / `revealInFinder` call the `open_with` / `reveal_in_finder`
 *     operations with the path payload.
 *   - A main action error (`{ code: 'path_outside_workspace', message }`) is
 *     unwrapped into the structured `DesktopCapabilityError` shape so the toast
 *     layer reads it uniformly.
 *   - `getDaemonStatus` / `startDaemon` / `stopDaemon` invoke the controller
 *     lifecycle operations and return/pass through the status payload.
 *   - When the bridge is absent (browser build, or invoked outside the
 *     shell), invoking a native method fails fast with `invoke_failed`.
 */
import { describe, expect, it, vi } from 'vitest';

import type { DesktopBridge } from '@/lib/nexus/desktop-bridge';
import { ElectronDesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

/** Install a fake version-1 bridge capturing the operation + payload. */
function mockBridge(
  invoke: (operation: string, payload?: unknown) => Promise<unknown>,
  onStatusChanged?: (listener: (status: unknown) => void) => () => void,
) {
  const invokeMock = vi.fn(invoke);
  const bridge = {
    version: 1,
    runtime: { localEndpoint: 'http://localhost:8420' },
    invoke: invokeMock,
    onStatusChanged: onStatusChanged ?? (() => () => {}),
  } as unknown as DesktopBridge;
  (window as unknown as { nexusDesktop?: DesktopBridge }).nexusDesktop = bridge;
  return { invoke: invokeMock, bridge };
}

function restoreBridge() {
  delete (window as unknown as { nexusDesktop?: unknown }).nexusDesktop;
}

describe('ElectronDesktopCapabilities', () => {
  it('openWith invokes the open_with operation with the path payload', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.openWith('Works/WRK/Stories/ch01.md');
    expect(invoke).toHaveBeenCalledWith('open_with', { path: 'Works/WRK/Stories/ch01.md' });
    restoreBridge();
  });

  it('revealInFinder invokes the reveal_in_finder operation with the path payload', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.revealInFinder('Works/WRK/Stories/ch01.md');
    expect(invoke).toHaveBeenCalledWith('reveal_in_finder', {
      path: 'Works/WRK/Stories/ch01.md',
    });
    restoreBridge();
  });

  it('openExternalUrl invokes the open_external_url operation with the url payload', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.openExternalUrl('https://example.com/install');
    expect(invoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://example.com/install',
    });
    restoreBridge();
  });

  it('openExternalUrl unwraps a structured error into DesktopCapabilityError', async () => {
    mockBridge(() =>
      Promise.reject({ code: 'invoke_failed', message: 'URL scheme not allowed: file' }),
    );
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.openExternalUrl('file:///etc/passwd')).rejects.toMatchObject({
      code: 'invoke_failed',
      message: 'URL scheme not allowed: file',
    });
    restoreBridge();
  });

  it('unwraps a main path_outside_workspace rejection into the structured error', async () => {
    // Mirrors the main action error serialized shape ({ code, message }).
    mockBridge(() =>
      Promise.reject({
        code: 'path_outside_workspace',
        message: 'Path not opened. The file is outside the active workspace.',
      }),
    );
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.openWith('/etc/passwd')).rejects.toMatchObject({
      code: 'path_outside_workspace',
      message: 'Path not opened. The file is outside the active workspace.',
    });
    restoreBridge();
  });

  it('collapses a non-envelope invoke failure into invoke_failed', async () => {
    mockBridge(() => Promise.reject('string error'));
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.revealInFinder('x')).rejects.toMatchObject({ code: 'invoke_failed' });
    restoreBridge();
  });

  it('getDaemonStatus invokes get_daemon_status and returns the status payload', async () => {
    mockBridge(() => Promise.resolve({ state: 'running', version: '1.0.0', port: 8420 }));
    const caps = new ElectronDesktopCapabilities();
    const status = await caps.getDaemonStatus();
    expect(status).toMatchObject({ state: 'running', version: '1.0.0', port: 8420 });
    restoreBridge();
  });

  it('startDaemon invokes start_daemon', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.startDaemon();
    expect(invoke).toHaveBeenCalledWith('start_daemon');
    restoreBridge();
  });

  it('stopDaemon invokes stop_daemon', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.stopDaemon();
    expect(invoke).toHaveBeenCalledWith('stop_daemon');
    restoreBridge();
  });

  it('resetLocalDatabase invokes reset_local_database', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.resetLocalDatabase();
    expect(invoke).toHaveBeenCalledWith('reset_local_database');
    restoreBridge();
  });

  it('pickDirectory invokes pick_directory with the defaultPath payload', async () => {
    const defaultPath = '/Users/example/Documents/nexus/default';
    const { invoke } = mockBridge(() => Promise.resolve(defaultPath));
    const caps = new ElectronDesktopCapabilities();
    const selected = await caps.pickDirectory(defaultPath);
    expect(selected).toBe(defaultPath);
    expect(invoke).toHaveBeenCalledWith('pick_directory', {
      defaultPath,
    });
    restoreBridge();
  });

  it('setAgentProfile invokes set_agent_profile with the profile payload', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.setAgentProfile('claude-code', '/usr/local/bin/claude');
    expect(invoke).toHaveBeenCalledWith('set_agent_profile', {
      name: 'claude-code',
      launchCommand: '/usr/local/bin/claude',
    });
    restoreBridge();
  });

  it('getAgentProfile invokes get_agent_profile and returns the profile payload', async () => {
    const { invoke } = mockBridge(() =>
      Promise.resolve({ name: 'codex', launchCommand: 'codex' }),
    );
    const caps = new ElectronDesktopCapabilities();
    const profile = await caps.getAgentProfile();
    expect(invoke).toHaveBeenCalledWith('get_agent_profile');
    expect(profile).toEqual({ name: 'codex', launchCommand: 'codex' });
    restoreBridge();
  });

  it('getAgentProfile returns null when the operation yields null', async () => {
    mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.getAgentProfile()).resolves.toBeNull();
    restoreBridge();
  });

  it('getAgentProfile returns null on invoke transport failure (preselect path)', async () => {
    mockBridge(() => Promise.reject('string error'));
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.getAgentProfile()).resolves.toBeNull();
    restoreBridge();
  });

  it('switchActiveCreator invokes switch_active_creator and returns the new path', async () => {
    const { invoke } = mockBridge((operation) => {
      if (operation === 'switch_active_creator') {
        return Promise.resolve('/Users/author/Documents/nexus-profile-b');
      }
      return Promise.resolve(null);
    });
    const caps = new ElectronDesktopCapabilities();
    const path = await caps.switchActiveCreator('creator-b');
    expect(invoke).toHaveBeenCalledWith('switch_active_creator', { creatorId: 'creator-b' });
    expect(path).toBe('/Users/author/Documents/nexus-profile-b');
    restoreBridge();
  });

  it('switchActiveCreator unwraps a structured error into DesktopCapabilityError', async () => {
    mockBridge(() =>
      Promise.reject({ code: 'invoke_failed', message: 'failed to switch active creator: config locked' }),
    );
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.switchActiveCreator('creator-b')).rejects.toMatchObject({
      code: 'invoke_failed',
      message: 'failed to switch active creator: config locked',
    });
    restoreBridge();
  });

  it('onDaemonStatusChanged subscribes via the bridge and returns an unsubscribe', async () => {
    const handler = vi.fn();
    const listener = vi.fn();
    mockBridge(() => Promise.resolve(null), (fn) => {
      listener.mockImplementation(fn);
      return () => {};
    });
    const caps = new ElectronDesktopCapabilities();
    const unlisten = await caps.onDaemonStatusChanged(handler);
    expect(typeof unlisten).toBe('function');
    // The bridge delivers bounded status frames directly (no event objects).
    listener({ state: 'running', version: '1.0.0', port: 8420 });
    expect(handler).toHaveBeenCalledWith({ state: 'running', version: '1.0.0', port: 8420 });
    restoreBridge();
  });

  it('fails fast when the bridge is absent (browser build defensive path)', async () => {
    restoreBridge();
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.openWith('x')).rejects.toMatchObject({ code: 'invoke_failed' });
  });

  it('toggleMaximizeWindow invokes toggle_maximize_window', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.toggleMaximizeWindow();
    expect(invoke).toHaveBeenCalledWith('toggle_maximize_window');
    restoreBridge();
  });

  it('getEntrance invokes get_entrance and returns the persisted value', async () => {
    const { invoke } = mockBridge(() => Promise.resolve('developer'));
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.getEntrance()).resolves.toBe('developer');
    expect(invoke).toHaveBeenCalledWith('get_entrance');
    restoreBridge();
  });

  it('getEntrance resolves a stored-but-unparseable value to content-creator (AR-16)', async () => {
    mockBridge(() => Promise.resolve('admin'));
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.getEntrance()).resolves.toBe('content-creator');
    restoreBridge();
  });

  it('getEntrance unwraps an operation error into DesktopCapabilityError', async () => {
    mockBridge(() => Promise.reject({ code: 'invoke_failed', message: 'operation not registered' }));
    const caps = new ElectronDesktopCapabilities();
    await expect(caps.getEntrance()).rejects.toMatchObject({
      code: 'invoke_failed',
      message: 'operation not registered',
    });
    restoreBridge();
  });

  it('setEntrance invokes set_entrance with the value payload', async () => {
    const { invoke } = mockBridge(() => Promise.resolve(null));
    const caps = new ElectronDesktopCapabilities();
    await caps.setEntrance('content-creator');
    expect(invoke).toHaveBeenCalledWith('set_entrance', { value: 'content-creator' });
    restoreBridge();
  });
});
