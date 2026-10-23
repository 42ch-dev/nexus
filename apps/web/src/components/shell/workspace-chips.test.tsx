/**
 * Workspace chip row (plan v1219 T3 + QC fix wave B) — product-behaviour tests:
 *
 *  - One chip per workspace `display_name`; a fresh creator sees exactly one
 *    chip (`默认工作区`).
 *  - The active chip reflects the daemon's active workspace, matched by
 *    `creator_id` + slug (two creators can both own a `default` workspace —
 *    only the active creator's chip lights up) and the list is fetched
 *    creator-scoped.
 *  - A retained 409 `uninitialized` (fresh home, pre-first-selection) maps to
 *    "no active workspace": chips render with none active and no error toast.
 *  - Switching calls set-active, invalidates workspace-scoped queries (the
 *    list refetches), and surfaces the § Copy switch toast.
 *  - No `＋ 新建工作区` chip this iteration (D23) and the component never calls
 *    createWorkspace (POST /v1/daemon/workspaces stays unrequested).
 *  - The hub mounts the World/Work create panel; the developer shell has no
 *    chips (and no workspace queries at all).
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { CreatorHubPage } from '@/pages/creator-hub-page';
import { HubPage } from '@/components/developer/hub-page';
import { WorkspaceProvider } from '@/lib/workspace-context';
import { WorkspaceChips } from '@/components/shell/workspace-chips';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

const DEFAULT_WS = {
  creator_id: 'creator-a',
  workspace_slug: 'default',
  creative_root: '/home/creator/Nexus',
  display_name: '默认工作区',
};
const NOVEL_WS = {
  creator_id: 'creator-a',
  workspace_slug: 'novel',
  creative_root: '/home/creator/Nexus/novel',
  display_name: '我的小说',
};
/** Same slug as creator-a's default — the multi-creator collision case. */
const CREATOR_B_DEFAULT_WS = {
  creator_id: 'creator-b',
  workspace_slug: 'default',
  creative_root: '/home/b/Nexus',
  display_name: 'B 的工作区',
};

/** Mutable service state so a set-active flips what the active route serves. */
let activeCreator = 'creator-a';
let activeSlug = 'default';
let activeStatus = 200;
let listRequests = 0;
let listCreatorIds: string[] = [];
let setActiveRequests: unknown[] = [];
let createRequests = 0;

interface WorkspaceHandlerOptions {
  items?: unknown[];
  /** 200 serves the active workspace; 409 simulates a fresh home. */
  active?: number;
  /** Creator id the active-workspace route reports. */
  activeCreatorId?: string;
}

function useWorkspaceHandlers({ items, active, activeCreatorId }: WorkspaceHandlerOptions = {}) {
  const rows = items ?? [DEFAULT_WS, NOVEL_WS];
  useHandlers(
    http.get('/v1/daemon/workspaces', ({ request }) => {
      listRequests += 1;
      listCreatorIds.push(new URL(request.url).searchParams.get('creator_id') ?? '');
      return HttpResponse.json({
        items: rows,
        pagination: { limit: 100, has_more: false },
      });
    }),
    http.get('/v1/daemon/workspace', () => {
      if (activeStatus === 409) {
        return HttpResponse.json({ error: 'uninitialized' }, { status: 409 });
      }
      return HttpResponse.json({
        creator_id: activeCreator,
        workspace_slug: activeSlug,
        operational_dir: '/home/creator/.nexus',
      });
    }),
    http.post('/v1/daemon/workspace/active', async ({ request }) => {
      const body = (await request.json()) as { workspace_slug: string };
      setActiveRequests.push(body);
      activeSlug = body.workspace_slug;
      return HttpResponse.json({ creator_id: activeCreator, workspace_slug: body.workspace_slug });
    }),
    // Create stays unexposed this iteration (D23): any create call is recorded
    // so the test can assert it never happens.
    http.post('/v1/daemon/workspaces', () => {
      createRequests += 1;
      return HttpResponse.json({ error: 'route_not_migrated' }, { status: 501 });
    }),
  );
  if (active !== undefined) activeStatus = active;
  if (activeCreatorId !== undefined) activeCreator = activeCreatorId;
}

function renderChips(activeCreatorId: string | null = 'creator-a') {
  return renderInApp(
    <WorkspaceProvider>
      <WorkspaceChips />
    </WorkspaceProvider>,
    { client: new BrowserClient(), activeCreatorId },
  );
}

