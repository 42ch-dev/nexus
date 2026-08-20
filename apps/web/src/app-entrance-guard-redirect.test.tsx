/**
 * Entrance guard redirects + entrance-aware index redirect (V1.170 P1 — AR-18/AR-19).
 *
 * Mirrors the `app-capabilities-redirect.test.tsx` precedent: a focused route
 * tree with the REAL `EntranceGuard` / `EntranceIndexRedirect` and marker
 * elements, plus a `useLocation` probe so the resolved pathname is observable.
 * Create bounces develop-only routes with the `entrance.bounceToast` one-shot
 * toast; `allowDeepLink` passes through; Develop never bounces; the index
 * redirect follows `landRoute`.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Route, Routes, useLocation } from 'react-router';

import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { EntranceProvider } from '@/lib/entrance-context';
import {
  EntranceGuard,
  EntranceIndexRedirect,
  resolveEntranceBounce,
} from '@/components/layout/entrance-guard';

function makeClient() {
  return new BrowserClient();
}

/** Captures the resolved router location after redirects settle. */
function LocationProbe({ onPathname }: { onPathname: (p: string) => void }) {
  const { pathname } = useLocation();
  onPathname(pathname);
  return null;
}

/** Focused route tree using the real guard, mirroring App.tsx's gate shape. */
function GuardTree({ onPathname }: { onPathname: (p: string) => void }) {
  return (
    <>
      <LocationProbe onPathname={onPathname} />
      <EntranceProvider>
        <Routes>
          <Route
            path="strategies"
            element={
              <EntranceGuard>
                <div data-testid="strategies-route">Strategies</div>
              </EntranceGuard>
            }
          />
          <Route
            path="strategies/:presetId"
            element={
              <EntranceGuard>
                <div data-testid="canvas-route">Canvas</div>
              </EntranceGuard>
            }
          />
          <Route
            path="sessions"
            element={
              <EntranceGuard>
                <div data-testid="sessions-route">Sessions</div>
              </EntranceGuard>
            }
          />
          <Route
            path="settings/agent"
            element={
              <EntranceGuard>
                <div data-testid="settings-agent-route">Agent settings</div>
              </EntranceGuard>
            }
          />
          <Route
            path="settings/workspace"
            element={
              <EntranceGuard>
                <div data-testid="settings-workspace-route">Workspace settings</div>
              </EntranceGuard>
            }
          />
          <Route
            path="works"
            element={<div data-testid="works-route">Works</div>}
          />
          <Route
            path="developer"
            element={<div data-testid="developer-route">Developer hub</div>}
          />
          <Route path="*" element={<div data-testid="not-found">Not found</div>} />
        </Routes>
      </EntranceProvider>
    </>
  );
}

describe('EntranceGuard redirects (AR-19)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('bounces Create on /strategies to /works with the one-shot toast', async () => {
    let resolvedPathname = '';
    renderInApp(<GuardTree onPathname={(p) => (resolvedPathname = p)} />, {
      client: makeClient(),
      initialRouterEntries: ['/strategies'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('works-route')).toBeInTheDocument();
    });
    expect(resolvedPathname).toBe('/works');
    expect(screen.queryByTestId('strategies-route')).not.toBeInTheDocument();
    await screen.findByText('entrance.bounceToast');
    expect(screen.getAllByText('entrance.bounceToast')).toHaveLength(1);
  });

  it('bounces Create on /sessions to /works', async () => {
    let resolvedPath = '';
    renderGuardTree(['/sessions'], (p) => (resolvedPath = p));
    await waitFor(() => {
      expect(screen.getByTestId('works-route')).toBeInTheDocument();
    });
    expect(resolvedPath).toBe('/works');
  });

  it('bounces Create on a hidden settings section (/settings/agent)', async () => {
    renderGuardTree(['/settings/agent']);
    await waitFor(() => {
      expect(screen.getByTestId('works-route')).toBeInTheDocument();
    });
    await screen.findByText('entrance.bounceToast');
  });

  it('passes Create through on an allowed settings section (/settings/workspace)', async () => {
    renderGuardTree(['/settings/workspace']);
    await waitFor(() => {
      expect(screen.getByTestId('settings-workspace-route')).toBeInTheDocument();
    });
    expect(screen.queryByText('entrance.bounceToast')).not.toBeInTheDocument();
  });

  it('passes Create through the strategy canvas deep link (allowDeepLink, no toast)', async () => {
    renderGuardTree(['/strategies/preset-1']);
    await waitFor(() => {
      expect(screen.getByTestId('canvas-route')).toBeInTheDocument();
    });
    expect(screen.queryByText('entrance.bounceToast')).not.toBeInTheDocument();
  });

  it('passes Create through on both-visibility routes (/works)', async () => {
    renderGuardTree(['/works']);
    await waitFor(() => {
      expect(screen.getByTestId('works-route')).toBeInTheDocument();
    });
    expect(screen.queryByText('entrance.bounceToast')).not.toBeInTheDocument();
  });

  it('passes unknowns through to the catch-all (guard never 404s)', async () => {
    renderGuardTree(['/nope']);
    await waitFor(() => {
      expect(screen.getByTestId('not-found')).toBeInTheDocument();
    });
    expect(screen.queryByText('entrance.bounceToast')).not.toBeInTheDocument();
  });

  it('never bounces the Develop entrance, including develop-only routes', async () => {
    const first = renderGuardTree(['/strategies?entrance=developer']);
    await waitFor(() => {
      expect(screen.getByTestId('strategies-route')).toBeInTheDocument();
    });
    expect(screen.queryByText('entrance.bounceToast')).not.toBeInTheDocument();
    first.unmount();

    renderGuardTree(['/developer?entrance=developer']);
    await waitFor(() => {
      expect(screen.getByTestId('developer-route')).toBeInTheDocument();
    });
    expect(screen.queryByText('entrance.bounceToast')).not.toBeInTheDocument();
  });
});

