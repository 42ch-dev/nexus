/**
 * V1.171 P1 — Develop strategy catalog tests (PL-8/PL-9, AR-27/AR-28).
 *
 * The catalog lists USER + embedded (non-hidden) presets with trigger-lane
 * badges and honest entry paths consumed from each preset's P0 profile via
 * NexusClient — never derived from the id-only list response (AR-27). A
 * missing profile degrades gracefully to id + list facts (PL-13 boundary).
 * Creator-entrance absence is a route-rule regression (AR-28) covered
 * end-to-end by `app-entrance-guard-redirect.test.tsx`; the rule-table
 * assertions here pin the develop-only posture of `/strategies`.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router';

import { StrategiesPage } from '@/pages/strategies-page';
import { ENTRANCE_ROUTE_RULES } from '@/components/layout/entrance-registry';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient, type PresetProfileLanes } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function renderStrategies() {
  return renderInApp(<StrategiesPage />, { client: makeClient(), activeCreatorId: 'creator-a' });
}

function profileHandler(id: string, lanes: PresetProfileLanes) {
  return http.get(`/v1/daemon/orchestration/presets/${encodeURIComponent(id)}/profile`, () =>
    HttpResponse.json({
      id,
      version: 1,
      sourceHash: 'b'.repeat(64),
      lanes,
      states: [],
    }),
  );
}

/** Works-cron role preset lane set (PL-9 row 3) — e.g. novel-brainstorm. */
const CRON_LANES: PresetProfileLanes = {
  cron: true,
  wallClock: true,
  session: true,
  direct: true,
};

/** Embedded sample with trigger + scheduled lanes (PL-8 canonical pair). */
const GAME_NARRATIVE_LANES: PresetProfileLanes = {
  cron: false,
  wallClock: true,
  session: true,
  direct: true,
};

const NO_LANES: PresetProfileLanes = {
  cron: false,
  wallClock: false,
  session: false,
  direct: false,
};

