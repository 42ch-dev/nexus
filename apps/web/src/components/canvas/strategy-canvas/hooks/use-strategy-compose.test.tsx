/**
 * Compose interactions (v239-p3-canvas-compose-ui T3, T3b, T4) at the hook
 * level over msw: every op sends the last-read `base_revision`; `nest` sends
 * its attach target and the daemon writes the connecting edge, so the canvas
 * adds no edge of its own (T3b); a 409/422 leaves the working document
 * unchanged, never auto-retries, and the 409 recovery reloads the canonical
 * candidate at the revision the daemon holds (T4).
 */
import { renderHook, act, waitFor, fireEvent, screen } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { afterEach, describe, expect, it } from 'vitest';
import type { PropsWithChildren } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { BrowserClient } from '@/lib/nexus';
import { ClientProvider } from '@/lib/client-context';
import { useStrategyCompose } from '@/components/canvas/strategy-canvas/hooks/use-strategy-compose';
import { SlotInspector } from '@/components/canvas/strategy-canvas/inspectors/slot-inspector';
import { buildStrategyGraphFromIr } from '@/lib/canvas/strategy-graph-ir';
import { makeQueryClient, renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { candidateDocument, servedIrDocument } from '@/test/simulation-fixture';

const STRATEGY_ID = 'user/demo';
const PATTERNS_PATH = '/v1/daemon/patterns';
const CANDIDATE_PATH = `/v1/daemon/strategies/${encodeURIComponent(STRATEGY_ID)}/candidate`;
const COMPOSE_PATH = `/v1/daemon/strategies/${encodeURIComponent(STRATEGY_ID)}/compose`;

const SEED = servedIrDocument({ id: STRATEGY_ID });

const PATTERN_ITEMS = [
  {
    package_dir: 'prompt-step',
    id: 'prompt-step',
    version: '1.0.0',
    description: 'Prompt step',
    content_hash: 'abc',
    effect_class: 'external',
    slots: [{ name: 'topic', kind: 'string', required: true }],
    requires_capabilities: [],
    available: true,
    reason: null,
  },
];

/** Seed document plus the slot and connecting edge the nest op commits. */
function nestedDocument(revision: number, bind: Record<string, unknown> = {}): WorkflowDocument {
  return {
    ...SEED,
    revision,
    edges: [
      ...SEED.edges,
      { id: 'draft->prompt-step-slot', from: 'draft', to: 'prompt-step-slot', kind: 'linear' },
    ],
    nodes: [
      ...SEED.nodes,
      {
        id: 'prompt-step-slot',
        kind: 'pattern_slot',
        effect_class: 'external',
        bind,
        pattern: { registry_id: 'prompt-step', version: '1.0.0', content_hash: 'abc' },
      },
    ],
    nests: [
      {
        id: 'prompt-step',
        parent_node: 'prompt-step-slot',
        expansion: 'expand_in_candidate',
        pin_policy: 'exact',
        child: { kind: 'pattern', registry_id: 'prompt-step', version: '1.0.0', content_hash: 'abc' },
      },
    ],
  };
}

function seedHandlers() {
  useHandlers(
    http.get(PATTERNS_PATH, () => HttpResponse.json({ items: PATTERN_ITEMS, pagination: { limit: 100, has_more: false } })),
    http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
  );
}

function makeWrapper() {
  const qc = makeQueryClient();
  return function Wrapper({ children }: PropsWithChildren) {
    return (
      <QueryClientProvider client={qc}>
        <ClientProvider client={new BrowserClient()} desktop={null} connectionConfig={null}>
          {children}
        </ClientProvider>
      </QueryClientProvider>
    );
  };
}

function renderCompose() {
  return renderHook(() => useStrategyCompose(STRATEGY_ID, true), { wrapper: makeWrapper() });
}

afterEach(() => {
  // msw resets handlers between tests via setup.ts.
});

describe('useStrategyCompose', () => {
  it('seeds the working candidate from the IR read and lists patterns', async () => {
    seedHandlers();
    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));
    expect(result.current.revision).toBe(0);
    expect(result.current.patternsQuery.data?.items).toHaveLength(1);
  });

  it('follows the listing cursor so a package on a later page is visible', async () => {
    const laterPage = {
      ...PATTERN_ITEMS[0],
      package_dir: 'second-page-step',
      id: 'second-page-step',
      description: 'Prompt step on page two',
    };
    useHandlers(
      http.get(PATTERNS_PATH, ({ request }) => {
        const cursor = new URL(request.url).searchParams.get('cursor');
        if (cursor === 'page-2') {
          return HttpResponse.json({ items: [laterPage], pagination: { limit: 1, has_more: false } });
        }
        return HttpResponse.json({
          items: PATTERN_ITEMS,
          pagination: { limit: 1, has_more: true, next_cursor: 'page-2' },
        });
      }),
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
    );

    const { result } = renderCompose();

    await waitFor(() => expect(result.current.patternsQuery.data?.items).toHaveLength(2));
    expect(result.current.patternsQuery.data?.items.map((pattern) => pattern.id)).toEqual([
      'prompt-step',
      'second-page-step',
    ]);
  });

  it('caps the listing walk and reports the unfinished walk as a query error', async () => {
    let pages = 0;
    useHandlers(
      http.get(PATTERNS_PATH, () => {
        pages += 1;
        return HttpResponse.json({
          items: PATTERN_ITEMS,
          // One page more than the cap allows: the walk must refuse, not return a short list.
          pagination: { limit: 1, has_more: pages < 25, next_cursor: `page-${pages}` },
        });
      }),
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
    );

    const { result } = renderCompose();

    await waitFor(() => expect(result.current.patternsQuery.isError).toBe(true));
    expect(result.current.patternsQuery.error?.message).toContain('20 pages');
    expect(pages).toBe(20);
  });

  it('resolves a pinned slot from the available duplicate, not the first listing entry', async () => {
    // Two directories declare one pin: the first is unreadable for the reader, so it declares no
    // slots. Reading its empty declaration would hide the required slot the pin really has.
    const brokenDuplicate = {
      ...PATTERN_ITEMS[0],
      package_dir: 'prompt-step-broken',
      description: null,
      content_hash: null,
      slots: [],
      available: false,
      reason: 'manifest parsed but the fragment is missing',
    };
    useHandlers(
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({
          items: [brokenDuplicate, PATTERN_ITEMS[0]],
          pagination: { limit: 100, has_more: false },
        }),
      ),
      http.get(CANDIDATE_PATH, () => HttpResponse.json(nestedDocument(0))),
    );

    const { result } = renderCompose();

    await waitFor(() => expect(result.current.patternsQuery.data?.items).toHaveLength(2));
    expect(result.current.unboundSlots).toEqual([
      {
        slotNodeId: 'prompt-step-slot',
        patternId: 'prompt-step',
        patternVersion: '1.0.0',
        slots: ['topic'],
      },
    ]);
  });

  it('nest sends the last-read revision and the interaction wires the connecting edge (T3b)', async () => {
    seedHandlers();
    const bodies: unknown[] = [];
    useHandlers(
      http.post(COMPOSE_PATH, async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          new_revision: 1,
          document: nestedDocument(1),
          validation_summary: [
            {
              code: 'unbound_required_slot',
              nest_id: 'prompt-step',
              package_id: 'prompt-step',
              package_version: '1.0.0',
              slot: 'topic',
              message: 'nest `prompt-step`: pattern `prompt-step@1.0.0` requires slot `topic`, which is unbound',
            },
          ],
        });
      }),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));

    await act(async () => {
      await result.current.nestPattern({ attachNodeId: 'draft', package: { id: 'prompt-step', version: '1.0.0' } });
    });

    // The op guarded the last-read revision (0) and named the new slot node.
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      base_revision: 0,
      op: 'nest',
      payload: {
        nest_id: 'prompt-step',
        parent_node: 'prompt-step-slot',
        attach_to: 'draft',
        package: { id: 'prompt-step', version: '1.0.0' },
        expansion: 'expand_in_candidate',
      },
    });

    // The daemon wrote the connecting edge on the committed document, and the
    // canvas added no second one of its own.
    const committed = result.current.workingDocument!;
    const slot = committed.nodes.find((node) => node.id === 'prompt-step-slot');
    expect(slot?.kind).toBe('pattern_slot');
    expect(committed.revision).toBe(1);
    const inbound = committed.edges.filter((edge) => edge.to === 'prompt-step-slot');
    expect(inbound).toEqual([
      { id: 'draft->prompt-step-slot', from: 'draft', to: 'prompt-step-slot', kind: 'linear' },
    ]);
    expect(result.current.unboundSlots).toEqual([
      { slotNodeId: 'prompt-step-slot', patternId: 'prompt-step', patternVersion: '1.0.0', slots: ['topic'] },
    ]);
  });

  it('bind_slot advances the revision and keeps the committed edge across the response', async () => {
    seedHandlers();
    let revision = 0;
    useHandlers(
      http.post(COMPOSE_PATH, async ({ request }) => {
        const body = (await request.json()) as { op: string; base_revision: number };
        expect(body.base_revision).toBe(revision);
        revision += 1;
        if (body.op === 'nest') {
          return HttpResponse.json({
            new_revision: 1,
            document: nestedDocument(1),
            validation_summary: [],
          });
        }
        return HttpResponse.json({
          new_revision: 2,
          document: nestedDocument(2, { topic: 'water policy' }),
          validation_summary: [],
        });
      }),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));
    await act(async () => {
      await result.current.nestPattern({ attachNodeId: 'draft', package: { id: 'prompt-step', version: '1.0.0' } });
    });
    await act(async () => {
      await result.current.bindSlot('prompt-step', 'topic', 'water policy');
    });

    const document = result.current.workingDocument!;
    expect(document.revision).toBe(2);
    expect(document.nodes.find((node) => node.id === 'prompt-step-slot')?.bind?.topic).toBe('water policy');
    expect(document.edges.some((edge) => edge.to === 'prompt-step-slot')).toBe(true);
    expect(result.current.unboundSlots).toEqual([]);
  });

  it('a stale base is the typed conflict: unchanged graph, no auto-retry, reload at the current revision (T4)', async () => {
    let composeCalls = 0;
    let candidateReads = 0;
    useHandlers(
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: PATTERN_ITEMS, pagination: { limit: 100, has_more: false } }),
      ),
      // The daemon moved on: the reload reads the candidate at revision 1.
      http.get(CANDIDATE_PATH, () => {
        candidateReads += 1;
        return HttpResponse.json(candidateReads === 1 ? SEED : nestedDocument(1));
      }),
      http.post(COMPOSE_PATH, async ({ request }) => {
        composeCalls += 1;
        const body = (await request.json()) as { base_revision: number };
        return HttpResponse.json(
          {
            error: {
              code: 'strategy_candidate_conflict',
              message: `Candidate changed before this edit. Reload revision ${body.base_revision + 1} and reapply the change.`,
              details: {
                strategy_id: STRATEGY_ID,
                base_revision: body.base_revision,
                current_revision: body.base_revision + 1,
                conflicting_path: '$',
                recovery_hint: 'reload the candidate and reapply',
              },
            },
          },
          { status: 409 },
        );
      }),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));
    const before = result.current.workingDocument;

    await act(async () => {
      await expect(
        result.current.nestPattern({ attachNodeId: 'draft', package: { id: 'prompt-step', version: '1.0.0' } }),
      ).rejects.toThrow();
    });

    expect(composeCalls).toBe(1);
    expect(result.current.conflict).toEqual({
      message: 'Candidate changed before this edit. Reload revision 1 and reapply the change.',
      baseRevision: 0,
      currentRevision: 1,
    });
    expect(result.current.workingDocument).toBe(before);

    // The recovery reads the canonical candidate at the revision the daemon holds and drops the
    // conflict, so the next op is applied against those bytes.
    await act(async () => {
      await result.current.reload();
    });
    expect(result.current.conflict).toBeNull();
    expect(result.current.workingDocument?.revision).toBe(1);
  });

  it('a failing recovery read surfaces a refusal and keeps the conflict (T4)', async () => {
    let candidateReads = 0;
    useHandlers(
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: PATTERN_ITEMS, pagination: { limit: 100, has_more: false } }),
      ),
      http.get(CANDIDATE_PATH, () => {
        candidateReads += 1;
        return candidateReads === 1
          ? HttpResponse.json(SEED)
          : HttpResponse.json(
              { error: { code: 'internal_error', message: 'candidate read failed' } },
              { status: 500 },
            );
      }),
      http.post(COMPOSE_PATH, () =>
        HttpResponse.json(
          {
            error: {
              code: 'strategy_candidate_conflict',
              message: 'Candidate changed before this edit. Reload revision 1 and reapply the change.',
              details: { strategy_id: STRATEGY_ID, base_revision: 0, current_revision: 1 },
            },
          },
          { status: 409 },
        ),
      ),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));

    await act(async () => {
      await expect(
        result.current.nestPattern({ attachNodeId: 'draft', package: { id: 'prompt-step', version: '1.0.0' } }),
      ).rejects.toThrow();
    });

    // The failing GET is reported in the dialog, not rejected into the void.
    await act(async () => {
      await result.current.reload();
    });
    expect(result.current.refusal).toEqual({
      reason: 'reload_failed',
      message: 'candidate read failed',
      details: {},
    });
    expect(result.current.conflict?.currentRevision).toBe(1);
    expect(result.current.workingDocument?.revision).toBe(0);
  });

  it('a named 422 refusal leaves the candidate and revision unchanged', async () => {
    seedHandlers();
    useHandlers(
      http.post(COMPOSE_PATH, () =>
        HttpResponse.json(
          {
            error: {
              code: 'invalid_input',
              message: 'compose refused: nest cycle',
              details: {
                code: 'compose_refused',
                reason: 'nest_cycle',
                nest_id: 'prompt-step',
                path: ['prompt-step@1.0.0', 'prompt-step@1.0.0'],
              },
            },
          },
          { status: 422 },
        ),
      ),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));
    const before = result.current.workingDocument;

    await act(async () => {
      await expect(
        result.current.nestPattern({ attachNodeId: 'draft', package: { id: 'prompt-step', version: '1.0.0' } }),
      ).rejects.toThrow();
    });

    expect(result.current.refusal?.reason).toBe('nest_cycle');
    expect(result.current.workingDocument).toBe(before);
    expect(result.current.revision).toBe(0);
  });

  it('binds a nested slot selected from its expanded parent and commits the binding', async () => {
    const patterns = [
      {
        package_dir: 'judge-step',
        id: 'judge-step',
        version: '2.0.0',
        description: 'Judge step',
        content_hash: 'abc',
        effect_class: 'read',
        slots: [{ name: 'tone', kind: 'string', required: true }],
        requires_capabilities: [],
        available: true,
        reason: null,
      },
      ...PATTERN_ITEMS,
    ];
    const seeded: WorkflowDocument = {
      ...nestedDocument(0),
      nodes: [
        ...nestedDocument(0).nodes,
        {
          id: 'prompt-step/judge',
          kind: 'pattern_slot',
          effect_class: 'read',
          pattern: { registry_id: 'judge-step', version: '2.0.0' },
        },
      ],
      nests: [
        ...nestedDocument(0).nests,
        {
          id: 'judge-region',
          parent_node: 'prompt-step/judge',
          expansion: 'expand_in_candidate',
          pin_policy: 'exact',
          child: { kind: 'pattern', registry_id: 'judge-step', version: '2.0.0' },
        },
      ],
    };
    const bindBodies: unknown[] = [];
    useHandlers(
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patterns, pagination: { limit: 100, has_more: false } }),
      ),
      http.get(CANDIDATE_PATH, () => HttpResponse.json(seeded)),
      http.post(COMPOSE_PATH, async ({ request }) => {
        bindBodies.push(await request.json());
        return HttpResponse.json({
          new_revision: 1,
          document: {
            ...seeded,
            revision: 1,
            nodes: seeded.nodes.map((node) =>
              node.id === 'prompt-step/judge' ? { ...node, bind: { tone: 'wry' } } : node,
            ),
          },
          validation_summary: [],
        });
      }),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));

    // The canvas renders the expanded parent, which emits the nested slot as its child.
    const graph = buildStrategyGraphFromIr(result.current.workingDocument!, {
      expandedSlots: new Set(['prompt-step-slot']),
      patterns,
      containment: new Map([['prompt-step/judge', 'prompt-step-slot']]),
    });
    const nested = graph.nodes.find((node) => node.id === 'prompt-step/judge');
    expect(nested?.parentId).toBe('prompt-step-slot');
    expect(nested?.data.nestId).toBe('judge-region');

    // Selecting it reaches the slot inspector, which binds through the hook.
    renderInApp(
      <SlotInspector
        node={nested!.data}
        patterns={patterns}
        isComposing={false}
        onBindSlot={result.current.bindSlot}
        onReplaceSubtree={result.current.replaceSubtree}
        onUnnest={result.current.unnest}
      />,
    );
    fireEvent.change(screen.getByTestId('slot-input-tone'), { target: { value: 'wry' } });
    fireEvent.click(screen.getByTestId('slot-bind-tone'));

    await waitFor(() =>
      expect(
        result.current.workingDocument?.nodes.find((node) => node.id === 'prompt-step/judge')?.bind
          ?.tone,
      ).toBe('wry'),
    );
    expect(bindBodies[0]).toMatchObject({
      op: 'bind_slot',
      payload: { nest_id: 'judge-region', slot: 'tone', value: 'wry' },
    });
  });

  it('reload keeps the document it already holds when the recovery read returns older bytes (T4 in-flight guard)', async () => {
    let candidateReads = 0;
    let releaseStale: () => void = () => {};
    const staleRead = new Promise<void>((resolve) => {
      releaseStale = resolve;
    });
    let composeCalls = 0;
    useHandlers(
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: PATTERN_ITEMS, pagination: { limit: 100, has_more: false } }),
      ),
      http.get(CANDIDATE_PATH, async () => {
        candidateReads += 1;
        if (candidateReads === 1) return HttpResponse.json(SEED);
        // The recovery read answers with revision 1 after a newer compose landed.
        await staleRead;
        return HttpResponse.json(nestedDocument(1));
      }),
      http.post(COMPOSE_PATH, () => {
        composeCalls += 1;
        return HttpResponse.json({
          new_revision: composeCalls === 1 ? 1 : 2,
          document: nestedDocument(composeCalls === 1 ? 1 : 2, { topic: 'water policy' }),
          validation_summary: [],
        });
      }),
    );

    const { result } = renderCompose();
    await waitFor(() => expect(result.current.workingDocument?.id).toBe(STRATEGY_ID));

    // A compose commits while the recovery GET is still in flight.
    let reloaded: Promise<void> = Promise.resolve();
    await act(async () => {
      reloaded = result.current.reload();
      await result.current.nestPattern({
        attachNodeId: 'draft',
        package: { id: 'prompt-step', version: '1.0.0' },
      });
      await result.current.bindSlot('prompt-step', 'topic', 'water policy');
    });
    expect(result.current.revision).toBe(2);

    releaseStale();
    await act(async () => {
      await reloaded;
    });

    // Revision 1 never overwrites the revision-2 bytes the compose committed.
    expect(result.current.workingDocument?.revision).toBe(2);
    expect(
      result.current.workingDocument?.nodes.find((node) => node.id === 'prompt-step-slot')?.bind
        ?.topic,
    ).toBe('water policy');
  });

  it('does nothing while disabled (flag off) — no reads, no mutations', () => {
    useHandlers(
      http.get(PATTERNS_PATH, () => HttpResponse.json({ items: [], pagination: { limit: 100, has_more: false } })),
      http.get(CANDIDATE_PATH, () => HttpResponse.json(candidateDocument())),
    );
    const { result } = renderHook(() => useStrategyCompose(STRATEGY_ID, false), {
      wrapper: makeWrapper(),
    });
    expect(result.current.workingDocument).toBeNull();
    expect(result.current.patternsQuery.fetchStatus).toBe('idle');
  });
});
