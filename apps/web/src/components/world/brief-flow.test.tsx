/**
 * Brief flow tests (plan v1219-world-module T2) — empty state → inline
 * editor → saved card over the World app surface, locking the acceptance
 * contract:
 *
 *   - Empty state shows the final prompt + example copy and the
 *     `+ 写 Brief` action; clicking it mounts the editor.
 *   - `保存 Brief` is disabled while the trimmed text is empty (no dialog,
 *     no toast).
 *   - Save creates through `kb/patch-entity` with the create-on-absent
 *     convention (`kb_<uuid>` id, `expected_version: 0`, `block_type: 'era'`,
 *     `body.attributes.world_summary`) and renders the saved card
 *     (`BRIEF · 刚刚`), the success toast, and the `世界已就绪` follow-up
 *     card without a reload (graph invalidation on settle).
 *   - Save failure shows the one actionable inline message and keeps the
 *     draft; 422 renders `validation_summary.errors[]` as the reason; a
 *     create 409 remints and retries exactly once.
 *   - Editing an existing Brief patches with the observed version (OCC).
 */
import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { cleanup, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { i18n } from '@/lib/i18n/config';

import { Route, Routes } from 'react-router';

import { WorldApp } from '@/components/world/world-app';
import { isUnlocked } from '@/lib/activation-store';
import { NexusClientError } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

const WORLD_ID = 'world-9';

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: WORLD_ID,
    block_type: 'era',
    status: 'confirmed',
    version: 1,
    ...overrides,
  };
}

function graph(entities: WorldKbEntityProjection[]): WorldKbGraphResponse {
  return { entities, source_anchors: [], relationships: [] };
}

// Promise.withResolvers is unavailable at the current tsconfig lib — local
// equivalent until the lib target reaches es2024.
function withResolvers<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

interface MockOpts {
  world?: () => Promise<unknown>;
  graph: () => Promise<unknown>;
  patchEntity?: Mock;
}

function makeClient({ world, graph: graphFn, patchEntity }: MockOpts): NexusClient {
  return {
    getWorld:
      world ??
      vi.fn(() =>
        Promise.resolve({ world: { world_id: WORLD_ID, title: 'Eryndor', slug: 'eryndor' } }),
      ),
    getWorldKbGraph: vi.fn(graphFn),
    worldKbPatchEntity: patchEntity ?? vi.fn(),
  } as unknown as NexusClient;
}

function renderWorld(
  client: NexusClient,
  initialPath = `/worlds/${WORLD_ID}`,
  opts: { activeCreatorId?: string | null } = {},
) {
  return renderInApp(
    <Routes>
      <Route path="/worlds/:worldId" element={<WorldApp />} />
    </Routes>,
    { client, initialRouterEntries: [initialPath], activeCreatorId: opts.activeCreatorId },
  );
}

const BRIEF_TEXT = '一个魔法正在消亡的低魔奇幻世界';

/** A patch-entity mock that succeeds and makes the next graph read see the era. */
function patchSucceeding(graphRef: { current: WorldKbGraphResponse }) {
  return vi.fn(
    (
      _: string,
      request: { entity_id: string; patch: { title?: string } },
    ) => {
      const title = request.patch.title ?? '';
      graphRef.current = graph([
        eraEntity({
          key_block_id: request.entity_id,
          canonical_name: title,
          version: 1,
          updated_at: new Date().toISOString(),
          body: { attributes: { world_summary: title } },
        }),
      ]);
      return Promise.resolve({ entity: { key_block_id: request.entity_id } });
    },
  );
}

beforeEach(() => {
  cleanup();
  window.localStorage.clear();
  window.localStorage.setItem('nexus-web-locale', 'zh-CN');
  return i18n.changeLanguage('zh-CN');
});

