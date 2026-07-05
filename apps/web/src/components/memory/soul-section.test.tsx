/**
 * SoulSection integration tests (V1.82 SP-2).
 *
 * Renders SoulSection against msw handlers to assert the selector→narrative
 * scope linkage: "All worlds" sends no `world_id`; selecting a world sends that
 * `world_id`; per-world insufficient/empty states render independently of the
 * Creator-level state.
 */
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { SoulSection } from '@/components/memory/soul-section';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

const CREATOR = 'creator-active';

function world(id: string, title: string) {
  return {
    schema_version: 1,
    world_id: id,
    owner_creator_id: CREATOR,
    title,
    slug: id,
    status: 'active',
    visibility: 'private',
    time_policy: 'manual',
    created_at: '2026-07-01T00:00:00Z',
  };
}

/** Default narrative response used when a test does not override the handler. */
function defaultNarrativeResponse(body: { world_id?: string }) {
  return HttpResponse.json({
    creator_id: CREATOR,
    state: 'current',
    narrative: 'An earlier reflection on your themes.',
    generated_at: '2026-07-01T12:00:00Z',
    stale: false,
    current_fragment_count: 1,
    current_distinct_keyword_count: 1,
    min_fragment_count: 10,
    min_distinct_keyword_count: 20,
    world_id: body.world_id,
  });
}

/** Worlds + fragments handlers; narrative handler is provided per-test. */
function baselineHandlers(overrides: {
  worlds?: unknown[];
  fragments?: unknown[];
} = {}) {
  const {
    worlds = [world('eryndor', 'Eryndor')],
    fragments = [{ fragment_id: 'f1', summary: 's1', world_id: 'eryndor', keywords: ['k1'], created_at: '2026-07-01T00:00:00Z' }],
  } = overrides;

  return [
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds })),
    http.get('/v1/daemon/memory/fragments', ({ request }) => {
      const url = new URL(request.url);
      const worldId = url.searchParams.get('world_id');
      const items = worldId
        ? fragments.filter((f) => (f as { world_id?: string }).world_id === worldId)
        : fragments;
      return HttpResponse.json({ fragments: items });
    }),
  ];
}

