/**
 * Entrance provider resolution semantics (V1.170 P1 — AR-16 / AR-20).
 *
 * Precedence: URL override > stored > default; invalid values ignored; no
 * write on unset; provider never rewrites the URL. The Tauri IPC pair is a
 * runtime-detected seam (T2 wires `get_entrance`/`set_entrance` into
 * `DesktopCapabilities`) — the desktop cases below exercise it now.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, waitFor } from '@testing-library/react';
import { useLocation } from 'react-router';

import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import {
  EntranceProvider,
  ENTRANCE_STORAGE_KEY,
  useEntrance,
} from '@/lib/entrance-context';
import type { EntranceId } from '@/components/layout/entrance-registry';

interface ProbeValue {
  entrance: EntranceId;
  isLoading: boolean;
  isFirstRun: boolean;
}

/** Renders the current context value + a setEntrance('developer') affordance. */
function EntranceProbe({ onChange }: { onChange: (value: ProbeValue) => void }) {
  const { entrance, isLoading, isFirstRun, setEntrance } = useEntrance();
  onChange({ entrance, isLoading, isFirstRun });
  return (
    <button
      type="button"
      data-testid="set-developer"
      onClick={() => void setEntrance('developer')}
    />
  );
}

function renderProbe(
  initialRouterEntries: string[],
  options: { desktop?: DesktopCapabilities | null } = {},
) {
  let latest: ProbeValue | null = null;
  const view = renderInApp(
    <EntranceProvider>
      <EntranceProbe onChange={(value) => (latest = value)} />
    </EntranceProvider>,
    { initialRouterEntries, ...options },
  );
  return { get: () => latest, ...view };
}

function ipcDesktop(overrides: {
  getEntrance?: () => Promise<EntranceId>;
  setEntrance?: (value: EntranceId) => Promise<void>;
} = {}) {
  // renderInApp always mounts SetupCompletedProvider, which reads
  // `getSetupCompleted` on mount when a desktop is injected.
  return {
    getSetupCompleted: async () => true,
    setSetupCompleted: async () => undefined,
    getEntrance: overrides.getEntrance ?? (async () => 'content-creator'),
    setEntrance: overrides.setEntrance ?? (async () => undefined),
  } as unknown as DesktopCapabilities;
}

describe('EntranceProvider resolution (AR-16/AR-20)', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it('resolves to content-creator when nothing is stored and no override is present (no write)', () => {
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
    const probe = renderProbe(['/']);
    expect(probe.get()).toEqual({
      entrance: 'content-creator',
      isLoading: false,
      isFirstRun: true,
    });
    expect(setItemSpy).not.toHaveBeenCalledWith(ENTRANCE_STORAGE_KEY, expect.anything());
  });

  it('reads a stored value (browser `nexus-entrance`)', () => {
    window.localStorage.setItem(ENTRANCE_STORAGE_KEY, 'developer');
    const probe = renderProbe(['/']);
    expect(probe.get()?.entrance).toBe('developer');
    expect(probe.get()?.isFirstRun).toBe(false);
  });

  it('flags first-run (no stored value) even when a URL override is present', () => {
    const probe = renderProbe(['/?entrance=developer']);
    expect(probe.get()?.entrance).toBe('developer');
    expect(probe.get()?.isFirstRun).toBe(true);
  });

  it('clears first-run after setEntrance persists', async () => {
    const probe = renderProbe(['/']);
    expect(probe.get()?.isFirstRun).toBe(true);
    fireEvent.click(probe.getByTestId('set-developer'));
    await waitFor(() => expect(probe.get()?.entrance).toBe('developer'));
    expect(probe.get()?.isFirstRun).toBe(false);
  });

  it('gives the URL override precedence over the stored value', () => {
    window.localStorage.setItem(ENTRANCE_STORAGE_KEY, 'developer');
    const probe = renderProbe(['/?entrance=content-creator']);
    expect(probe.get()?.entrance).toBe('content-creator');
  });

  it('ignores an invalid URL override and falls through to the default', () => {
    const probe = renderProbe(['/?entrance=admin']);
    expect(probe.get()?.entrance).toBe('content-creator');
  });

  it('resolves a stored-but-unparseable value to content-creator WITHOUT writing', () => {
    window.localStorage.setItem(ENTRANCE_STORAGE_KEY, 'bogus');
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
    const probe = renderProbe(['/']);
    expect(probe.get()?.entrance).toBe('content-creator');
    expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('bogus');
    expect(setItemSpy).not.toHaveBeenCalledWith(ENTRANCE_STORAGE_KEY, expect.anything());
  });

  it('does not persist the URL override (session-only)', () => {
    renderProbe(['/?entrance=developer']);
    expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBeNull();
  });

  it('does not rewrite the URL when an override is present', () => {
    let captured: string | null = null;
    renderInApp(
      <EntranceProvider>
        <UrlProbe onPathname={(url) => (captured = url)} />
      </EntranceProvider>,
      { initialRouterEntries: ['/works?entrance=developer#conn'] },
    );
    expect(captured).toBe('/works?entrance=developer#conn');
  });

  it('persists and syncs through setEntrance (no optimistic write on the landing tree)', async () => {
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
    const probe = renderProbe(['/']);
    fireEvent.click(probe.getByTestId('set-developer'));
    await waitFor(() => expect(probe.get()?.entrance).toBe('developer'));
    expect(setItemSpy).toHaveBeenCalledWith(ENTRANCE_STORAGE_KEY, 'developer');
  });

  it('reads the desktop IPC pair on mount (fail-open on error)', async () => {
    const probe = renderProbe(['/'], {
      desktop: ipcDesktop({ getEntrance: async () => 'developer' }),
    });
    await waitFor(() => expect(probe.get()?.entrance).toBe('developer'));
    expect(probe.get()?.isLoading).toBe(false);
    // Desktop first-run is the wizard (AR-17) — never the identity page.
    expect(probe.get()?.isFirstRun).toBe(false);
  });

  it('never flags first-run on desktop, even when the IPC read fails', async () => {
    const probe = renderProbe(['/'], {
      desktop: ipcDesktop({
        getEntrance: async () => {
          throw new Error('command not registered');
        },
      }),
    });
    await waitFor(() => expect(probe.get()?.isLoading).toBe(false));
    expect(probe.get()?.entrance).toBe('content-creator');
    expect(probe.get()?.isFirstRun).toBe(false);
  });

  it('fails open to content-creator when the desktop command errors', async () => {
    const probe = renderProbe(['/'], {
      desktop: ipcDesktop({
        getEntrance: async () => {
          throw new Error('command not registered');
        },
      }),
    });
    await waitFor(() => expect(probe.get()?.isLoading).toBe(false));
    expect(probe.get()?.entrance).toBe('content-creator');
  });

  it('persists through the desktop IPC pair on setEntrance', async () => {
    const setEntrance = vi.fn(async () => undefined);
    const probe = renderProbe(['/'], { desktop: ipcDesktop({ setEntrance }) });
    fireEvent.click(probe.getByTestId('set-developer'));
    await waitFor(() => expect(probe.get()?.entrance).toBe('developer'));
    expect(setEntrance).toHaveBeenCalledWith('developer');
  });
});

/** Captures the router URL so tests can assert the provider never rewrites it. */
function UrlProbe({ onPathname }: { onPathname: (url: string) => void }) {
  const { pathname, search, hash } = useLocation();
  onPathname(`${pathname}${search}${hash}`);
  return null;
}