describe('Brief flow — empty state → editor', () => {
  it('shows the empty state with the action and mounts the editor on click', async () => {
    const user = userEvent.setup();
    renderWorld(makeClient({ graph: () => Promise.resolve(graph([])) }));

    const empty = await screen.findByTestId('world-brief-empty');
    expect(empty).toHaveTextContent('写下你的第一个 Brief');
    expect(empty).toHaveTextContent('例如：“一个魔法正在消亡的低魔奇幻世界”');

    await user.click(screen.getByTestId('world-brief-create'));
    const editor = await screen.findByTestId('world-brief-editor');
    expect(editor).toBeInTheDocument();
    expect(screen.getByTestId('world-brief-input')).toHaveAttribute(
      'placeholder',
      '一个魔法正在消亡的低魔奇幻世界……',
    );
    // 取消 unmounts the editor back to the empty state.
    await user.click(screen.getByTestId('world-brief-cancel'));
    expect(screen.getByTestId('world-brief-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('world-brief-editor')).not.toBeInTheDocument();
  });

  it('keeps 保存 Brief disabled while the trimmed text is empty', async () => {
    const user = userEvent.setup();
    renderWorld(makeClient({ graph: () => Promise.resolve(graph([])) }));

    await user.click(await screen.findByTestId('world-brief-create'));
    const save = await screen.findByTestId('world-brief-save');
    expect(save).toBeDisabled();
    await user.type(screen.getByTestId('world-brief-input'), '   ');
    expect(save).toBeDisabled();
    await user.type(screen.getByTestId('world-brief-input'), '魔法');
    expect(save).toBeEnabled();
    // Validation is silent — no error surface at all.
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});

describe('Brief flow — create save', () => {
  it('saves through the create-on-absent convention and renders the saved card without a reload', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = { current: graph([]) };
    const patchEntity = patchSucceeding(graphRef);
    renderWorld(makeClient({ graph: () => Promise.resolve(graphRef.current), patchEntity }));

    await user.click(await screen.findByTestId('world-brief-create'));
    await user.type(screen.getByTestId('world-brief-input'), BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));

    const card = await screen.findByTestId('world-brief-card');
    expect(card).toHaveTextContent(BRIEF_TEXT);
    expect(await screen.findByTestId('world-brief-tag')).toHaveTextContent('BRIEF · 刚刚');
    // The unlock follow-up card + toast render on success.
    expect(screen.getByTestId('world-brief-ready')).toHaveTextContent('世界已就绪');
    expect(screen.getByTestId('world-brief-ready')).toHaveTextContent(
      '「作品」模块已解锁——创建一个作品，开始在这个世界里写作。',
    );
    await waitFor(() =>
      expect(screen.getByText('✅ Brief 已保存——「作品」模块已解锁')).toBeInTheDocument(),
    );

    expect(patchEntity).toHaveBeenCalledTimes(1);
    const [worldId, request] = patchEntity.mock.calls[0] as [string, Record<string, unknown>];
    expect(worldId).toBe(WORLD_ID);
    expect(request.entity_id).toMatch(/^kb_[0-9a-f]{32}$/);
    expect(request.expected_version).toBe(0);
    const patch = request.patch as {
      title: string;
      block_type: string;
      body: { attributes: { world_summary: string } };
    };
    expect(patch.title).toBe(BRIEF_TEXT);
    expect(patch.block_type).toBe('era');
    expect(patch.body.attributes.world_summary).toBe(BRIEF_TEXT);
    // The editor is gone — the card replaced it without a reload.
    expect(screen.queryByTestId('world-brief-editor')).not.toBeInTheDocument();
  });

  it('remints and retries exactly once when create returns 409', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = { current: graph([]) };
    const succeeding = patchSucceeding(graphRef);
    const patchEntity = vi
      .fn()
      .mockRejectedValueOnce(
        new NexusClientError(409, 'world_kb_conflict', 'conflict', {
          current_version: 1,
          entity_id: 'kb_taken',
          conflicting_path: 'entity_id',
          recovery_hint: 'remint',
        }),
      )
      .mockImplementation(succeeding);
    renderWorld(makeClient({ graph: () => Promise.resolve(graphRef.current), patchEntity }));

    await user.click(await screen.findByTestId('world-brief-create'));
    await user.type(screen.getByTestId('world-brief-input'), BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));

    expect(await screen.findByTestId('world-brief-card')).toBeInTheDocument();
    expect(patchEntity).toHaveBeenCalledTimes(2);
    const [first, second] = patchEntity.mock.calls.map(
      (c) => (c as [string, { entity_id: string }])[1].entity_id,
    );
    expect(first).not.toBe(second);
  });
});

