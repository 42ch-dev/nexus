/**
 * V1.171 P1 — per-preset profile drill-down tests (PL-13, AR-27).
 *
 * The profile view renders states (enter / exit-when / next), roles +
 * recommended skills, and required capabilities deep-linked to the
 * capability-schema browser — all from the P0 profile fields consumed via
 * NexusClient (AR-27). A missing profile renders a graceful summary (id +
 * list facts), never a hard error implying the preset is gone (PL-13).
 * Trigger lanes render with the locked vocabulary (PL-11) and declared
 * signals render "Declared, not delivered" with a lifecycle pointer (PL-10);
 * no next-run clock is fabricated (PL-12).
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import { Route, Routes } from 'react-router';

import { PresetProfilePage } from '@/pages/preset-profile-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient, type PresetProfileResponse } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function renderProfile(presetId: string) {
  return renderInApp(
    <Routes>
      <Route path="strategies/:presetId/profile" element={<PresetProfilePage />} />
    </Routes>,
    { client: makeClient(), initialRouterEntries: [`/strategies/${encodeURIComponent(presetId)}/profile`] },
  );
}

const LIST = {
  user: [{ id: 'user/foo', source: 'user' }],
  system: [],
  embedded: [{ id: 'game-narrative', source: 'embedded' }],
};

function listHandler() {
  return http.get('/v1/daemon/presets', () => HttpResponse.json(LIST));
}

/** Rich profile exercising every PL-13 field: states (enter/exit-when/next),
 * roles + recommended skills, required capabilities, declared signals. */
const RICH_PROFILE: PresetProfileResponse = {
  id: 'game-narrative',
  version: 3,
  sourceHash: 'c'.repeat(64),
  lanes: { cron: false, wallClock: true, session: true, direct: true },
  states: [
    {
      id: 'opening',
      description: 'Establishes the world premise.',
      enter: [{ kind: 'capability', name: 'nexus.world.describe' }],
      exitWhen: {
        kind: 'llm_judge',
        judgeCapability: 'nexus.judge.quality',
        minInterval: '1h',
      },
      next: { kind: 'goNogo', go: 'develop', nogo: 'opening' },
      terminal: false,
    },
    {
      id: 'develop',
      enter: [
        { kind: 'inner_graph', name: 'plot-weave' },
        { kind: 'host_tool', name: 'fs.write' },
      ],
      exitWhen: { kind: 'graph_complete' },
      next: {
        kind: 'labeled',
        labeled: [{ label: 'complete', target: 'ending' }],
      },
      terminal: false,
    },
    { id: 'ending', exitWhen: { kind: 'manual' }, terminal: true },
  ],
  roles: [
    {
      id: 'narrator',
      description: 'Owns prose voice.',
      systemPromptFile: 'prompts/narrator.md',
      recommendedSkills: ['worldbuilding', 'prose'],
    },
    {
      id: 'editor',
      description: 'Reviews chapters.',
      systemPromptFile: 'prompts/editor.md',
    },
  ],
  requiredCapabilities: ['nexus.world.describe', 'nexus.judge.quality'],
  signals: [
    { name: 'pause', action: 'pause' },
    { name: 'stop', action: 'force_transition', target: 'ending' },
  ],
};

/** Minimal profile exercising the empty/absent-field states. */
const MINIMAL_PROFILE: PresetProfileResponse = {
  id: 'user/foo',
  version: 1,
  sourceHash: 'd'.repeat(64),
  lanes: { cron: false, wallClock: false, session: false, direct: false },
  states: [],
};

