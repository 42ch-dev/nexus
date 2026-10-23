/**
 * Harness agent-profile bar (plan v1220 T2) — the browser save affordance over
 * the Task 1 service surface. Success is one explicit `已保存`; a failed save is
 * inline (`保存失败：…`) and the previously saved profile stays displayed.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { delay, http, HttpResponse } from 'msw';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ProfileBar } from '@/components/harness/profile-bar';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

function installedAgents() {
  return {
    agents: [
      {
        name: 'claude-code',
        registry_agent_id: 'claude',
        launch_command: '/usr/local/bin/claude',
        installed: true,
        version: '1.0.0',
      },
      {
        name: 'codex-cli',
        registry_agent_id: 'codex',
        launch_command: '/usr/local/bin/codex',
        installed: true,
        version: '0.9.0',
      },
    ],
  };
}

function renderBar() {
  return renderInApp(<ProfileBar />, {
    client: new BrowserClient(),
    activeCreatorId: 'creator-a',
  });
}

beforeEach(async () => {
  window.localStorage.clear();
  // LocaleProvider resolves from this preference; without it jsdom's en-US wins.
  window.localStorage.setItem('nexus-web-locale', 'zh-CN');
  await i18n.changeLanguage('zh-CN');
});

describe('ProfileBar', () => {
  it('shows the saved profile and saves the selected agent to the service', async () => {
    const puts: unknown[] = [];
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json({ profile: { name: 'claude-code' } }),
      ),
      http.put('/v1/daemon/agent-host/profile', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        puts.push(body);
        return HttpResponse.json({ profile: body });
      }),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json(installedAgents())),
    );

    renderBar();

    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );

    const select = screen.getByRole('combobox', { name: 'Agent profile' });
    await userEvent.selectOptions(select, 'codex-cli');
    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-saved')).toHaveTextContent('已保存');
    expect(puts).toEqual([
      { name: 'codex-cli', launch_command: '/usr/local/bin/codex' },
    ]);
    expect(screen.queryByTestId('harness-profile-save-error')).not.toBeInTheDocument();
  });

  it('renders a save failure inline and keeps the prior saved profile', async () => {
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json({ profile: { name: 'claude-code' } }),
      ),
      http.put('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'config write failed' } },
          { status: 500 },
        ),
      ),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json(installedAgents())),
    );

    renderBar();

    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Agent profile' }),
      'codex-cli',
    );
    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-save-error')).toHaveTextContent(
      '保存失败：config write failed',
    );
    // No success state, and the stored profile is still the prior one.
    expect(screen.queryByTestId('harness-profile-saved')).not.toBeInTheDocument();
    expect(screen.getByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );
  });

  it('shows the loading state while the profile read is pending', async () => {
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', async () => {
        await delay(50);
        return HttpResponse.json({ profile: { name: 'claude-code' } });
      }),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json(installedAgents())),
    );

    renderBar();

    expect(screen.getByTestId('harness-profile-loading')).toHaveTextContent('正在加载…');
    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );
  });

  it('desktop saves through the IPC path and never calls the service', async () => {
    const setAgentProfile = vi.fn().mockResolvedValue(undefined);
    let puts = 0;
    useHandlers(
      http.put('/v1/daemon/agent-host/profile', () => {
        puts += 1;
        return HttpResponse.json({ profile: null });
      }),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json(installedAgents())),
    );
    const desktop = {
      getAgentProfile: () =>
        Promise.resolve({ name: 'claude-code', launchCommand: '/usr/local/bin/claude' }),
      setAgentProfile,
    } as unknown as DesktopCapabilities;

    renderInApp(<ProfileBar />, {
      client: new BrowserClient(),
      desktop,
      activeCreatorId: 'creator-a',
    });

    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Agent profile' }),
      'codex-cli',
    );
    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-saved')).toHaveTextContent('已保存');
    expect(setAgentProfile).toHaveBeenCalledWith('codex-cli', '/usr/local/bin/codex');
    // Transport coherence: the desktop save never writes the service store.
    expect(puts).toBe(0);
  });

  it('keeps the stored custom launch command when the selected agent reports none', async () => {
    const puts: unknown[] = [];
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json({
          profile: { name: 'claude-code', launch_command: '/custom/claude --flag' },
        }),
      ),
      http.put('/v1/daemon/agent-host/profile', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        puts.push(body);
        return HttpResponse.json({ profile: body });
      }),
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [{ name: 'claude-code', installed: true, launch_command: null }],
        }),
      ),
    );

    renderBar();

    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );
    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-saved')).toHaveTextContent('已保存');
    expect(puts).toEqual([
      { name: 'claude-code', launch_command: '/custom/claude --flag' },
    ]);
  });
});