describe('SoulSection — selector → narrative scope linkage', () => {
  it('reflects the Creator-level narrative when "All worlds" is selected', async () => {
    const reflectSpy = vi.fn();
    useHandlers(
      ...baselineHandlers(),
      http.post('/v1/daemon/memory/soul/reflect', async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as { world_id?: string };
        reflectSpy(body);
        return HttpResponse.json({
          creator_id: CREATOR,
          state: 'current',
          narrative: 'Creator-level reflection.',
          generated_at: '2026-07-01T12:00:00Z',
          stale: false,
          current_fragment_count: 1,
          current_distinct_keyword_count: 1,
          min_fragment_count: 10,
          min_distinct_keyword_count: 20,
          world_id: body.world_id,
        });
      }),
    );

    renderInApp(
      <SoulSection creatorId={CREATOR} onFilterFragments={() => {}} />,
      { client: new BrowserClient() },
    );

    await waitFor(() => expect(reflectSpy).toHaveBeenCalled());
    const lastCall = reflectSpy.mock.calls[reflectSpy.mock.calls.length - 1]![0] as {
      creator_id: string;
      world_id?: string;
    };
    expect(lastCall.creator_id).toBe(CREATOR);
    expect(lastCall.world_id).toBeUndefined();
    expect(await screen.findByTestId('soul-narrative-current')).toBeInTheDocument();
    expect(screen.getByTestId('soul-narrative-prose')).toHaveTextContent('Creator-level reflection.');
  });

  it('reflects the per-world narrative when a world is selected', async () => {
    const reflectSpy = vi.fn();
    useHandlers(
      ...baselineHandlers({
        worlds: [world('eryndor', 'Eryndor'), world('solara', 'Solara')],
        fragments: [
          { fragment_id: 'f1', summary: 's1', world_id: 'eryndor', keywords: ['k1'], created_at: '2026-07-01T00:00:00Z' },
        ],
      }),
      http.post('/v1/daemon/memory/soul/reflect', async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as { world_id?: string };
        reflectSpy(body);
        return HttpResponse.json({
          creator_id: CREATOR,
          state: 'current',
          narrative: body.world_id === 'solara' ? 'Solara-specific reflection.' : 'Creator-level reflection.',
          generated_at: '2026-07-01T12:00:00Z',
          stale: false,
          current_fragment_count: body.world_id === 'solara' ? 0 : 1,
          current_distinct_keyword_count: 1,
          min_fragment_count: 10,
          min_distinct_keyword_count: 20,
          world_id: body.world_id,
        });
      }),
    );

    renderInApp(
      <SoulSection creatorId={CREATOR} onFilterFragments={() => {}} />,
      { client: new BrowserClient() },
    );

    await waitFor(() => expect(reflectSpy).toHaveBeenCalled());

    fireEvent.change(screen.getByTestId('soul-world-selector'), {
      target: { value: 'solara' },
    });

    await waitFor(() =>
      expect(reflectSpy).toHaveBeenLastCalledWith(
        expect.objectContaining({ creator_id: CREATOR, world_id: 'solara' }),
      ),
    );
    expect(await screen.findByText('Solara-specific reflection.')).toBeInTheDocument();
  });

  it('shows the per-world insufficient state independently of the Creator-level state', async () => {
    useHandlers(
      ...baselineHandlers({
        worlds: [world('solara', 'Solara')],
        fragments: [
          { fragment_id: 'f1', summary: 's1', world_id: 'solara', keywords: ['k1'], created_at: '2026-07-01T00:00:00Z' },
        ],
      }),
      http.post('/v1/daemon/memory/soul/reflect', async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as { world_id?: string };
        return HttpResponse.json({
          creator_id: CREATOR,
          state: body.world_id ? 'insufficient_data' : 'current',
          narrative: body.world_id ? undefined : 'Creator-level reflection.',
          generated_at: '2026-07-01T12:00:00Z',
          stale: false,
          current_fragment_count: 1,
          current_distinct_keyword_count: 1,
          min_fragment_count: 10,
          min_distinct_keyword_count: 20,
          world_id: body.world_id,
        });
      }),
    );

    renderInApp(
      <SoulSection creatorId={CREATOR} onFilterFragments={() => {}} />,
      { client: new BrowserClient() },
    );

    await waitFor(() => expect(screen.getByTestId('soul-narrative-current')).toBeInTheDocument());

    fireEvent.change(screen.getByTestId('soul-world-selector'), {
      target: { value: 'solara' },
    });

    expect(await screen.findByTestId('soul-narrative-insufficient')).toBeInTheDocument();
    expect(screen.getByText("This world's SOUL is still forming")).toBeInTheDocument();
  });

  it('shows the honest subset-empty state for a Work-backed world with zero fragments', async () => {
    useHandlers(
      ...baselineHandlers({
        worlds: [world('solara', 'Solara')],
        fragments: [
          { fragment_id: 'f1', summary: 's1', world_id: 'eryndor', keywords: ['k1'], created_at: '2026-07-01T00:00:00Z' },
        ],
      }),
      http.post('/v1/daemon/memory/soul/reflect', async ({ request }) => defaultNarrativeResponse((await request.json().catch(() => ({}))) as { world_id?: string })),
    );

    renderInApp(
      <SoulSection creatorId={CREATOR} onFilterFragments={() => {}} />,
      { client: new BrowserClient() },
    );

    await waitFor(() => expect(screen.getByTestId('soul-world-selector')).not.toBeDisabled());

    fireEvent.change(screen.getByTestId('soul-world-selector'), {
      target: { value: 'solara' },
    });

    expect(await screen.findByTestId('soul-world-subset-empty')).toBeInTheDocument();
    expect(screen.getByText('No fragments in this world yet')).toBeInTheDocument();
  });
});