function profileHandler(profile: PresetProfileResponse) {
  return http.get(
    `/v1/daemon/orchestration/presets/${encodeURIComponent(profile.id)}/profile`,
    () => HttpResponse.json(profile),
  );
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('PresetProfilePage', () => {
  it('renders states with enter actions, exit-when, and next transitions from profile fields (PL-13)', async () => {
    useHandlers(listHandler(), profileHandler(RICH_PROFILE));

    renderProfile('game-narrative');

    // Header identity comes from the profile response (AR-27).
    await screen.findByTestId('profile-id');
    expect(screen.getByTestId('profile-id')).toHaveTextContent('game-narrative');
    expect(screen.getByTestId('profile-version')).toHaveTextContent('v3');

    // opening: enter capability + llm_judge exit + goNogo next.
    const opening = screen.getByTestId('profile-state-opening');
    expect(opening).toHaveTextContent('Enter');
    expect(opening).toHaveTextContent('capability: nexus.world.describe');
    expect(opening).toHaveTextContent('Establishes the world premise.');
    expect(screen.getByTestId('profile-exit-opening')).toHaveTextContent(
      'llm_judge · judge: nexus.judge.quality · min interval: 1h',
    );
    expect(screen.getByTestId('profile-next-opening')).toHaveTextContent(
      'goNogo → go: develop / nogo: opening',
    );

    // develop: inner_graph + host_tool enter kinds, labeled next form.
    const develop = screen.getByTestId('profile-state-develop');
    expect(develop).toHaveTextContent('inner_graph: plot-weave');
    expect(develop).toHaveTextContent('host_tool: fs.write');
    expect(screen.getByTestId('profile-exit-develop')).toHaveTextContent('graph_complete');
    expect(screen.getByTestId('profile-next-develop')).toHaveTextContent(
      'labeled → complete → ending',
    );

    // ending: terminal state — no next form rendered.
    const ending = screen.getByTestId('profile-state-ending');
    expect(ending).toHaveTextContent('Terminal');
    expect(ending).toHaveTextContent('manual');
    expect(within(ending).queryByTestId('profile-next-ending')).toBeNull();
  });

  it('renders roles with system prompts and recommended skills (PL-13)', async () => {
    useHandlers(listHandler(), profileHandler(RICH_PROFILE));

    renderProfile('game-narrative');

    const narrator = await screen.findByTestId('profile-role-narrator');
    expect(narrator).toHaveTextContent('Owns prose voice.');
    expect(narrator).toHaveTextContent('System prompt');
    expect(narrator).toHaveTextContent('prompts/narrator.md');
    expect(screen.getByTestId('profile-skill-narrator-worldbuilding')).toHaveTextContent('worldbuilding');
    expect(screen.getByTestId('profile-skill-narrator-prose')).toHaveTextContent('prose');

    // Role without recommendedSkills shows the honest "None declared" line.
    const editor = screen.getByTestId('profile-role-editor');
    expect(editor).toHaveTextContent('None declared');
  });

  it('renders required capabilities as deep links to the capability-schema browser (PL-13)', async () => {
    useHandlers(listHandler(), profileHandler(RICH_PROFILE));

    renderProfile('game-narrative');

    const describeLink = await screen.findByTestId('profile-capability-nexus.world.describe');
    expect(describeLink).toHaveAttribute('href', '/capabilities?filter=nexus.world.describe');
    expect(screen.getByTestId('profile-capability-nexus.judge.quality')).toHaveAttribute(
      'href',
      '/capabilities?filter=nexus.judge.quality',
    );
  });

  it('renders declared signals as "Declared, not delivered" with a lifecycle pointer (PL-10)', async () => {
    useHandlers(listHandler(), profileHandler(RICH_PROFILE));

    renderProfile('game-narrative');

    // Honesty badge — locked vocabulary (AR-25 parity with `preset show`).
    const badge = await screen.findByTestId('profile-signals-not-delivered');
    expect(badge).toHaveTextContent('Declared, not delivered');

    // Card copy: declared ≠ delivered; nothing fires them at runtime; not
    // bindable webhooks; lifecycle runs through the signal actions.
    const card = screen.getByTestId('profile-signals');
    expect(card).toHaveTextContent(
      'declared, not delivered. Nothing fires them at runtime and they are not bindable webhooks',
    );
    expect(card).toHaveTextContent(
      'Lifecycle control runs through the signal actions: start / pause / resume / cancel / advance.',
    );

    // Declared metadata still renders (name / action / target).
    const pause = screen.getByTestId('profile-signal-pause');
    expect(pause).toHaveTextContent('pause');
    expect(pause).toHaveTextContent('Action: pause');
    const stop = screen.getByTestId('profile-signal-stop');
    expect(stop).toHaveTextContent('Action: force_transition');
    expect(stop).toHaveTextContent('Target: ending');

    // No delivery/bind UI exists in the card — no buttons or links that
    // could offer webhook binding / "deliver this signal".
    expect(within(card).queryByRole('button')).not.toBeInTheDocument();
    expect(within(card).queryByRole('link')).not.toBeInTheDocument();
  });

  it('shows the trigger-lane classification with locked vocabulary and no fabricated next-fire (PL-11, PL-12)', async () => {
    useHandlers(listHandler(), profileHandler(RICH_PROFILE));

    renderProfile('game-narrative');

    const lanesCard = await screen.findByTestId('profile-lanes');
    expect(lanesCard).toHaveTextContent('Trigger lanes');

    // Classification is read from the profile flags — session/direct
    // (trigger), wall-clock schedule, Work-role cron — with yes/no presence.
    expect(screen.getByTestId('profile-lane-cron-no')).toHaveTextContent('No');
    expect(screen.getByTestId('profile-lane-wallclock-yes')).toHaveTextContent('Yes');
    expect(screen.getByTestId('profile-lane-session-yes')).toHaveTextContent('Yes');
    expect(screen.getByTestId('profile-lane-direct-yes')).toHaveTextContent('Yes');

    // Locked vocabulary: cron is per-Work roles (and timezone), not the
    // wall-clock poller; `scheduled_at` is a schedule field, not a
    // fabricated next-fire clock.
    expect(screen.getByTestId('profile-lane-cron')).toHaveTextContent(
      'Per-Work cron with roles (brainstorm / write / review) and timezone — not the wall-clock poller.',
    );
    expect(screen.getByTestId('profile-lane-wallclock')).toHaveTextContent(
      '`scheduled_at` is a schedule field, not a next-fire clock.',
    );

    // PL-12: no fabricated next-run clock anywhere on the profile page.
    expect(screen.queryByText(/next run at/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/next fire at/i)).not.toBeInTheDocument();
  });

  it('degrades gracefully when the profile is missing — id + list facts, not a "preset gone" error (PL-13)', async () => {
    useHandlers(
      // The list still shows the preset — the profile failure must not
      // imply it is gone (PL-13 premise).
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

    renderProfile('user/ghost');

    const summary = await screen.findByTestId('preset-profile-unavailable');
    // Id + list fact (source) — the list still shows the preset.
    expect(summary).toHaveTextContent('user/ghost');
    expect(screen.getByTestId('profile-unavailable-source')).toHaveTextContent('User');
    // Honest copy: profile failed to load, the preset is still listed.
    expect(summary).toHaveTextContent('The profile for user/ghost could not be loaded.');
    expect(summary).toHaveTextContent('The preset is still listed in the catalog.');
    // Never a hard error implying the preset is gone.
    expect(screen.queryByText('Preset not found')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('omits the "still listed in the catalog" sentence when the preset is absent from the list (F-4 / QC3 S-1)', async () => {
    useHandlers(
      // The deep-linked id is NOT in any list group — the copy must not
      // claim it is still listed.
      listHandler(),
      http.get('/v1/daemon/orchestration/presets/unknown%2Fpreset/profile', () =>
        HttpResponse.json(
          { error: { code: 'not_found', message: 'no such preset' } },
          { status: 404 },
        ),
      ),
    );

    renderProfile('unknown/preset');

    const summary = await screen.findByTestId('preset-profile-unavailable');
    // Base honest copy still renders…
    expect(summary).toHaveTextContent('The profile for unknown/preset could not be loaded.');
    // …but "still listed in the catalog" is absent (list lookup found
    // nothing) and no source badge is shown (no list facts for the id).
    expect(summary).not.toHaveTextContent('The preset is still listed in the catalog.');
    expect(screen.queryByTestId('profile-unavailable-source')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('renders the unavailable state when the orchestration engine is down (503)', async () => {
    useHandlers(
      listHandler(),
      http.get('/v1/daemon/orchestration/presets/game-narrative/profile', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'service_unavailable', message: 'engine not available' },
          },
          { status: 503 },
        ),
      ),
    );

    renderProfile('game-narrative');

    await waitFor(() =>
      expect(screen.getByText('Orchestration engine not running')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('shows honest empty lines when roles / capabilities / signals are absent', async () => {
    useHandlers(listHandler(), profileHandler(MINIMAL_PROFILE));

    renderProfile('user/foo');

    expect(await screen.findByTestId('profile-id')).toHaveTextContent('user/foo');
    // No-lane preset: every classification renders honestly as "No".
    expect(screen.getByTestId('profile-lane-cron-no')).toHaveTextContent('No');
    expect(screen.getByTestId('profile-lane-wallclock-no')).toHaveTextContent('No');
    expect(screen.getByTestId('profile-lane-session-no')).toHaveTextContent('No');
    expect(screen.getByTestId('profile-lane-direct-no')).toHaveTextContent('No');
    expect(screen.getByText('No states declared.')).toBeInTheDocument();
    expect(screen.getByText('Single-agent preset — no roles declared.')).toBeInTheDocument();
    expect(screen.getByText('No required capabilities declared.')).toBeInTheDocument();
    expect(screen.getByText('No signals declared.')).toBeInTheDocument();
  });
});