describe('Brief flow — save failure', () => {
  it('shows the actionable message, keeps the draft mounted, and retries succeed', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = { current: graph([]) };
    const succeeding = patchSucceeding(graphRef);
    const patchEntity = vi
      .fn()
      .mockRejectedValueOnce(new Error('boom'))
      .mockImplementation(succeeding);
    renderWorld(makeClient({ graph: () => Promise.resolve(graphRef.current), patchEntity }));

    await user.click(await screen.findByTestId('world-brief-create'));
    const input = screen.getByTestId('world-brief-input');
    await user.type(input, BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Brief 保存失败：boom。草稿已保留，可直接重试。');
    // Editor stays mounted with the draft intact.
    expect(input).toHaveValue(BRIEF_TEXT);

    // Retry with the draft intact succeeds.
    await user.click(screen.getByTestId('world-brief-save'));
    expect(await screen.findByTestId('world-brief-card')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('renders 422 validation_summary.errors[] inline as the reason', async () => {
    const user = userEvent.setup();
    const patchEntity = vi.fn().mockRejectedValue(
      new NexusClientError(422, 'world_kb_validation_failed', 'invalid', {
        validation_summary: { errors: ['world_summary 过长', 'title 为空'] },
      }),
    );
    renderWorld(makeClient({ graph: () => Promise.resolve(graph([])), patchEntity }));

    await user.click(await screen.findByTestId('world-brief-create'));
    await user.type(screen.getByTestId('world-brief-input'), BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Brief 保存失败：world_summary 过长；title 为空。草稿已保留，可直接重试。',
    );
    expect(screen.getByTestId('world-brief-input')).toHaveValue(BRIEF_TEXT);
  });
});

describe('Brief flow — edit (per-row OCC)', () => {
  it('patches the observed entity version when editing an existing Brief', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = {
      current: graph([
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: BRIEF_TEXT,
          version: 3,
          updated_at: new Date().toISOString(),
          body: { attributes: { world_summary: BRIEF_TEXT } },
        }),
      ]),
    };
    const patchEntity = vi.fn(
      (
        _: string,
        request: {
          entity_id: string;
          expected_version: number;
          patch: { title?: string };
        },
      ) => {
        graphRef.current = graph([
          eraEntity({
            key_block_id: 'kb-era-1',
            canonical_name: request.patch.title!,
            version: 4,
            updated_at: new Date().toISOString(),
            body: { attributes: { world_summary: request.patch.title } },
          }),
        ]);
        return Promise.resolve({ entity: { key_block_id: 'kb-era-1' } });
      },
    );
    renderWorld(makeClient({ graph: () => Promise.resolve(graphRef.current), patchEntity }));

    await user.click(await screen.findByTestId('world-brief-edit'));
    const input = await screen.findByTestId('world-brief-input');
    expect(input).toHaveValue(BRIEF_TEXT);
    await user.clear(input);
    await user.type(input, '魔法回归的世界');
    await user.click(screen.getByTestId('world-brief-save'));

    expect(await screen.findByTestId('world-brief-card')).toHaveTextContent('魔法回归的世界');
    expect(patchEntity).toHaveBeenCalledTimes(1);
    const [worldId, request] = patchEntity.mock.calls[0] as [
      string,
      { entity_id: string; expected_version: number },
    ];
    expect(worldId).toBe(WORLD_ID);
    expect(request.entity_id).toBe('kb-era-1');
    expect(request.expected_version).toBe(3);
  });

  it('binds the clicked card: editing the second era sends the second entity id + version', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = {
      current: graph([
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: '第一纪元',
          version: 1,
          updated_at: new Date().toISOString(),
          body: { attributes: { world_summary: '第一纪元的简介。' } },
        }),
        eraEntity({
          key_block_id: 'kb-era-2',
          canonical_name: '第二纪元',
          version: 5,
          updated_at: new Date().toISOString(),
          body: { attributes: { world_summary: '第二纪元的简介。' } },
        }),
      ]),
    };
    const patchEntity = vi.fn(
      (
        _: string,
        request: {
          entity_id: string;
          expected_version: number;
          patch: { title?: string };
        },
      ) => {
        graphRef.current = graph([
          eraEntity({
            key_block_id: 'kb-era-2',
            canonical_name: request.patch.title!,
            version: 6,
            updated_at: new Date().toISOString(),
            body: { attributes: { world_summary: request.patch.title } },
          }),
        ]);
        return Promise.resolve({ entity: { key_block_id: 'kb-era-2' } });
      },
    );
    renderWorld(makeClient({ graph: () => Promise.resolve(graphRef.current), patchEntity }));

    const editButtons = await screen.findAllByTestId('world-brief-edit');
    expect(editButtons).toHaveLength(2);
    await user.click(editButtons[1]);

    const input = await screen.findByTestId('world-brief-input');
    expect(input).toHaveValue('第二纪元的简介。');
    await user.clear(input);
    await user.type(input, '更新后的第二纪元。');
    await user.click(screen.getByTestId('world-brief-save'));

    expect(patchEntity).toHaveBeenCalledTimes(1);
    const request = (patchEntity.mock.calls[0] as [string, { entity_id: string; expected_version: number }])[1];
    expect(request.entity_id).toBe('kb-era-2');
    expect(request.expected_version).toBe(5);
  });
});