/** Focused index-route tree using the real entrance-aware redirect. */
function IndexTree({ onPathname }: { onPathname: (p: string) => void }) {
  return (
    <>
      <LocationProbe onPathname={onPathname} />
      <EntranceProvider>
        <Routes>
          <Route index element={<EntranceIndexRedirect />} />
          <Route path="works" element={<div data-testid="works-route">Works</div>} />
          <Route
            path="developer"
            element={<div data-testid="developer-route">Developer hub</div>}
          />
        </Routes>
      </EntranceProvider>
    </>
  );
}

describe('EntranceIndexRedirect (AR-18)', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('redirects / to /works for the default content-creator entrance', async () => {
    let resolvedPath = '';
    renderIndexRedirect(['/'], (p) => (resolvedPath = p));
    await waitFor(() => {
      expect(screen.getByTestId('works-route')).toBeInTheDocument();
    });
    expect(resolvedPath).toBe('/works');
  });

  it('redirects / to /developer when the URL override is developer', async () => {
    let resolvedPath = '';
    renderIndexRedirect(['/?entrance=developer'], (p) => (resolvedPath = p));
    await waitFor(() => {
      expect(screen.getByTestId('developer-route')).toBeInTheDocument();
    });
    expect(resolvedPath).toBe('/developer');
  });

  it('redirects / by the stored entrance (browser `nexus-entrance`)', async () => {
    window.localStorage.setItem('nexus-entrance', 'developer');
    let resolvedPath = '';
    renderIndexRedirect(['/'], (p) => (resolvedPath = p));
    await waitFor(() => {
      expect(screen.getByTestId('developer-route')).toBeInTheDocument();
    });
    expect(resolvedPath).toBe('/developer');
  });
});

describe('resolveEntranceBounce (AR-19 classification)', () => {
  it('returns the land route for develop-only mismatches on Create', () => {
    expect(resolveEntranceBounce('content-creator', '/strategies', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/sessions', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/schedule', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/capabilities', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/connect', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/modules', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/works/w1/inspector', '')).toBe(
      '/works',
    );
    expect(resolveEntranceBounce('content-creator', '/settings/agent', '')).toBe('/works');
    expect(resolveEntranceBounce('content-creator', '/settings/advanced', '')).toBe('/works');
  });

  it('passes allow-deep-link and both-visibility surfaces through', () => {
    expect(resolveEntranceBounce('content-creator', '/strategies/p1', '')).toBeNull();
    expect(resolveEntranceBounce('content-creator', '/settings/workspace', '')).toBeNull();
    expect(resolveEntranceBounce('content-creator', '/settings/appearance', '')).toBeNull();
    expect(resolveEntranceBounce('content-creator', '/works', '')).toBeNull();
    expect(resolveEntranceBounce('content-creator', '/memory', '')).toBeNull();
    expect(resolveEntranceBounce('content-creator', '/timeline', '')).toBeNull();
    expect(resolveEntranceBounce('content-creator', '/unknown', '')).toBeNull();
  });

  it('never bounces the developer entrance', () => {
    expect(resolveEntranceBounce('developer', '/strategies', '')).toBeNull();
    expect(resolveEntranceBounce('developer', '/sessions', '')).toBeNull();
    expect(resolveEntranceBounce('developer', '/developer', '')).toBeNull();
    expect(resolveEntranceBounce('developer', '/settings/agent', '')).toBeNull();
  });
});

/** render helpers (wrap renderInApp with a client + guard tree). */
function renderGuardTree(
  initialRouterEntries: string[] = ['/strategies'],
  onPathname: (p: string) => void = () => {},
) {
  return renderInApp(
    <GuardTree onPathname={onPathname} />,
    { client: makeClient(), initialRouterEntries },
  );
}

function renderIndexRedirect(
  initialRouterEntries: string[],
  onPathname: (p: string) => void,
) {
  return renderInApp(
    <IndexTree onPathname={onPathname} />,
    { client: makeClient(), initialRouterEntries },
  );
}
