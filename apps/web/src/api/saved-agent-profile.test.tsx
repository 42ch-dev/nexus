/**
 * Saved-agent-profile seam (v1220 T2, service-backed in T1) — transport-aware
 * reads for the shared profile seam.
 *
 * Browser: the daemon-global service profile is authoritative; a null service
 * profile falls back ONCE to the legacy per-creator localStorage snapshot and
 * migrates it with a best-effort PUT. Desktop: the IPC profile is unchanged.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { useSavedAgentProfile } from '@/api/queries';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

const CREATOR_ID = 'creator-a';
const STORAGE_KEY = `nexus-agent-profile:${CREATOR_ID}`;

function ProfileProbe() {
  const { profile, isLoading, error, refetch } = useSavedAgentProfile();
  return (
    <div>
      <span data-testid="profile-name">{profile?.name ?? 'none'}</span>
      <span data-testid="profile-command">{profile?.launchCommand ?? 'none'}</span>
      <span data-testid="profile-loading">{String(isLoading)}</span>
      <span data-testid="profile-error">{error ? error.message : 'none'}</span>
      <button type="button" onClick={refetch}>
        retry-profile
      </button>
    </div>
  );
}

function renderProbe(desktop: DesktopCapabilities | null = null) {
  return renderInApp(<ProfileProbe />, {
    client: new BrowserClient(),
    desktop,
    activeCreatorId: CREATOR_ID,
  });
}

describe('useSavedAgentProfile', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('browser reads the service profile and retry re-runs the service read', async () => {
    let reads = 0;
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', () => {
        reads += 1;
        return HttpResponse.json({
          profile: { name: 'service-agent', launch_command: '/bin/service-agent' },
        });
      }),
    );

    renderProbe();

    expect(await screen.findByText('service-agent')).toBeInTheDocument();
    expect(screen.getByTestId('profile-command')).toHaveTextContent('/bin/service-agent');
    expect(screen.getByTestId('profile-loading')).toHaveTextContent('false');
    expect(screen.getByTestId('profile-error')).toHaveTextContent('none');

    await userEvent.click(screen.getByRole('button', { name: 'retry-profile' }));

    expect(await screen.findByText('service-agent')).toBeInTheDocument();
    expect(reads).toBe(2);
  });

  it('migrates the legacy creator snapshot with one PUT, then stops consulting it', async () => {
    let stored: Record<string, unknown> | null = null;
    const puts: unknown[] = [];
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', () => HttpResponse.json({ profile: stored })),
      http.put('/v1/daemon/agent-host/profile', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        puts.push(body);
        stored = body;
        return HttpResponse.json({ profile: stored });
      }),
    );
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ name: 'snapshot-agent' }));

    renderProbe();

    expect(await screen.findByText('snapshot-agent')).toBeInTheDocument();
    // The legacy snapshot is not written back; it is migrated to the service.
    expect(puts).toEqual([{ name: 'snapshot-agent' }]);

    // A later read is answered by the service, so a changed legacy value is
    // neither consulted nor re-written.
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ name: 'stale-agent' }));
    await userEvent.click(screen.getByRole('button', { name: 'retry-profile' }));

    expect(await screen.findByText('snapshot-agent')).toBeInTheDocument();
    expect(screen.queryByText('stale-agent')).not.toBeInTheDocument();
    expect(puts).toHaveLength(1);
  });

  it('keeps the legacy snapshot when the migration PUT fails', async () => {
    useHandlers(
      http.get('/v1/daemon/agent-host/profile', () => HttpResponse.json({ profile: null })),
      http.put('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'write failed' } },
          { status: 500 },
        ),
      ),
    );
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ name: 'snapshot-agent' }));

    renderProbe();

    expect(await screen.findByText('snapshot-agent')).toBeInTheDocument();
    // Best-effort migration: the snapshot stays for the next load.
    expect(window.localStorage.getItem(STORAGE_KEY)).not.toBeNull();
  });

  it('desktop retry re-runs the IPC profile read', async () => {
    const getAgentProfile = vi
      .fn()
      .mockRejectedValueOnce(new Error('ipc offline'))
      .mockResolvedValue({ name: 'ipc-agent' });
    renderProbe({ getAgentProfile } as unknown as DesktopCapabilities);

    expect(await screen.findByText('ipc offline')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'retry-profile' }));

    expect(await screen.findByTestId('profile-name')).toHaveTextContent('ipc-agent');
    expect(screen.getByTestId('profile-error')).toHaveTextContent('none');
    expect(getAgentProfile).toHaveBeenCalledTimes(2);
  });
});