describe('Brief flow — no flash / no lost rows (fix round 1)', () => {
  it('stays in the editor while the graph refetch is pending, then lands on the card', async () => {
    const user = userEvent.setup();
    const savedGraph = graph([
      eraEntity({
        key_block_id: 'kb-new',
        canonical_name: BRIEF_TEXT,
        version: 1,
        updated_at: new Date().toISOString(),
        body: { attributes: { world_summary: BRIEF_TEXT } },
      }),
    ]);
    // First read resolves immediately; the post-save invalidation refetch
    // stays pending until the test releases it.
    let releaseRefetch: (g: WorldKbGraphResponse) => void = () => {};
    let read = 0;
    const graphFn = () => {
      read += 1;
      if (read === 1) return Promise.resolve(graph([]));
      const { promise, resolve } = withResolvers<WorldKbGraphResponse>();
      releaseRefetch = resolve;
      return promise;
    };
    const patchEntity = vi.fn(() => Promise.resolve({ entity: { key_block_id: 'kb-new' } }));
    renderWorld(makeClient({ graph: graphFn, patchEntity }));

    await user.click(await screen.findByTestId('world-brief-create'));
    await user.type(screen.getByTestId('world-brief-input'), BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));

    await waitFor(() => expect(patchEntity).toHaveBeenCalledTimes(1));
    // The editor is still mounted and the empty state never flashes — save
    // does not leave edit mode until the awaited graph refetch resolves.
    expect(screen.getByTestId('world-brief-editor')).toBeInTheDocument();
    expect(screen.queryByTestId('world-brief-empty')).not.toBeInTheDocument();

    releaseRefetch(savedGraph);
    expect(await screen.findByTestId('world-brief-card')).toBeInTheDocument();
    expect(screen.queryByTestId('world-brief-empty')).not.toBeInTheDocument();
  });

  it('keeps the without-summary era rows visible alongside the empty state', async () => {
    renderWorld(
      makeClient({
        graph: () =>
          Promise.resolve(
            graph([
              eraEntity({
                key_block_id: 'kb-era-2',
                canonical_name: '第二纪元',
                version: 1,
              }),
            ]),
          ),
      }),
    );

    expect(await screen.findByTestId('world-brief-empty')).toBeInTheDocument();
    expect(screen.getByTestId('world-layer-brief')).toHaveTextContent('第二纪元');
  });
});

