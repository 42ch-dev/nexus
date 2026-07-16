/**
 * Settings Agent section — mount, G1 preselect, Save Agent (V1.103 P1).
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Navigate, Route, Routes } from 'react-router-dom';

import { SettingsAgentSection } from '@/pages/settings/settings-agent-section';
import { SettingsShellLayout } from '@/pages/settings/settings-shell-layout';
import { makeQueryClient, renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(
  overrides: Partial<DesktopCapabilities> = {},
): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    openExternalUrl: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: (callback) => {
      callback({ state: 'running', port: 8420 });
      return Promise.resolve(() => {});
    },
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(true),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getAgentProfile: () => Promise.resolve(null),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () =>
      Promise.resolve({
        creator_id: 'ctr_local1234567890ab',
        already_bootstrapped: true,
      }),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    ...overrides,
  };
}

const MIXED_AGENTS = [
  {
    name: 'claude-code',
    // P2: claude-acp is hard-excluded; use the native curated key so the card
    // renders in the default grid (priority 1).
    registry_agent_id: 'claude-native',
    launch_command: 'claude',
    installed: true,
    version: '1.0.0',
  },
  {
    name: 'codex',
    registry_agent_id: 'codex-native',
    launch_command: 'codex',
    installed: true,
    version: '1.0.0',
  },
];

function scanHandler(agents: Array<Record<string, unknown>> = MIXED_AGENTS) {
  return http.post('/v1/daemon/agent-host/scan', () =>
    HttpResponse.json({ agents }),
  );
}

function creatorsHandler() {
  return http.get('/v1/daemon/creators', () =>
    HttpResponse.json({
      items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
      pagination: { limit: 20, has_more: false },
    }),
  );
}

const settingsRouteTree = (
  <Route path="settings" element={<SettingsShellLayout />}>
    <Route index element={<Navigate to="agent" replace />} />
    <Route path="agent" element={<SettingsAgentSection />} />
  </Route>
);

describe('SettingsAgentSection preselect (G1)', () => {
  it('renders locked Agent helper and browser-only helper without desktop', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'Agent', level: 3 }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Choose which local agent Nexus uses for creative work/i),
    ).toBeInTheDocument();
    expect(screen.getByTestId('settings-agent-browser-helper')).toHaveTextContent(
      'Agent selection is available on the desktop app only.',
    );

    // claude-native (priority 1, installed) renders in the default grid once
    // the async scan settles.
    expect(await screen.findByTestId('agent-card-claude-native')).toBeInTheDocument();
  });

  it('preselects saved profile by name after scan settles (desktop)', async () => {
    const getAgentProfile = vi.fn(() =>
      Promise.resolve({ name: 'codex', launchCommand: 'codex' }),
    );
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: makeDesktop({ getAgentProfile }),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('agent-picker')).toBeInTheDocument(),
    );
    // codex-native (priority 0, installed) renders in the default grid.
    expect(await screen.findByTestId('agent-card-codex-native')).toBeInTheDocument();

    await waitFor(() => expect(getAgentProfile).toHaveBeenCalled());

    await waitFor(() => {
      const pressed = screen
        .getAllByTestId('agent-card-select-codex-native')
        .filter((el) => el.getAttribute('aria-pressed') === 'true');
      expect(pressed.length).toBeGreaterThanOrEqual(1);
    });

    // Not the first-installed Claude default when a saved profile matches Codex.
    const claudePressed = screen
      .getAllByTestId('agent-card-select-claude-native')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(claudePressed.length).toBe(0);
    expect(screen.queryByTestId('settings-agent-browser-helper')).not.toBeInTheDocument();
  });

  it('does not overwrite author selection made before async preselect resolves', async () => {
    const user = userEvent.setup();
    let resolveProfile: (value: { name: string; launchCommand?: string } | null) => void =
      () => {};
    const getAgentProfile = vi.fn(
      () =>
        new Promise<{ name: string; launchCommand?: string } | null>((resolve) => {
          resolveProfile = resolve;
        }),
    );
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: makeDesktop({ getAgentProfile }),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('agent-picker')).toBeInTheDocument(),
    );
    // codex-native renders in the default grid.
    await user.click(await screen.findByTestId('agent-card-select-codex-native'));

    await waitFor(() => {
      const pressed = screen
        .getAllByTestId('agent-card-select-codex-native')
        .filter((el) => el.getAttribute('aria-pressed') === 'true');
      expect(pressed.length).toBeGreaterThanOrEqual(1);
    });

    // Late profile would prefer Claude (first-installed name) if applied.
    resolveProfile({ name: 'claude-code', launchCommand: 'claude' });

    await waitFor(() => expect(getAgentProfile).toHaveBeenCalled());

    // Author's Codex click must stick — late preselect must not snap back.
    await waitFor(() => {
      const codexPressed = screen
        .getAllByTestId('agent-card-select-codex-native')
        .filter((el) => el.getAttribute('aria-pressed') === 'true');
      expect(codexPressed.length).toBeGreaterThanOrEqual(1);
    });
    const claudePressed = screen
      .getAllByTestId('agent-card-select-claude-native')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(claudePressed.length).toBe(0);
  });

  it('falls back to first installed when getAgentProfile returns null', async () => {
    const getAgentProfile = vi.fn(() => Promise.resolve(null));
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: makeDesktop({ getAgentProfile }),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    await waitFor(() => expect(getAgentProfile).toHaveBeenCalled());

    // claude-native is the first installed agent in the scan (browser/desktop
    // fallback selects the first installed) → renders preselected in the grid.
    await waitFor(() => {
      const pressed = screen
        .getAllByTestId('agent-card-select-claude-native')
        .filter((el) => el.getAttribute('aria-pressed') === 'true');
      expect(pressed.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('uses custom launch when saved name is not in scan but launchCommand is set', async () => {
    const getAgentProfile = vi.fn(() =>
      Promise.resolve({ name: 'custom-agent', launchCommand: '/usr/local/bin/my-agent' }),
    );
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: makeDesktop({ getAgentProfile }),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    await waitFor(() => expect(getAgentProfile).toHaveBeenCalled());

    await waitFor(() => {
      const custom = screen.getByTestId('agent-picker-custom-launch');
      expect(within(custom).getByRole('textbox')).toHaveValue(
        '/usr/local/bin/my-agent',
      );
    });
  });

  it('persists via setAgentProfile on Save Agent, dirty-gated (AC-P1-1)', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.resolve());
    const getAgentProfile = vi.fn(() =>
      Promise.resolve({ name: 'codex', launchCommand: 'codex' }),
    );
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setAgentProfile, getAgentProfile }),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    // codex-native is preselected by the saved profile → CLEAN (Save disabled).
    await waitFor(() => {
      const pressed = screen
        .getAllByTestId('agent-card-select-codex-native')
        .filter((el) => el.getAttribute('aria-pressed') === 'true');
      expect(pressed.length).toBeGreaterThanOrEqual(1);
    });
    await waitFor(() => expect(getAgentProfile).toHaveBeenCalled());

    // AC-P1-1: selection matches last-saved profile → Save disabled.
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-agent')).toBeDisabled(),
    );

    // DIRTY: switch to claude → Save enabled.
    await user.click(screen.getByTestId('agent-card-select-claude-native'));
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-agent')).not.toBeDisabled(),
    );

    await user.click(screen.getByTestId('settings-save-agent'));

    await waitFor(() =>
      expect(setAgentProfile).toHaveBeenCalledWith('claude-code', 'claude'),
    );
    expect(await screen.findByText('Agent profile saved')).toBeInTheDocument();

    // AC-P1-1: after Save success the baseline updates and the gate closes
    // (clean again → Save disabled).
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-agent')).toBeDisabled(),
    );
  });

  it('AC-P1-2/AC-P1-7: Save invalidates the query keys DaemonStatusBar consumes (footer refresh signal)', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.resolve());
    const getAgentProfile = vi.fn(() =>
      Promise.resolve({ name: 'codex', launchCommand: 'codex' }),
    );
    useHandlers(scanHandler(), creatorsHandler());
    const qc = makeQueryClient();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setAgentProfile, getAgentProfile }),
        initialRouterEntries: ['/settings/agent'],
        queryClient: qc,
      },
    );

    // Wait for preselect (clean) then dirty + save.
    await waitFor(() => expect(getAgentProfile).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-agent')).toBeDisabled(),
    );

    await user.click(screen.getByTestId('agent-card-select-claude-native'));
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-agent')).not.toBeDisabled(),
    );
    await user.click(screen.getByTestId('settings-save-agent'));

    await waitFor(() =>
      expect(setAgentProfile).toHaveBeenCalledWith('claude-code', 'claude'),
    );

    // Footer refresh signal: exactly the keys DaemonStatusBar reads are
    // invalidated so the agent badge refreshes without a 10s poll (AD-P1-1).
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: queryKeys.agentProfile.detail(),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: queryKeys.agentHost.scan({ filter: 'all' }),
    });
  });

  it('shows locked desktop-only toast when saving without desktop caps', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        desktop: null,
        initialRouterEntries: ['/settings/agent'],
      },
    );

    // claude-native is the first installed agent; without desktop caps the
    // browser auto-select falls back to it so Save is enabled.
    await waitFor(() =>
      expect(screen.getByTestId('agent-card-claude-native')).toBeInTheDocument(),
    );

    // Browser falls back to first installed so Save is enabled.
    await waitFor(() => {
      expect(screen.getByTestId('settings-save-agent')).not.toBeDisabled();
    });

    await user.click(screen.getByTestId('settings-save-agent'));

    expect(await screen.findByText('Save agent on desktop')).toBeInTheDocument();
    expect(
      await screen.findByText(
        'Open the Nexus desktop app to change your local agent.',
      ),
    ).toBeInTheDocument();
  });

  it('AC-P2-6: Settings uses the same default-grid pipeline as Setup (no fork)', async () => {
    // Smoke: Settings mounts AgentPicker with `defaultGridEntries` output, so an
    // installed curated agent renders in the primary grid (visible without the
    // More toggle) in priority order — the same contract SetupStepAgent uses.
    const agents = [
      {
        name: 'Claude',
        registry_agent_id: 'claude-native',
        installed: true,
        version: '1.0.0',
      },
      {
        name: 'Cursor',
        registry_agent_id: 'cursor',
        installed: true,
      },
    ];
    useHandlers(scanHandler(agents), creatorsHandler());

    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    // codex slot is absent from this scan; claude-native (priority 1) and
    // cursor (priority 2) are both installed-curated → default grid, ordered by
    // priority: claude-native before cursor. No More toggle (both installed).
    const grid = await screen.findByTestId('agent-picker-grid');
    const cardIds = Array.from(
      grid.querySelectorAll<HTMLElement>('li > div[data-testid^="agent-card-"]'),
    ).map((el) => el.dataset.testid);
    expect(cardIds[0]).toBe('agent-card-claude-native');
    expect(cardIds[1]).toBe('agent-card-cursor');
    // No More toggle — both agents are in the default grid.
    expect(screen.queryByTestId('agent-picker-more')).toBeNull();
  });
});
