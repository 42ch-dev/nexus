/**
 * Capabilities soft-remove redirect — V1.120 P2 T2 (AC-P2-3).
 *
 * `/capabilities` is no longer an author-facing surface: it must redirect to
 * `/sessions` (deep link does not 404). This mirrors the route config declared
 * in `App.tsx` (`<Route path="capabilities" element={<Navigate to="/sessions"
 * replace />} />`) and asserts the redirect lands on `/sessions` content.
 *
 * Test strategy follows `app-work-routes.test.tsx`: a focused route tree with
 * the real `<Navigate>` declaration and marker elements, plus a `useLocation`
 * probe so the final pathname is observable through the MemoryRouter.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Navigate, Route, Routes, useLocation } from 'react-router';

import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient() {
  return new BrowserClient();
}

/** Captures the resolved router location after redirects settle. */
function LocationProbe({ onPathname }: { onPathname: (p: string) => void }) {
  const { pathname } = useLocation();
  onPathname(pathname);
  return null;
}

function CapabilitiesRedirectTree({ onPathname }: { onPathname: (p: string) => void }) {
  return (
    <>
      <LocationProbe onPathname={onPathname} />
      <Routes>
        {/* Mirrors App.tsx — `/capabilities` redirects to `/sessions`. */}
        <Route path="capabilities" element={<Navigate to="/sessions" replace />} />
        <Route path="sessions" element={<div data-testid="sessions-route">Sessions</div>} />
        <Route path="*" element={<div data-testid="not-found">Not found</div>} />
      </Routes>
    </>
  );
}

describe('Capabilities redirect (AC-P2-3)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('redirects /capabilities to /sessions (deep link does not 404)', async () => {
    let resolvedPathname = '';
    renderInApp(<CapabilitiesRedirectTree onPathname={(p) => (resolvedPathname = p)} />, {
      client: makeClient(),
      initialRouterEntries: ['/capabilities'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('sessions-route')).toBeInTheDocument();
    });

    // Redirect landed on /sessions content — not the catch-all Not Found.
    expect(resolvedPathname).toBe('/sessions');
    expect(screen.queryByTestId('not-found')).not.toBeInTheDocument();
  });
});
