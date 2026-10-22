/**
 * World app surface tests (plan v1219-world-module T1).
 *
 * Locks the acceptance contract:
 *   - Header renders the world name + the product-owned subtitle
 *     `世界 · 时间线`; layer tabs render with Brief selected by default.
 *   - Brief is the default layer — populated from the era KnowledgeEntry
 *     projection (`block_type === 'era'` over the World KB graph) when era
 *     entries exist, otherwise the empty state.
 *   - Narrative / Moment tabs navigate via `?layer=` without breaking the
 *     Brief route; their content is the labelled pre-rebuild placeholder
 *     with the sub-line `此层内容在后续迭代启用`.
 *   - Loading state `正在加载世界…` and error state `世界加载失败：{reason}`
 *     with a `重试` button — single explicit states, never silent blanks.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { i18n } from '@/lib/i18n/config';

import { Route, Routes } from 'react-router';

import { parseWorldLayer } from '@/components/world/layer-tabs';
import { WorldApp } from '@/components/world/world-app';
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

const ERA_FIRST = eraEntity({
  key_block_id: 'kb-era-1',
  canonical_name: '第一纪元',
  body: { attributes: { world_summary: '神话与传说的时代。' } },
});

const ERA_SECOND = eraEntity({
  key_block_id: 'kb-era-2',
  canonical_name: '第二纪元',
  body: { attributes: {} },
});

const KB_EVENT = eraEntity({
  key_block_id: 'kb-event-1',
  canonical_name: 'Coronation',
  block_type: 'event',
});

function graph(entities: WorldKbEntityProjection[]): WorldKbGraphResponse {
  return { entities, source_anchors: [], relationships: [] };
}

function makeClient(over: {
  world?: () => Promise<unknown>;
  graph?: () => Promise<unknown>;
}): NexusClient {
  return {
    getWorld: vi.fn(
      over.world ??
        (() =>
          Promise.resolve({
            world: { world_id: WORLD_ID, title: 'Eryndor', slug: 'eryndor' },
          })),
    ),
    getWorldKbGraph: vi.fn(over.graph ?? (() => Promise.resolve(graph([])))),
  } as unknown as NexusClient;
}

function renderWorld(initialPath = `/worlds/${WORLD_ID}`, client?: NexusClient) {
  return renderInApp(
    <Routes>
      <Route path="/worlds/:worldId" element={<WorldApp />} />
    </Routes>,
    { client: client ?? makeClient({}), initialRouterEntries: [initialPath] },
  );
}

beforeEach(() => {
  cleanup();
  // LocaleProvider resolves from localStorage on mount — pin zh-CN so the
  // product-owned Chinese copy assertions below hold.
  window.localStorage.setItem('nexus-web-locale', 'zh-CN');
  return i18n.changeLanguage('zh-CN');
});

describe('WorldApp — header + layer tabs', () => {
  it('renders the world name and the 世界 · 时间线 subtitle', async () => {
    renderWorld();
    expect(await screen.findByRole('heading', { level: 1, name: 'Eryndor' })).toBeInTheDocument();
    expect(screen.getByText('世界 · 时间线')).toBeInTheDocument();
  });

  it('renders the three layer tabs with Brief selected by default', async () => {
    renderWorld();
    const tabs = await screen.findAllByRole('tab');
    expect(tabs.map((tab) => tab.textContent)).toEqual(['Brief', 'Narrative', 'Moment']);
    expect(screen.getByTestId('layer-tab-brief')).toHaveAttribute('aria-selected', 'true');
  });
});

describe('WorldApp — Brief layer (era KnowledgeEntry projection)', () => {
  it('renders era entries with their world_summary when eras exist', async () => {
    renderWorld(undefined, makeClient({ graph: () => Promise.resolve(graph([KB_EVENT, ERA_FIRST, ERA_SECOND])) }));
    const layer = await screen.findByTestId('world-layer-brief');
    // Only era projections render — narrative events stay out of the Brief read.
    expect(layer).toHaveTextContent('第一纪元');
    expect(layer).toHaveTextContent('神话与传说的时代。');
    expect(layer).toHaveTextContent('第二纪元');
    expect(layer).not.toHaveTextContent('Coronation');
  });

  it('renders the empty state when no era entries exist', async () => {
    renderWorld(undefined, makeClient({ graph: () => Promise.resolve(graph([KB_EVENT])) }));
    const empty = await screen.findByTestId('world-brief-empty');
    expect(empty).toHaveTextContent('写下你的第一个 Brief');
    expect(empty).toHaveTextContent('Brief 是这个世界的最高层描述——一句话讲清它是什么。');
    expect(empty).toHaveTextContent('例如：“一个魔法正在消亡的低魔奇幻世界”');
  });
});

describe('WorldApp — Narrative / Moment pre-rebuild placeholders', () => {
  it('navigates to the Narrative placeholder via ?layer=narrative', async () => {
    renderWorld(`/worlds/${WORLD_ID}?layer=narrative`);
    const layer = await screen.findByTestId('world-layer-narrative');
    expect(layer).toHaveTextContent('Narrative 层——章节级叙事时间线');
    expect(layer).toHaveTextContent('此层内容在后续迭代启用');
    // The URL convention is honoured — no route break.
    expect(screen.getByTestId('layer-tab-narrative')).toHaveAttribute('aria-selected', 'true');
  });

  it('navigates to the Moment placeholder via ?layer=moment', async () => {
    renderWorld(`/worlds/${WORLD_ID}?layer=moment`);
    const layer = await screen.findByTestId('world-layer-moment');
    expect(layer).toHaveTextContent('Moment 层——场景级细粒度时间线');
    expect(layer).toHaveTextContent('此层内容在后续迭代启用');
  });

  it('clicking a tab switches the layer param without leaving the world route', async () => {
    const user = userEvent.setup();
    renderWorld();
    await screen.findByTestId('world-layer-brief');
    await user.click(screen.getByTestId('layer-tab-narrative'));
    expect(await screen.findByTestId('world-layer-narrative')).toBeInTheDocument();
    expect(screen.getByTestId('layer-tab-brief')).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByTestId('layer-tab-narrative')).toHaveAttribute('aria-selected', 'true');
  });
});

describe('WorldApp — explicit loading / error states', () => {
  it('renders 正在加载世界… while the world query is pending', () => {
    renderWorld(undefined, makeClient({ world: () => new Promise(() => {}) }));
    expect(screen.getByText('正在加载世界…')).toBeInTheDocument();
  });

  it('renders 世界加载失败：{reason} with a 重试 button on error and refetches on retry', async () => {
    const user = userEvent.setup();
    const failingWorld = vi
      .fn()
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValue({ world: { world_id: WORLD_ID, title: 'Eryndor', slug: 'eryndor' } });
    renderWorld(undefined, makeClient({ world: failingWorld }));

    expect(await screen.findByRole('alert')).toHaveTextContent('世界加载失败：boom');
    await user.click(screen.getByRole('button', { name: '重试' }));
    expect(await screen.findByTestId('world-app')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1, name: 'Eryndor' })).toBeInTheDocument();
    expect(failingWorld).toHaveBeenCalledTimes(2);
  });
});

describe('parseWorldLayer', () => {
  it('defaults unknown / absent values to brief', () => {
    expect(parseWorldLayer(null)).toBe('brief');
    expect(parseWorldLayer('')).toBe('brief');
    expect(parseWorldLayer('bogus')).toBe('brief');
  });

  it('accepts the two non-default layers', () => {
    expect(parseWorldLayer('narrative')).toBe('narrative');
    expect(parseWorldLayer('moment')).toBe('moment');
  });
});