/** Daemon 503 envelope when the orchestration engine is not running. */
const ENGINE_503_BODY = {
  success: false,
  error: { code: 'service_unavailable', message: 'engine not available' },
};

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('StrategyCatalog', () => {
  it('lists USER + embedded (non-hidden) presets with source badges; excludes system and _system.* presets', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [{ id: 'operator-strategy', source: 'system' }],
          embedded: [
            { id: 'game-narrative', source: 'embedded' },
            { id: '_system.maintenance', source: 'embedded' },
          ],
        }),
      ),
      profileHandler('user/foo', NO_LANES),
      profileHandler('game-narrative', GAME_NARRATIVE_LANES),
      profileHandler('_system.maintenance', NO_LANES),
    );

    renderStrategies();

    const catalog = within(await screen.findByTestId('strategy-catalog'));
    // User + embedded non-hidden rows present.
    expect(catalog.getByText('user/foo')).toBeInTheDocument();
    expect(catalog.getByText('game-narrative')).toBeInTheDocument();
    // System presets are not part of the catalog (PL-8).
    expect(catalog.queryByText('operator-strategy')).not.toBeInTheDocument();
    // `_system.` internals stay hidden (non-hidden rule).
    expect(catalog.queryByText('_system.maintenance')).not.toBeInTheDocument();
    // Source badges are list facts, rendered honestly.
    expect(catalog.getByTestId('catalog-source-user/foo')).toHaveTextContent('User');
    expect(catalog.getByTestId('catalog-source-game-narrative')).toHaveTextContent('Embedded');
  });

  it('shows trigger + scheduled lanes and honest entry paths for game-narrative (PL-8, PL-9)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [],
          system: [],
          embedded: [{ id: 'game-narrative', source: 'embedded' }],
        }),
      ),
      profileHandler('game-narrative', GAME_NARRATIVE_LANES),
    );

    renderStrategies();

    const row = await screen.findByTestId('catalog-row-game-narrative');
    // Trigger (session/direct) + Scheduled (wall-clock) lane badges.
    expect(await within(row).findByTestId('catalog-trigger-game-narrative')).toHaveTextContent('Trigger');
    expect(within(row).getByTestId('catalog-scheduled-game-narrative')).toHaveTextContent('Scheduled');
    // Honest entry paths: Connect-declared → backend owns the loop; daemon
    // schedule → requires creator daemon, explained as a wall-clock poller
    // (no cron recurrence) whose `scheduled_at` is a schedule field, not a
    // next-fire clock (PL-11). No Work-cron row (cron: false).
    const paths = await within(row).findByTestId('catalog-paths-game-narrative');
    expect(paths).toHaveTextContent('Your backend owns the loop');
    expect(paths).toHaveTextContent('Requires creator daemon');
    expect(paths).toHaveTextContent(
      "Daemon schedule admission on a wall-clock tick — no cron recurrence. The schedule's `scheduled_at` is a schedule field, not a next-fire clock.",
    );
    expect(paths).not.toHaveTextContent('Work cron roles');

    // PL-12: no fabricated next-run clock anywhere on the row.
    expect(within(row).queryByText(/next run at/i)).not.toBeInTheDocument();
    expect(within(row).queryByText(/next fire at/i)).not.toBeInTheDocument();
  });

  it('labels Work cron roles distinctly from the wall-clock poller (PL-9 row 3)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [],
          system: [],
          embedded: [{ id: 'novel-brainstorm', source: 'embedded' }],
        }),
      ),
      profileHandler('novel-brainstorm', CRON_LANES),
    );

    renderStrategies();

    const row = await screen.findByTestId('catalog-row-novel-brainstorm');
    const paths = await within(row).findByTestId('catalog-paths-novel-brainstorm');
    expect(paths).toHaveTextContent('Work cron roles');
    expect(paths).toHaveTextContent(
      'Per-Work cron roles (brainstorm / write / review) — distinct from the wall-clock poller.',
    );
  });

  it('derives lane badges from the profile, never from list data (AR-27)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/custom', source: 'user', run_intents: ['write', 'edit'] }],
          system: [],
          embedded: [],
        }),
      ),
      // Profile says no lanes at all — despite run_intents in the list data.
      profileHandler('user/custom', NO_LANES),
    );

    renderStrategies();

    const row = await screen.findByTestId('catalog-row-user/custom');
    expect(within(row).queryByTestId('catalog-trigger-user/custom')).not.toBeInTheDocument();
    expect(within(row).queryByTestId('catalog-scheduled-user/custom')).not.toBeInTheDocument();
    // List facts (id + source) still render.
    expect(within(row).getByText('user/custom')).toBeInTheDocument();
    expect(within(row).getByTestId('catalog-source-user/custom')).toHaveTextContent('User');
  });

  it('degrades gracefully when the profile is missing — id + list facts, not a "preset gone" error (PL-13)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/ghost', source: 'user' }],
          system: [],
          embedded: [],
        }),
      ),
      http.get('/v1/daemon/orchestration/presets/user%2Fghost/profile', () =>
        HttpResponse.json(
          { error: { code: 'not_found', message: 'no such preset' } },
          { status: 404 },
        ),
      ),
    );

    renderStrategies();

    const row = await screen.findByTestId('catalog-row-user/ghost');
    // Row still lists the preset (it exists in the list) with its source.
    expect(within(row).getByText('user/ghost')).toBeInTheDocument();
    expect(within(row).getByTestId('catalog-source-user/ghost')).toHaveTextContent('User');
    // Honest fallback copy, not a hard error implying the preset is gone.
    await waitFor(() =>
      expect(within(row).getByTestId('catalog-profile-unavailable-user/ghost')).toHaveTextContent(
        'Profile unavailable — lane details could not be loaded.',
      ),
    );
    expect(within(row).queryByTestId('catalog-trigger-user/ghost')).not.toBeInTheDocument();
    expect(within(row).queryByTestId('catalog-scheduled-user/ghost')).not.toBeInTheDocument();
  });

  it('keeps the catalog behind the develop-only /strategies route rule (AR-28)', () => {
    const listRule = ENTRANCE_ROUTE_RULES.find((rule) => rule.path === '/strategies');
    const detailRule = ENTRANCE_ROUTE_RULES.find((rule) => rule.path === '/strategies/:presetId');
    expect(listRule?.visibility).toBe('develop-only');
    expect(detailRule?.visibility).toBe('develop-only');
    expect(detailRule?.allowDeepLink).toBe(true);
  });

  it('opens the profile drill-down when a catalog row is selected (PL-13)', async () => {
    function LocationProbe() {
      const location = useLocation();
      return <div data-testid="location-probe">{location.pathname}</div>;
    }

    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [],
          system: [],
          embedded: [{ id: 'game-narrative', source: 'embedded' }],
        }),
      ),
      profileHandler('game-narrative', GAME_NARRATIVE_LANES),
    );

    renderInApp(
      <>
        <LocationProbe />
        <Routes>
          <Route path="/strategies" element={<StrategiesPage />} />
          <Route path="*" element={<div data-testid="probe-route" />} />
        </Routes>
      </>,
      { client: makeClient(), activeCreatorId: 'creator-a', initialRouterEntries: ['/strategies'] },
    );

    const row = await screen.findByTestId('catalog-row-game-narrative');
    await user.click(within(row).getByRole('button', { name: 'View profile of game-narrative' }));

    await waitFor(() =>
      expect(screen.getByTestId('location-probe')).toHaveTextContent(
        '/strategies/game-narrative/profile',
      ),
    );
  });

  it('updates catalog lane badges after a preset reload without navigation (F-1 / QC1 W-1)', async () => {
    const user = userEvent.setup();
    // The reload request swaps the on-disk manifest lane set before the
    // invalidated profile query refetches — a mounted catalog must pick the
    // change up without a remount.
    let lanes: PresetProfileLanes = NO_LANES;
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [],
          embedded: [],
        }),
      ),
      http.get('/v1/daemon/orchestration/presets/user%2Ffoo/profile', () =>
        HttpResponse.json({
          id: 'user/foo',
          version: 2,
          sourceHash: 'b'.repeat(64),
          lanes,
          states: [],
        }),
      ),
      http.post('/v1/daemon/presets/user%2Ffoo:reload', () => {
        lanes = CRON_LANES;
        return HttpResponse.json({ id: 'user/foo', reloaded: true });
      }),
    );

    renderStrategies();

    const row = await screen.findByTestId('catalog-row-user/foo');
    // Pre-reload manifest declares no lanes — no badges.
    expect(within(row).queryByTestId('catalog-trigger-user/foo')).not.toBeInTheDocument();
    expect(within(row).queryByTestId('catalog-scheduled-user/foo')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Reload' }));

    // `useReloadPreset` invalidates `presets.all`, so the mounted profile
    // refetches and the badge updates — still on the same page.
    await waitFor(() =>
      expect(screen.getByTestId('catalog-trigger-user/foo')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('catalog-scheduled-user/foo')).toBeInTheDocument();
    expect(screen.getByTestId('strategy-catalog')).toBeInTheDocument();
  });

  it('renders ONE catalog-level engine-unavailable state with retry when every profile 503s (F-2 / QC3 W-3)', async () => {
    const user = userEvent.setup();
    // Engine down for the first profile attempts; recovers before retry.
    let engineDown = true;
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [],
          embedded: [{ id: 'game-narrative', source: 'embedded' }],
        }),
      ),
      http.get('/v1/daemon/orchestration/presets/user%2Ffoo/profile', () =>
        engineDown
          ? HttpResponse.json(ENGINE_503_BODY, { status: 503 })
          : HttpResponse.json({
              id: 'user/foo',
              version: 1,
              sourceHash: 'b'.repeat(64),
              lanes: NO_LANES,
              states: [],
            }),
      ),
      http.get('/v1/daemon/orchestration/presets/game-narrative/profile', () =>
        engineDown
          ? HttpResponse.json(ENGINE_503_BODY, { status: 503 })
          : HttpResponse.json({
              id: 'game-narrative',
              version: 1,
              sourceHash: 'c'.repeat(64),
              lanes: GAME_NARRATIVE_LANES,
              states: [],
            }),
      ),
    );

    renderStrategies();

    const catalog = await screen.findByTestId('strategy-catalog');
    await waitFor(() =>
      expect(within(catalog).getByText('Orchestration engine not running')).toBeInTheDocument(),
    );
    // ONE catalog-level state: no rows, no N per-row unavailable lines.
    expect(within(catalog).getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(within(catalog).queryByTestId('catalog-row-user/foo')).not.toBeInTheDocument();
    expect(within(catalog).queryByTestId('catalog-row-game-narrative')).not.toBeInTheDocument();
    expect(within(catalog).queryAllByTestId(/catalog-profile-unavailable-/)).toHaveLength(0);

    // Retry refetches the profile queries; the engine recovered → rows render.
    engineDown = false;
    await user.click(within(catalog).getByRole('button', { name: 'Try again' }));

    const row = await screen.findByTestId('catalog-row-game-narrative');
    expect(await within(row).findByTestId('catalog-trigger-game-narrative')).toBeInTheDocument();
    expect(within(row).getByTestId('catalog-scheduled-game-narrative')).toBeInTheDocument();
  });
});
