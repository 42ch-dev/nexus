/**
 * Workspace chip row (plan v1219 T3) — product-behaviour tests:
 *
 *  - One chip per workspace `display_name`; a fresh creator sees exactly one
 *    chip (`默认工作区`).
 *  - The active chip reflects `active_workspace_slug`.
 *  - Switching calls set-active, invalidates workspace-scoped queries (the
 *    list refetches), and surfaces the § Copy switch toast.
 *  - No `＋ 新建工作区` chip this iteration (D23) and the component never calls
 *    createWorkspace (POST /v1/daemon/workspaces stays unrequested).
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { CreatorHubPage } from '@/pages/creator-hub-page';
import { DeveloperHubPage } from '@/pages/developer-hub-page';
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

/** Mutable service state so a set-active flips what the active route serves. */
let activeSlug = 'default';
let listRequests = 0;
let setActiveRequests: unknown[] = [];
let createRequests = 0;

function useWorkspaceHandlers(items: unknown[] = [DEFAULT_WS, NOVEL_WS]) {
  useHandlers(
    http.get('/v1/daemon/workspaces', () => {
      listRequests += 1;
      return HttpResponse.json({
        items,
        pagination: { limit: 100, has_more: false },
      });
    }),
    http.get('/v1/daemon/workspace', () =>
      HttpResponse.json({
        creator_id: 'creator-a',
        workspace_slug: activeSlug,
        operational_dir: '/home/creator/.nexus',
      }),
    ),
    http.post('/v1/daemon/workspace/active', async ({ request }) => {
      const body = (await request.json()) as { workspace_slug: string };
      setActiveRequests.push(body);
      activeSlug = body.workspace_slug;
      return HttpResponse.json({ creator_id: 'creator-a', workspace_slug: body.workspace_slug });
    }),
    // Create stays unexposed this iteration (D23): any create call is recorded
    // so the test can assert it never happens.
    http.post('/v1/daemon/workspaces', () => {
      createRequests += 1;
      return HttpResponse.json({ state_db_path: '/nope' }, { status: 501 });
    }),
  );
}

function renderChips() {
  return renderInApp(
    <WorkspaceProvider>
      <WorkspaceChips />
    </WorkspaceProvider>,
    { client: new BrowserClient(), activeCreatorId: 'creator-a' },
  );
}

describe('WorkspaceChips', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    activeSlug = 'default';
    listRequests = 0;
    setActiveRequests = [];
    createRequests = 0;
    await i18n.changeLanguage('zh-CN');
  });

  it('fresh creator sees exactly one chip, 默认工作区', async () => {
    useWorkspaceHandlers([DEFAULT_WS]);
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

  it('reflects active_workspace_slug on the active chip', async () => {
    useWorkspaceHandlers();
    renderChips();
    const active = await screen.findByRole('button', { name: '默认工作区' });
    expect(active).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: '我的小说' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('switching calls set-active, refetches workspace content, and toasts the switch copy', async () => {
    useWorkspaceHandlers();
    const user = userEvent.setup();
    renderChips();

    await screen.findByRole('button', { name: '默认工作区' });
    const listsBefore = listRequests;
    await user.click(screen.getByRole('button', { name: '我的小说' }));

    await waitFor(() =>
      expect(setActiveRequests).toEqual([{ workspace_slug: 'novel' }]),
    );
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

  it('mounts on the creator content surface (creator hub page)', async () => {
    useWorkspaceHandlers();
    renderInApp(<CreatorHubPage />, {
      client: new BrowserClient(),
      activeCreatorId: 'creator-a',
    });
    await screen.findByTestId('workspace-chips');
    expect(await screen.findByRole('button', { name: '默认工作区' })).toBeInTheDocument();
  });

  it('never mounts in the developer shell (developer hub page)', () => {
    useWorkspaceHandlers();
    renderInApp(<DeveloperHubPage />, {
      client: new BrowserClient(),
      activeCreatorId: 'creator-a',
    });
    expect(screen.queryByTestId('workspace-chips')).not.toBeInTheDocument();
  });
});
