/**
 * Harness agent-profile bar (plan v1220 T2) — the browser save affordance over
 * the Task 1 service surface. Success is one explicit `已保存`; a failed save is
 * inline (`保存失败：…`) and the previously saved profile stays displayed.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { delay, http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
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
    // Switching agents with an unedited command field sends no override: the
    // scan default is not user intent (the service preserves any stored one).
    expect(puts).toEqual([{ name: 'codex-cli' }]);
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
    // A scan default is not an override: the IPC save carries no command.
    expect(setAgentProfile).toHaveBeenCalledWith('codex-cli', undefined);
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
    // The stored custom command is displayed, not the (absent) scan default.
    expect(screen.getByRole('textbox', { name: '启动命令' })).toHaveValue(
      '/custom/claude --flag',
    );

    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-saved')).toHaveTextContent('已保存');
    // An unedited stored command is omitted, so the service preserves it.
    expect(puts).toEqual([{ name: 'claude-code' }]);
  });

  it('does not send a differing scan default as an override', async () => {
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
          agents: [
            { name: 'claude-code', installed: true, launch_command: '/usr/local/bin/claude' },
          ],
        }),
      ),
    );

    renderBar();

    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );
    // The field shows the stored custom command; the scan default never wins.
    expect(screen.getByRole('textbox', { name: '启动命令' })).toHaveValue(
      '/custom/claude --flag',
    );

    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-saved')).toHaveTextContent('已保存');
    expect(puts).toEqual([{ name: 'claude-code' }]);
  });

  it('sends a user-edited launch command as an override', async () => {
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
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            { name: 'claude-code', installed: true, launch_command: '/usr/local/bin/claude' },
          ],
        }),
      ),
    );

    renderBar();

    const commandField = await screen.findByRole('textbox', { name: '启动命令' });
    await waitFor(() => expect(commandField).toHaveValue('/usr/local/bin/claude'));
    await userEvent.clear(commandField);
    await userEvent.type(commandField, '/my/claude --model opus');

    await userEvent.click(screen.getByRole('button', { name: '保存到 service' }));

    expect(await screen.findByTestId('harness-profile-saved')).toHaveTextContent('已保存');
    expect(puts).toEqual([
      { name: 'claude-code', launch_command: '/my/claude --model opus' },
    ]);
  });

  it('lets the saved profile win over the scan default when it arrives late', async () => {
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', async () => {
        await delay(50);
        return HttpResponse.json({ profile: { name: 'codex-cli' } });
      }),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json(installedAgents())),
    );

    renderBar();

    const select = screen.getByRole('combobox', { name: 'Agent profile' });
    const save = screen.getByRole('button', { name: '保存到 service' });

    // Scan-first: the recommended default shows, but Save is gated while the
    // persisted choice is still being read.
    await waitFor(() => expect(select).toHaveValue('claude-code'));
    expect(save).toBeDisabled();

    // The saved profile applies on arrival and wins over the scan default.
    await waitFor(() => expect(select).toHaveValue('codex-cli'));
    expect(save).toBeEnabled();
    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：codex-cli',
    );
  });
});
