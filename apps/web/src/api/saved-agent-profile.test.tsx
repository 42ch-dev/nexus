/**
 * Saved-agent-profile seam (v1220 T2) — the transport-aware retry contract.
 *
 * The desktop IPC profile query is only enabled when a desktop bridge exists.
 * Retry must re-run the transport that actually backs the current build: the
 * enabled IPC query on desktop, the synchronous browser snapshot otherwise.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { useSavedAgentProfile } from '@/api/queries';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { queryKeys } from '@/lib/nexus/query-keys';
import { makeQueryClient, renderInApp } from '@/test/test-providers';

const CREATOR_ID = 'creator-a';
const STORAGE_KEY = `nexus-agent-profile:${CREATOR_ID}`;

function ProfileProbe() {
  const { profile, isLoading, error, refetch } = useSavedAgentProfile();
  return (
    <div>
      <span data-testid="profile-name">{profile?.name ?? 'none'}</span>
      <span data-testid="profile-loading">{String(isLoading)}</span>
      <span data-testid="profile-error">{error ? error.message : 'none'}</span>
      <button type="button" onClick={refetch}>
        retry-profile
      </button>
    </div>
  );
}

describe('useSavedAgentProfile', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('browser retry re-reads the snapshot without forcing the disabled IPC query', async () => {
    const queryClient = makeQueryClient();
    renderInApp(<ProfileProbe />, {
      client: new BrowserClient(),
      desktop: null,
      activeCreatorId: CREATOR_ID,
      queryClient,
    });

    expect(screen.getByTestId('profile-name')).toHaveTextContent('none');
    expect(screen.getByTestId('profile-loading')).toHaveTextContent('false');

    // A snapshot written after mount is picked up by the retry re-read.
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ name: 'snapshot-agent' }));
    await userEvent.click(screen.getByRole('button', { name: 'retry-profile' }));

    expect(screen.getByTestId('profile-name')).toHaveTextContent('snapshot-agent');
    expect(screen.getByTestId('profile-error')).toHaveTextContent('none');

    // The disabled desktop query was never fetched: it stays pending + idle.
    const ipcQuery = queryClient
      .getQueryCache()
      .find({ queryKey: queryKeys.agentProfile.detail() });
    expect(ipcQuery?.state.fetchStatus).toBe('idle');
    expect(ipcQuery?.state.dataUpdateCount).toBe(0);
    expect(ipcQuery?.state.errorUpdateCount).toBe(0);
  });

  it('desktop retry re-runs the IPC profile read', async () => {
    const queryClient = makeQueryClient();
    const getAgentProfile = vi
      .fn()
      .mockRejectedValueOnce(new Error('ipc offline'))
      .mockResolvedValue({ name: 'ipc-agent' });
    renderInApp(<ProfileProbe />, {
      client: new BrowserClient(),
      desktop: { getAgentProfile } as unknown as DesktopCapabilities,
      activeCreatorId: CREATOR_ID,
      queryClient,
    });

    expect(await screen.findByText('ipc offline')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'retry-profile' }));

    expect(await screen.findByTestId('profile-name')).toHaveTextContent('ipc-agent');
    expect(screen.getByTestId('profile-error')).toHaveTextContent('none');
    expect(getAgentProfile).toHaveBeenCalledTimes(2);
  });
});
