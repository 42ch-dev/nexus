/**
 * Capabilities route — V1.170 P1 (EL-6): live Develop-tree surface.
 *
 * `/capabilities` is restored as a LIVE route on the Develop tree (read-only
 * builtin capability schemas). The V1.120 P2 soft-remove redirect
 * (`/capabilities → /sessions`) is replaced by the entrance guard: the Create
 * tree bounces to its land route (`/works`) with the one-shot
 * `entrance.bounceToast`, while Develop renders the browser. Deep links never
 * 404 on either tree.
 *
 * Test strategy follows `app-entrance-guard-redirect.test.tsx`: a focused
 * route tree with the REAL `EntranceGuard` and marker elements, plus a
 * `useLocation` probe so the resolved pathname is observable through the
 * MemoryRouter.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Route, Routes, useLocation } from 'react-router';

import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { EntranceGuard } from '@/components/layout/entrance-guard';

function makeClient() {
  return new BrowserClient();
}

/** Captures the resolved router location after redirects settle. */
function LocationProbe({ onPathname }: { onPathname: (p: string) => void }) {
  const { pathname } = useLocation();
  onPathname(pathname);
  return null;
}

/** Focused route tree mirroring App.tsx's gate shape for `/capabilities`. */
function CapabilitiesTree({ onPathname }: { onPathname: (p: string) => void }) {
  return (
    <>
      <LocationProbe onPathname={onPathname} />
      <Routes>
        {/* Mirrors App.tsx — `/capabilities` renders the live browser inside
            the guard; Create bounces to `/works`, Develop passes through. */}
        <Route
          path="capabilities"
          element={
            <EntranceGuard>
              <div data-testid="capabilities-route">Capabilities browser</div>
            </EntranceGuard>
          }
        />
        <Route path="sessions" element={<div data-testid="sessions-route">Sessions</div>} />
        <Route path="works" element={<div data-testid="works-route">Works</div>} />
        <Route path="*" element={<div data-testid="not-found">Not found</div>} />
      </Routes>
    </>
  );
}

describe('Capabilities route (EL-6)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('bounces Create on /capabilities to the Create land route /works (soft-remove via guard)', async () => {
    let resolvedPathname = '';
    renderInApp(<CapabilitiesTree onPathname={(p) => (resolvedPathname = p)} />, {
      client: makeClient(),
      initialRouterEntries: ['/capabilities'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('works-route')).toBeInTheDocument();
    });
    // The V1.120 redirect target `/sessions` is no longer involved.
    expect(resolvedPathname).toBe('/works');
    expect(screen.queryByTestId('capabilities-route')).not.toBeInTheDocument();
    expect(screen.queryByTestId('sessions-route')).not.toBeInTheDocument();
    expect(screen.queryByTestId('not-found')).not.toBeInTheDocument();
    await screen.findByText('Available in the Develop layout — switch entrance to use this.');
    expect(
      screen.getAllByText('Available in the Develop layout — switch entrance to use this.'),
    ).toHaveLength(1);
  });

  it('renders the live capability browser on Develop (deep link does not 404)', async () => {
    window.localStorage.setItem('nexus-entrance', 'developer');
    let resolvedPathname = '';
    renderInApp(<CapabilitiesTree onPathname={(p) => (resolvedPathname = p)} />, {
      client: makeClient(),
      initialRouterEntries: ['/capabilities'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('capabilities-route')).toBeInTheDocument();
    });
    expect(resolvedPathname).toBe('/capabilities');
    expect(screen.queryByTestId('not-found')).not.toBeInTheDocument();
    expect(
      screen.queryByText('Available in the Develop layout — switch entrance to use this.'),
    ).not.toBeInTheDocument();
  });
});
