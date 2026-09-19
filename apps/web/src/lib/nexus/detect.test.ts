/**
 * `isDesktopBuild()` detection tests (compass §5 #7; parity row 27).
 *
 * `isDesktopBuild()` is the single factory signal — runtime, not build-time,
 * because `apps/web` is one bundle served in both the browser tab and the
 * Electron shell. It must read `true` only with a valid
 * `window.nexusDesktop.version === 1` bridge (or the explicit `NEXUS_DESKTOP`
 * flag), and `false` in a plain browser/jsdom. No Tauri runtime marker remains.
 */

import { afterEach, describe, expect, it } from 'vitest';

import { isDesktopBuild } from '@/lib/nexus/detect';
import type { DesktopBridge } from '@/lib/nexus/desktop-bridge';

function mockBridge(): void {
  const bridge: DesktopBridge = {
    version: 1,
    runtime: { localEndpoint: 'http://localhost:8420' },
    // The mock is only used for synchronous detection checks — invoke never runs.
    invoke: () => Promise.reject(new Error('not implemented in detection mock')),
    onStatusChanged: () => () => {},
  };
  (window as unknown as { nexusDesktop?: DesktopBridge }).nexusDesktop = bridge;
}

describe('isDesktopBuild (capability detection — §5 #7)', () => {
  afterEach(() => {
    // jsdom defaults: no bridge, no flag. Restore between cases.
    delete (window as Partial<Window>).NEXUS_DESKTOP;
    delete (window as unknown as { nexusDesktop?: unknown }).nexusDesktop;
  });

  it('returns false in a plain browser/jsdom environment (no bridge)', () => {
    expect(isDesktopBuild()).toBe(false);
  });

  it('returns true when the version-1 preload bridge is present (Electron shell)', () => {
    mockBridge();
    expect(isDesktopBuild()).toBe(true);
  });

  it('returns false for a bridge with a foreign version', () => {
    (window as unknown as { nexusDesktop?: unknown }).nexusDesktop = {
      version: 2,
      runtime: { localEndpoint: 'http://localhost:8420' },
    };
    expect(isDesktopBuild()).toBe(false);
  });

  it('returns true when the explicit NEXUS_DESKTOP flag is set (override)', () => {
    (window as unknown as { NEXUS_DESKTOP: boolean }).NEXUS_DESKTOP = true;
    expect(isDesktopBuild()).toBe(true);
  });

  it('does not flip on unrelated global properties (defends against false positives)', () => {
    (window as unknown as { __TAURI__?: unknown }).__TAURI__ = {};
    expect(isDesktopBuild()).toBe(false);
  });
});
