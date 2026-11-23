import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { UserEvent } from '@testing-library/user-event';
import { useLocation } from 'react-router';

import { WizardShell } from './wizard-shell';
import { renderInApp } from '@/test/test-providers';
import { ENTRANCE_STORAGE_KEY } from '@/lib/entrance-context';
import type { NexusClient } from '@/lib/nexus';

const AGENTS = [
  {
    name: 'Claude Code',
    registry_agent_id: 'claude',
    launch_command: '/usr/local/bin/claude',
    installed: true,
    version: '2.1.4',
  },
];

function makeClient({
  initialized = true,
  worldFails = false,
  selectionFails = false,
  gateProbeFails = false,
  profileReadFails = false,
}: {
  initialized?: boolean;
  worldFails?: boolean;
  selectionFails?: boolean;
  /** Reject the SECOND runtime-status probe — the finish-path gate re-probe. */
  gateProbeFails?: boolean;
  /** Reject the browser agent-profile read (service unavailable). */
  profileReadFails?: boolean;
} = {}) {
  const calls: string[] = [];
  let workspaceInitialized = initialized;
  let probeCount = 0;
  const client = {
    runtimeStatus: () => {
      calls.push('probe');
      probeCount += 1;
      if (gateProbeFails && probeCount > 1) {
        return Promise.reject(new Error('gate probe failed'));
      }
      return Promise.resolve({ workspace_initialized: workspaceInitialized });
    },
    scanAgents: () => Promise.resolve({ agents: AGENTS }),
    getAgentProfile: () =>
      profileReadFails
        ? Promise.reject(new Error('profile read failed'))
        : Promise.resolve({ profile: null }),
    saveAgentProfile: (profile: { name: string; launch_command?: string }) =>
      Promise.resolve({ profile }),
    getActiveWorkspace: () =>
      Promise.resolve({
        creator_id: 'creator-a',
        workspace_slug: 'default',
        operational_dir: '/tmp/nexus',
      }),
    createCreator: (request: { display_name: string }) => {
      calls.push('creator');
      return Promise.resolve({ creator_id: 'creator-new', display_name: request.display_name });
    },
    setActiveWorkspace: (request: { creator_id?: string; workspace_slug: string }) => {
      calls.push('select');
      if (selectionFails) return Promise.reject(new Error('workspace selection failed'));
      workspaceInitialized = true;
      return Promise.resolve({
        creator_id: request.creator_id ?? 'creator-new',
        workspace_slug: request.workspace_slug,
      });
    },
    createWorld: (request: { title: string }) => {
      calls.push('world');
      if (worldFails) return Promise.reject(new Error('daemon refused'));
      return Promise.resolve({ world_id: 'world-1', status: 'active', title: request.title });
    },
  } as unknown as NexusClient;
  return { client, calls };
}

/** Reports the router location so navigation (the wizard's exit signal) is assertable. */
function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{`${location.pathname}${location.search}`}</div>;
}

function renderShell(
  { client, activeCreatorId }: { client: NexusClient; activeCreatorId?: string | null } = {
    client: makeClient().client,
  },
) {
  return renderInApp(
    <>
      <WizardShell />
      <LocationProbe />
    </>,
    { client, activeCreatorId, initialRouterEntries: ['/setup'] },
  );
}

async function advanceIdentity(user: UserEvent, option: string) {
  await user.click(screen.getByTestId(`identity-option-${option}`));
  await user.click(screen.getByRole('button', { name: 'Next' }));
}

async function reachWorldStep(user: UserEvent) {
  await advanceIdentity(user, 'content-creator');
  await screen.findByRole('heading', { name: 'Your AI Partner' });
  await waitFor(() => expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled());
  await user.click(screen.getByRole('button', { name: 'Next' }));
  await screen.findByRole('heading', { name: 'Create Your First World' });
}

