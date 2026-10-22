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

/**
 * Browser-shaped stub plus a call log, so finish-path ordering is observable:
 * creator bootstrap → workspace selection → world creation, with the derived
 * gate re-probed after selection (`probe` entries) and no client-side marker.
 */
function makeClient({
  initialized = true,
  worldFails = false,
  selectionFails = false,
}: { initialized?: boolean; worldFails?: boolean; selectionFails?: boolean } = {}) {
  const calls: string[] = [];
  let workspaceInitialized = initialized;
  const client = {
    runtimeStatus: () => {
      calls.push('probe');
      return Promise.resolve({ workspace_initialized: workspaceInitialized });
    },
    scanAgents: () => Promise.resolve({ agents: AGENTS }),
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
      // The service promotes the shell in process on selection (D21).
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
  await user.click(screen.getByRole('button', { name: '下一步' }));
}

async function reachWorldStep(user: UserEvent) {
  await advanceIdentity(user, 'content-creator');
  await screen.findByRole('heading', { name: '你的 AI 搭档' });
  await waitFor(() => expect(screen.getByRole('button', { name: '下一步' })).toBeEnabled());
  await user.click(screen.getByRole('button', { name: '下一步' }));
  await screen.findByRole('heading', { name: '创建你的第一个世界' });
}

describe('WizardShell', () => {
  beforeEach(() => window.localStorage.clear());

  it('starts on the identity step with 下一步 gated by the choice', () => {
    renderShell();

    expect(screen.getByRole('heading', { name: '你的 AI 创作工作室' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一步' })).toBeDisabled();
  });

  it('persists the developer entrance, which selects the developer rail registry', async () => {
    const user = userEvent.setup();
    renderShell();

    await advanceIdentity(user, 'developer');

    await waitFor(() =>
      expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('developer'),
    );
    expect(await screen.findByRole('heading', { name: 'Agent 与工作区' })).toBeInTheDocument();
  });

  it('persists the creator entrance and advances to the AI partner step', async () => {
    const user = userEvent.setup();
    renderShell();

    await advanceIdentity(user, 'content-creator');

    await waitFor(() =>
      expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('content-creator'),
    );
    expect(await screen.findByRole('heading', { name: '你的 AI 搭档' })).toBeInTheDocument();
  });

  it('returns from the second step to the identity choice', async () => {
    const user = userEvent.setup();
    renderShell();

    await advanceIdentity(user, 'content-creator');
    await screen.findByRole('heading', { name: '你的 AI 搭档' });

    await user.click(screen.getByRole('button', { name: '返回' }));

    expect(screen.getByRole('heading', { name: '你的 AI 创作工作室' })).toBeInTheDocument();
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
    await screen.findByRole('heading', { name: '你的 AI 搭档' });

    expect(calls.filter((call) => call === 'creator')).toHaveLength(1);
    expect(window.localStorage.getItem('nexus:activeCreatorId')).toBe('creator-new');
  });

  it('finishes the creator path in order: creator → selection → world → World Brief', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false });
    renderShell({ client });

    await reachWorldStep(user);
    expect(screen.getByLabelText('世界名称')).toHaveValue('灰烬王座');
    await user.click(screen.getByRole('button', { name: '创建并进入' }));

    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/world-1?layer=brief'),
    );
    // No workspace-create call exists anywhere in this path (compass D25).
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select', 'world']);
    // The gate flips by re-probing the promoted service, never by a marker.
    expect(calls.slice(calls.indexOf('select') + 1)).toContain('probe');
    expect(screen.getByText('🎉 世界「灰烬王座」已创建')).toBeInTheDocument();
  });

  it('reuses an existing creator instead of creating another one', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient();
    renderShell({ client, activeCreatorId: 'creator-a' });

    await reachWorldStep(user);
    await user.click(screen.getByRole('button', { name: '创建并进入' }));

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
    await user.click(screen.getByRole('button', { name: '创建并进入' }));

    expect(
      await screen.findByText('创建世界失败：daemon refused。请重试或更换名称。'),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '创建你的第一个世界' })).toBeInTheDocument();
    expect(screen.getByLabelText('世界名称')).toHaveValue('远星号');
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select', 'world']);
  });

  it('reports a workspace-selection failure at the shell stage, not as a world failure', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false, selectionFails: true });
    renderShell({ client });

    await reachWorldStep(user);
    await user.click(screen.getByRole('button', { name: '创建并进入' }));

    expect(await screen.findByText('无法选择默认工作区')).toBeInTheDocument();
    expect(screen.queryByText(/创建世界失败/)).not.toBeInTheDocument();
    // Still in the step with the draft; no world was created and nothing navigated.
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(screen.getByLabelText('世界名称')).toHaveValue('灰烬王座');
    expect(screen.getByRole('button', { name: '创建并进入' })).toBeEnabled();
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select']);
  });

  it('finishes the developer path after workspace selection and lands in the Hub', async () => {
    const user = userEvent.setup();
    const { client, calls } = makeClient({ initialized: false });
    renderShell({ client });

    await advanceIdentity(user, 'developer');
    await screen.findByRole('heading', { name: 'Agent 与工作区' });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '进入开发台' })).toBeEnabled(),
    );
    await user.click(screen.getByRole('button', { name: '进入开发台' }));

    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/developer'));
    expect(calls.filter((call) => call !== 'probe')).toEqual(['creator', 'select']);
  });
});
