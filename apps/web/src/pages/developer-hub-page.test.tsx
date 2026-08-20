/**
 * Develop hub v1 cards (V1.170 P1 — AR-18 / EL §4).
 *
 * Every card links to an EXISTING surface — no new endpoints. The presets
 * query may be pending or error (daemon unavailable) — the hub must still
 * render the full card set with sensible fallback targets.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { useHandlers } from '@/test/msw-server';
import { DeveloperHubPage } from '@/pages/developer-hub-page';

function makeClient() {
  return new BrowserClient();
}

const EXPECTED_LINKS: ReadonlyArray<readonly [string, string]> = [
  ['developer-hub-link-presets', '/strategies'],
  ['developer-hub-link-capabilities', '/capabilities'],
  ['developer-hub-link-modules', '/settings/modules'],
  ['developer-hub-link-strategy-canvas', '/strategies'],
  ['developer-hub-link-run-studio', '/settings/modules'],
  ['developer-hub-link-connect', '/connect'],
];

describe('DeveloperHubPage (EL §4)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders the six hub cards linking to existing surfaces', async () => {
    renderInApp(<DeveloperHubPage />, { client: makeClient() });

    await screen.findByTestId('developer-hub-page');
    for (const [testId, href] of EXPECTED_LINKS) {
      expect(screen.getByTestId(testId)).toHaveAttribute('href', href);
    }
    expect(screen.getByTestId('developer-hub-link-capabilities')).toHaveAttribute(
      'href',
      '/capabilities',
    );
  });

  it('stays link-only when the presets query fails (no count, canvas falls back to the manager)', async () => {
    renderInApp(<DeveloperHubPage />, { client: makeClient() });

    await screen.findByTestId('developer-hub-page');
    // Query error → no preset count line.
    await waitFor(() =>
      expect(
        screen.queryByTestId('developer-hub-presets-count'),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId('developer-hub-link-strategy-canvas')).toHaveAttribute(
      'href',
      '/strategies',
    );
  });

  it('shows the preset count excluding _system.* and links the canvas to the first USER preset (F-4)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/alpha', source: 'user' }],
          embedded: [
            { id: '_system.builtin', source: 'embedded' },
            { id: 'embedded/demo', source: 'embedded' },
          ],
          system: [],
        }),
      ),
    );
    renderInApp(<DeveloperHubPage />, { client: makeClient() });

    await screen.findByTestId('developer-hub-page');
    // Count = 1 user + 1 embedded non-`_system.` (the _system.* preset is
    // excluded per EL §4 / the existing _system.* product lock).
    await waitFor(() =>
      expect(screen.getByTestId('developer-hub-presets-count')).toHaveTextContent(
        '2 presets',
      ),
    );
    // Canvas target prefers `user[0]` over embedded presets.
    expect(screen.getByTestId('developer-hub-link-strategy-canvas')).toHaveAttribute(
      'href',
      '/strategies/user%2Falpha',
    );
  });

  it('falls back to the first embedded non-_system.* preset when there are no user presets (F-4)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [],
          embedded: [
            { id: '_system.builtin', source: 'embedded' },
            { id: 'embedded/demo', source: 'embedded' },
          ],
          system: [],
        }),
      ),
    );
    renderInApp(<DeveloperHubPage />, { client: makeClient() });

    await screen.findByTestId('developer-hub-page');
    await waitFor(() =>
      expect(screen.getByTestId('developer-hub-presets-count')).toHaveTextContent(
        '1 preset',
      ),
    );
    expect(screen.getByTestId('developer-hub-link-strategy-canvas')).toHaveAttribute(
      'href',
      '/strategies/embedded%2Fdemo',
    );
  });

  it('links the canvas to the manager when every preset group is empty (F-4)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({ user: [], embedded: [], system: [] }),
      ),
    );
    renderInApp(<DeveloperHubPage />, { client: makeClient() });

    await screen.findByTestId('developer-hub-page');
    // Empty success is distinct from the query-error case: the count line
    // renders with zero, and the canvas falls back to the manager.
    await waitFor(() =>
      expect(screen.getByTestId('developer-hub-presets-count')).toHaveTextContent(
        '0 presets',
      ),
    );
    expect(screen.getByTestId('developer-hub-link-strategy-canvas')).toHaveAttribute(
      'href',
      '/strategies',
    );
  });
});
