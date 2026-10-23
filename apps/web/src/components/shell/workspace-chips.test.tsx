/**
 * Workspace chip row + create flow (plan v1219 T3; create added by v1220 P3-T3)
 * — product-behaviour tests:
 *
 *  - One workspace chip per `display_name`; a fresh creator sees exactly one,
 *    plus the trailing 「＋新建工作区」 chip.
 *  - The active chip reflects the daemon's active workspace, matched by
 *    `creator_id` + slug (two creators can both own a `default` workspace).
 *  - A retained 409 `uninitialized` (fresh home, pre-first-selection) maps to
 *    "no active workspace" (no error toast).
 *  - Switching calls set-active, invalidates workspace-scoped queries, and
 *    surfaces the switch toast.
 *  - No create request fires until the panel is opened and submitted; the panel
 *    derives the root preview from the display name, disables submit on an
 *    empty name, and 「创建并切换」 creates then activates.
 *  - A refused create keeps the panel + draft with one inline
 *    `创建失败：{reason}` line, leaves the chip row (and the active workspace)
 *    untouched, and never toasts.
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

/** Mutable service state so set-active flips what the active route serves. */
let activeCreator = 'creator-a';
let activeSlug = 'default';
let activeStatus = 200;
let listRequests = 0;
let listCreatorIds: string[] = [];
let setActiveRequests: unknown[] = [];
let createRequests: Array<Record<string, unknown>> = [];
let createdItems: unknown[] = [];
let createConflict = false;
let activateFailures = 0;
let createGate: Promise<void> | null = null;
let releaseCreate: (() => void) | null = null;

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
        items: [...rows, ...createdItems],
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
      if (activateFailures > 0) {
        activateFailures -= 1;
        return HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'Internal server error' } },
          { status: 500 },
        );
      }
      activeSlug = body.workspace_slug;
      return HttpResponse.json({ creator_id: activeCreator, workspace_slug: body.workspace_slug });
    }),
    http.post('/v1/daemon/workspaces', async ({ request }) => {
      const body = (await request.json()) as Record<string, string>;
      createRequests.push(body);
      if (createGate !== null) await createGate;
      if (createConflict) {
        return HttpResponse.json(
          {
            success: false,
            error: {
              code: 'workspace_conflict',
              message: "workspace conflict: workspace 'taken' already exists",
            },
          },
          { status: 409 },
        );
      }
      const created = {
        creator_id: body.creator_id,
        workspace_slug: body.workspace_slug,
        creative_root: `/home/creator/Nexus/${body.workspace_slug}`,
        display_name: body.display_name,
      };
      createdItems.push(created);
      return HttpResponse.json(
        {
          creator_id: created.creator_id,
          workspace_slug: created.workspace_slug,
          creative_root: created.creative_root,
          operational_dir: `/home/creator/.nexus42/creators/${created.creator_id}/workspaces/${created.workspace_slug}`,
          state_db_path: `/home/creator/.nexus42/creators/${created.creator_id}/workspaces/${created.workspace_slug}/state.db`,
        },
        { status: 201 },
      );
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

async function openCreatePanel() {
  const user = userEvent.setup();
  renderChips();
  await screen.findByTestId('workspace-chips');
  await user.click(screen.getByTestId('workspace-create-chip'));
  await screen.findByTestId('workspace-create-panel');
  return user;
}

describe('WorkspaceChips', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    // LocaleProvider resolves `system` (en in jsdom) and calls
    // changeLanguage on mount; pin the stored preference so the shell copy is
    // the zh-CN one these assertions read.
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    activeCreator = 'creator-a';
    activeSlug = 'default';
    activeStatus = 200;
    listRequests = 0;
    listCreatorIds = [];
    setActiveRequests = [];
    createRequests = [];
    createdItems = [];
    createConflict = false;
    activateFailures = 0;
    createGate = null;
    releaseCreate = null;
    await i18n.changeLanguage('zh-CN');
  });

  it('fresh creator sees exactly one workspace chip plus the create chip', async () => {
    useWorkspaceHandlers({ items: [DEFAULT_WS] });
    renderChips();
    await screen.findByTestId('workspace-chips');
    const chips = screen.getAllByTestId('workspace-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0]).toHaveTextContent('默认工作区');
    expect(screen.getByTestId('workspace-create-chip')).toHaveTextContent('＋ 新建工作区');
  });

  it('renders one chip per workspace display_name', async () => {
    useWorkspaceHandlers();
    renderChips();
    await screen.findByTestId('workspace-chips');
    const chips = screen.getAllByTestId('workspace-chip');
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
    await screen.findByTestId('workspace-chips');
    const chips = screen.getAllByTestId('workspace-chip');
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

    await waitFor(() =>
      expect(setActiveRequests).toEqual([{ workspace_slug: 'novel', creator_id: 'creator-a' }]),
    );
    // Invalidation refires the workspace list (workspace-scoped refetch).
    await waitFor(() => expect(listRequests).toBeGreaterThan(listsBefore));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '我的小说' })).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    );
    expect(await screen.findByText('已切换到工作区「我的小说」')).toBeInTheDocument();
  });

  it('opens the panel on the create chip without any create request', async () => {
    useWorkspaceHandlers();
    renderChips();
    await screen.findByTestId('workspace-chips');
    expect(createRequests).toHaveLength(0);

    await userEvent.setup().click(screen.getByTestId('workspace-create-chip'));

    expect(await screen.findByTestId('workspace-create-panel')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-create-name')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-root-preview')).toBeInTheDocument();
    // The advanced override is collapsed by default.
    const advanced = screen.getByText('高级：更改位置…').closest('details');
    expect(advanced?.open).toBe(false);
    expect(createRequests).toHaveLength(0);
  });

  it('derives the root preview from the display name and blocks an empty name', async () => {
    useWorkspaceHandlers();
    const user = await openCreatePanel();

    const submit = screen.getByTestId('workspace-create-submit');
    const preview = screen.getByTestId('workspace-root-preview');
    expect(submit).toBeDisabled();
    expect(preview).toHaveTextContent('~/Nexus/');

    // Pure-CJK name → deterministic ASCII fallback slug.
    await user.type(screen.getByTestId('workspace-create-name'), '我的小说');
    expect(preview).toHaveTextContent('~/Nexus/ws-20029f24');
    expect(submit).toBeEnabled();

    // ASCII name → slugified path.
    await user.clear(screen.getByTestId('workspace-create-name'));
    await user.type(screen.getByTestId('workspace-create-name'), 'My Novel');
    expect(preview).toHaveTextContent('~/Nexus/my-novel');

    // The advanced override takes over the preview when set.
    await user.type(screen.getByLabelText('内容根目录'), '/data/roots/novel');
    expect(preview).toHaveTextContent('/data/roots/novel');

    await user.clear(screen.getByTestId('workspace-create-name'));
    expect(submit).toBeDisabled();
  });

  it('creates then activates: the chip row refreshes with the new workspace active', async () => {
    useWorkspaceHandlers();
    const user = await openCreatePanel();

    await user.type(screen.getByTestId('workspace-create-name'), '星际拓荒志');
    await user.click(screen.getByTestId('workspace-create-submit'));

    await waitFor(() => expect(createRequests).toHaveLength(1));
    expect(createRequests[0]).toEqual({
      creator_id: 'creator-a',
      workspace_slug: 'ws-748324af',
      display_name: '星际拓荒志',
    });
    await waitFor(() =>
      expect(setActiveRequests).toEqual([
        { creator_id: 'creator-a', workspace_slug: 'ws-748324af' },
      ]),
    );

    // Panel closes on success; the chip row now carries the workspace and
    // reflects it as active.
    await waitFor(() =>
      expect(screen.queryByTestId('workspace-create-panel')).not.toBeInTheDocument(),
    );
    const chip = await screen.findByRole('button', { name: '星际拓荒志' });
    await waitFor(() => expect(chip).toHaveAttribute('aria-pressed', 'true'));
  });

  it('treats a failed activation after a committed create as a retry-activate state', async () => {
    useWorkspaceHandlers();
    activateFailures = 1;
    const user = await openCreatePanel();

    await user.type(screen.getByTestId('workspace-create-name'), '星际拓荒志');
    await user.click(screen.getByTestId('workspace-create-submit'));

    // Activation failed after the create committed: distinct state + copy.
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('切换失败：未切换成功，请重试激活。');
    expect(screen.getByTestId('workspace-create-created')).toBeInTheDocument();

    // The chip row now carries the created workspace, but it is not active.
    const chip = await screen.findByRole('button', { name: '星际拓荒志' });
    expect(chip).toHaveAttribute('aria-pressed', 'false');

    const retry = screen.getByTestId('workspace-create-submit');
    expect(retry).toHaveTextContent('重试激活');
    await user.click(retry);

    await waitFor(() =>
      expect(screen.queryByTestId('workspace-create-panel')).not.toBeInTheDocument(),
    );
    // Only activation re-ran: no second create request (a re-create would 409).
    expect(createRequests).toHaveLength(1);
    expect(setActiveRequests).toHaveLength(2);
    expect(await screen.findByRole('button', { name: '星际拓荒志' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('renders activation-failure copy in en without the create fallback', async () => {
    window.localStorage.setItem('nexus-web-locale', 'en');
    await i18n.changeLanguage('en');
    useWorkspaceHandlers();
    activateFailures = 1;
    const user = await openCreatePanel();

    await user.type(screen.getByTestId('workspace-create-name'), 'Race EN');
    await user.click(screen.getByTestId('workspace-create-submit'));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Switch failed: Switching did not complete');
    // The created-but-not-activated state must never show create-failure copy.
    expect(alert).not.toHaveTextContent(/not created/i);
    expect(screen.getByTestId('workspace-create-created')).toBeInTheDocument();
  });

  it('shows 创建中… and disables the button while creating', async () => {
    useWorkspaceHandlers();
    // `Promise.withResolvers` is outside this app's `es2022` lib target.
    createGate = new Promise<void>((resolve) => {
      releaseCreate = resolve;
    });
    const user = await openCreatePanel();

    await user.type(screen.getByTestId('workspace-create-name'), 'Slow novel');
    await user.click(screen.getByTestId('workspace-create-submit'));

    const submit = screen.getByTestId('workspace-create-submit');
    await waitFor(() => expect(submit).toHaveTextContent('创建中…'));
    expect(submit).toBeDisabled();

    releaseCreate?.();
    await waitFor(() =>
      expect(screen.queryByTestId('workspace-create-panel')).not.toBeInTheDocument(),
    );
  });

  it('keeps the panel open with the draft and one inline error on a refused create', async () => {
    useWorkspaceHandlers();
    createConflict = true;
    const user = await openCreatePanel();

    await user.type(screen.getByTestId('workspace-create-name'), 'Taken');
    await user.click(screen.getByTestId('workspace-create-submit'));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('创建失败：该工作区或目标目录已存在');
    expect(screen.getAllByRole('alert')).toHaveLength(1);
    expect(screen.getByTestId('workspace-create-name')).toHaveValue('Taken');
    // No activation switch and no chip-row change.
    expect(setActiveRequests).toHaveLength(0);
    expect(screen.getAllByTestId('workspace-chip')).toHaveLength(2);
    expect(screen.getByRole('button', { name: '默认工作区' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.queryByText(/已切换到工作区/)).not.toBeInTheDocument();
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