describe('WizardShell', () => {
  beforeEach(() => window.localStorage.clear());

  it('starts on the identity step with Next gated by the choice', () => {
    renderShell();

    expect(screen.getByRole('heading', { name: 'Your AI Creative Studio' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
  });

  it('persists the developer entrance, which selects the developer rail registry', async () => {
    const user = userEvent.setup();
    renderShell();

    await advanceIdentity(user, 'developer');

    await waitFor(() =>
      expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('developer'),
    );
    expect(await screen.findByRole('heading', { name: 'Agent and Workspace' })).toBeInTheDocument();
  });

  it('persists the creator entrance and advances to the AI partner step', async () => {
    const user = userEvent.setup();
    renderShell();

    await advanceIdentity(user, 'content-creator');

    await waitFor(() =>
      expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('content-creator'),
    );
    expect(await screen.findByRole('heading', { name: 'Your AI Partner' })).toBeInTheDocument();
  });

  it('returns from the second step to the identity choice', async () => {
    const user = userEvent.setup();
    renderShell();

    await advanceIdentity(user, 'content-creator');
    await screen.findByRole('heading', { name: 'Your AI Partner' });

    await user.click(screen.getByRole('button', { name: 'Back' }));

    expect(screen.getByRole('heading', { name: 'Your AI Creative Studio' })).toBeInTheDocument();
    expect(screen.getByTestId('identity-option-content-creator')).toHaveAttribute(
      'aria-checked',
      'true',
    );
  });

  it('bootstraps the creator once so the agent choice has a persistence key', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient();
    renderShell({ client });

    await advanceIdentity(user, 'content-creator');
    await screen.findByRole('heading', { name: 'Your AI Partner' });

    expect(calls.filter((call) => call === 'creator')).toHaveLength(1);
    expect(window.localStorage.getItem('nexus:activeCreatorId')).toBe('creator-new');
  });

  it('reaches the world step when the saved-profile read fails', async () => {
    const user = userEvent.setup();
    const { client } = makeClient({ profileReadFails: true });
    renderShell({ client });

    await reachWorldStep(user);
  });

  it('finishes the creator path in order: creator → selection → world → World Brief', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false });
    renderShell({ client });

    await reachWorldStep(user);
    expect(screen.getByLabelText('World name')).toHaveValue('Throne of Ashes');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/world-1?layer=brief'),
    );
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select', 'world']);
    expect(calls.slice(calls.indexOf('select') + 1)).toContain('probe');
    expect(screen.getByText('🎉 World “Throne of Ashes” created')).toBeInTheDocument();
  });

  it('reuses an existing creator instead of creating another one', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient();
    renderShell({ client, activeCreatorId: 'creator-a' });

    await reachWorldStep(user);
    await user.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/world-1?layer=brief'),
    );
    expect(calls.filter((call) => call !== 'probe')).toEqual(['select', 'world']);
  });

  it('keeps the world step with the draft and no exit signal when creation fails', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false, worldFails: true });
    renderShell({ client });

    await reachWorldStep(user);
    await user.click(screen.getByTestId('world-template-scifi'));
    await user.click(screen.getByRole('button', { name: 'Create' }));

    expect(
      await screen.findByText('Could not create the World: daemon refused. Try again or choose a different name.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Create Your First World' })).toBeInTheDocument();
    expect(screen.getByLabelText('World name')).toHaveValue('Farstar');
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select', 'world']);
  });

  it('reports a workspace-selection failure at the shell stage, not as a world failure', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false, selectionFails: true });
    renderShell({ client });

    await reachWorldStep(user);
    await user.click(screen.getByRole('button', { name: 'Create' }));

    expect(await screen.findByText('Could not select the default workspace')).toBeInTheDocument();
    expect(screen.queryByText(/Could not create the World/)).not.toBeInTheDocument();
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(screen.getByLabelText('World name')).toHaveValue('Throne of Ashes');
    expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled();
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select']);
  });

  it('reports a gate re-probe failure as a status-refresh failure, not a selection failure', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false, gateProbeFails: true });
    renderShell({ client });

    await reachWorldStep(user);
    await user.click(screen.getByRole('button', { name: 'Create' }));

    expect(await screen.findByText('Could not refresh setup status')).toBeInTheDocument();
    expect(screen.getByText('gate probe failed. Try again.')).toBeInTheDocument();
    expect(screen.queryByText('Could not select the default workspace')).not.toBeInTheDocument();
    expect(screen.queryByText(/Could not create the World/)).not.toBeInTheDocument();
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(screen.getByLabelText('World name')).toHaveValue('Throne of Ashes');
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select']);
  });

  it('finishes the developer path after workspace selection and lands in the Hub', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false });
    renderShell({ client });

    await advanceIdentity(user, 'developer');
    await screen.findByRole('heading', { name: 'Agent and Workspace' });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Finish' })).toBeEnabled(),
    );
    await user.click(screen.getByRole('button', { name: 'Finish' }));

    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/developer'));
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select']);
  });
});
