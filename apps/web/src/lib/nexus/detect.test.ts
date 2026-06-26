/**
 * Capability detection tests (compass §5 #7 LOCKED).
 *
 * `isDesktopBuild()` is the single factory signal — runtime, not build-time,
 * because `apps/web` is one bundle served in both the browser tab and the Tauri
 * webview. It must read `true` only inside the Tauri webview (or when the
 * `NEXUS_DESKTOP` flag is explicit), and `false` in a plain browser/jsdom.
 */
import { afterEach, describe, expect, it } from 'vitest';

import { isDesktopBuild } from '@/lib/nexus/detect';

describe('isDesktopBuild (capability detection — §5 #7)', () => {
  afterEach(() => {
    // jsdom defaults: no Tauri markers, no flag. Restore between cases.
    delete (window as Partial<Window>).__TAURI_INTERNALS__;
    delete (window as Partial<Window> & { NEXUS_DESKTOP?: boolean }).NEXUS_DESKTOP;
  });

  it('returns false in a plain browser/jsdom environment (no Tauri marker)', () => {
    expect(isDesktopBuild()).toBe(false);
  });

  it('returns true when the Tauri runtime marker is present (webview)', () => {
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    expect(isDesktopBuild()).toBe(true);
  });

  it('returns true when the explicit NEXUS_DESKTOP flag is set (override)', () => {
    (window as unknown as { NEXUS_DESKTOP: boolean }).NEXUS_DESKTOP = true;
    expect(isDesktopBuild()).toBe(true);
  });

  it('does not flip on unrelated global properties (defends against false positives)', () => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = {}; // without __TAURI_INTERNALS__
    expect(isDesktopBuild()).toBe(false);
  });
});