describe('WorkspaceChips', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    activeCreator = 'creator-a';
    activeSlug = 'default';
    activeStatus = 200;
    listRequests = 0;
    listCreatorIds = [];
    setActiveRequests = [];
    createRequests = 0;
    await i18n.changeLanguage('zh-CN');
  });

  it('fresh creator sees exactly one chip, 默认工作区', async () => {
    useWorkspaceHandlers({ items: [DEFAULT_WS] });
    renderChips();
    const chips = await screen.findAllByRole('button');
    expect(chips).toHaveLength(1);
    expect(chips[0]).toHaveTextContent('默认工作区');
  });

  it('renders one chip per workspace display_name', async () => {
    useWorkspaceHandlers();
    renderChips();
    const chips = await screen.findAllByRole('button');
    expect(chips).toHaveLength(2);
    expect(screen.getByText('默认工作区')).toBeInTheDocument();
    expect(screen.getByText('我的小说')).toBeInTheDocument();
  });

  it('reflects the active workspace on the active chip', async () => {
    useWorkspaceHandlers();
    renderChips();
    const active = await screen.findByRole('button', { name: '默认工作区' });
    expect(active).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: '我的小说' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('matches creator_id + slug: with creator-b active, only creator-b’s default chip is active', async () => {
    // Both creators own a `default` workspace; the service receives the
    // creator filter and the client-side match must not light creator-a's chip.
    useWorkspaceHandlers({
      items: [DEFAULT_WS, CREATOR_B_DEFAULT_WS],
      activeCreatorId: 'creator-b',
    });
    renderChips('creator-b');
    await waitFor(() => expect(listCreatorIds).toContain('creator-b'));
    const creatorBChip = await screen.findByRole('button', { name: 'B 的工作区' });
    expect(creatorBChip).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: '默认工作区' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('maps a retained 409 uninitialized to no active chip (no error toast)', async () => {
    useWorkspaceHandlers({ active: 409 });
    renderChips();
    // Chips still render from the list; nothing is active and the query
    // resolves cleanly instead of surfacing the 409 to the global handler.
    const chips = await screen.findAllByRole('button');
    expect(chips).toHaveLength(2);
    for (const chip of chips) {
      expect(chip).toHaveAttribute('aria-pressed', 'false');
    }
  });

  it('switching calls set-active, refetches workspace content, and toasts the switch copy', async () => {
    useWorkspaceHandlers();
    const user = userEvent.setup();
    renderChips();

    await screen.findByRole('button', { name: '默认工作区' });
    const listsBefore = listRequests;
    await user.click(screen.getByRole('button', { name: '我的小说' }));

    await waitFor(() => expect(setActiveRequests).toEqual([{ workspace_slug: 'novel', creator_id: 'creator-a' }]));
    // Invalidation refires the workspace list (workspace-scoped refetch).
    await waitFor(() => expect(listRequests).toBeGreaterThan(listsBefore));
    // The active reflection follows the new active_workspace_slug.
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '我的小说' })).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    );
    expect(await screen.findByText('已切换到工作区「我的小说」')).toBeInTheDocument();
  });

  it('never renders the ＋ 新建工作区 chip or calls createWorkspace', async () => {
    useWorkspaceHandlers();
    const user = userEvent.setup();
    renderChips();

    await screen.findByRole('button', { name: '默认工作区' });
    expect(screen.queryByText(/新建工作区/)).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '我的小说' }));
    await screen.findByText('已切换到工作区「我的小说」');
    expect(createRequests).toBe(0);
  });

  it('mounts chips AND the World/Work create panel on the creator content surface', async () => {
    useWorkspaceHandlers();
    renderInApp(<CreatorHubPage />, {
      client: new BrowserClient(),
      activeCreatorId: 'creator-a',
    });
    await screen.findByTestId('workspace-chips');
    expect(await screen.findByRole('button', { name: '默认工作区' })).toBeInTheDocument();
    // QC W: the sidebar is gone, so the hub itself must carry the create entry.
    expect(await screen.findByTestId('sidebar-create-panel')).toBeInTheDocument();
  });

  it('never mounts in the developer shell (操作台 hub page)', () => {
    useWorkspaceHandlers();
    renderInApp(<HubPage />, {
      client: new BrowserClient(),
      activeCreatorId: 'creator-a',
    });
    expect(screen.queryByTestId('workspace-chips')).not.toBeInTheDocument();
    expect(screen.queryByTestId('sidebar-create-panel')).not.toBeInTheDocument();
  });
});