describe('Brief flow — T3 checklist + Brief→Work unlock', () => {
  const CREATOR = 'creator-a';

  function savedBriefGraph(): WorldKbGraphResponse {
    return graph([
      eraEntity({
        key_block_id: 'kb-era-1',
        canonical_name: BRIEF_TEXT,
        version: 1,
        updated_at: new Date().toISOString(),
        body: { attributes: { world_summary: BRIEF_TEXT } },
      }),
    ]);
  }

  it('shows 进度 0 / 3 with step ① next before any Brief', async () => {
    renderWorld(makeClient({ graph: () => Promise.resolve(graph([])) }));

    expect(await screen.findByTestId('world-checklist-progress')).toHaveTextContent('进度 0 / 3');
    expect(screen.getByTestId('world-checklist-step-1')).toHaveClass('font-medium');
    expect(screen.getByTestId('world-checklist-step-1')).not.toHaveClass('line-through');
  });

  it('persists the Work unlock per creator on save and advances the checklist to 进度 1 / 3', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = { current: graph([]) };
    renderWorld(
      makeClient({
        graph: () => Promise.resolve(graphRef.current),
        patchEntity: patchSucceeding(graphRef),
      }),
      `/worlds/${WORLD_ID}`,
      { activeCreatorId: CREATOR },
    );

    await user.click(await screen.findByTestId('world-brief-create'));
    await user.type(screen.getByTestId('world-brief-input'), BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));

    // Persisted per creator; only `work` is written — no other unlock events.
    expect(window.localStorage.getItem(`nexus-activation:${CREATOR}`)).toBe(
      JSON.stringify({ work: true }),
    );
    expect(isUnlocked(CREATOR, 'work')).toBe(true);
    expect(isUnlocked(CREATOR, 'session')).toBe(false);

    // Checklist: ① done, ② next, 进度 1 / 3.
    expect(await screen.findByTestId('world-checklist-progress')).toHaveTextContent('进度 1 / 3');
    expect(screen.getByTestId('world-checklist-step-1')).toHaveClass('line-through');
    expect(screen.getByTestId('world-checklist-step-2')).toHaveClass('font-medium');
  });

  it('survives a reload: a fresh render for the same creator still sees Work unlocked', async () => {
    const user = userEvent.setup();
    const graphRef: { current: WorldKbGraphResponse } = { current: graph([]) };
    const first = renderWorld(
      makeClient({
        graph: () => Promise.resolve(graphRef.current),
        patchEntity: patchSucceeding(graphRef),
      }),
      `/worlds/${WORLD_ID}`,
      { activeCreatorId: CREATOR },
    );
    await user.click(await screen.findByTestId('world-brief-create'));
    await user.type(screen.getByTestId('world-brief-input'), BRIEF_TEXT);
    await user.click(screen.getByTestId('world-brief-save'));
    await screen.findByTestId('world-brief-card');
    first.unmount();

    renderWorld(
      makeClient({ graph: () => Promise.resolve(savedBriefGraph()) }),
      `/worlds/${WORLD_ID}`,
      { activeCreatorId: CREATOR },
    );
    // The unlock persisted across the reload — the rail/store sees it.
    expect(isUnlocked(CREATOR, 'work')).toBe(true);
  });

  it('gates BriefReadyCard to the just-saved session — revisits with an existing Brief do not re-announce', async () => {
    renderWorld(
      makeClient({ graph: () => Promise.resolve(savedBriefGraph()) }),
      `/worlds/${WORLD_ID}`,
      { activeCreatorId: CREATOR },
    );

    expect(await screen.findByTestId('world-brief-card')).toBeInTheDocument();
    expect(screen.queryByTestId('world-brief-ready')).not.toBeInTheDocument();
    // The checklist still reflects the saved Brief.
    expect(screen.getByTestId('world-checklist-progress')).toHaveTextContent('进度 1 / 3');
  });
});
