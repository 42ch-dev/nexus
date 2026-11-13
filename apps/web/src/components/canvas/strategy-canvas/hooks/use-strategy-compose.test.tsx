/**
 * Compose interactions (v239-p3-canvas-compose-ui T3, T3b, T4) at the hook
 * level over msw: every op sends the last-read `base_revision`; the nest
 * interaction authors the connecting edge the walk needs (T3b); a 409/422
 * leaves the working document unchanged, never auto-retries, and the 409
 * recovery names the missing candidate read route instead of inventing a
 * client-side cache (T4).
 */
import { renderHook, act, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { afterEach, describe, expect, it } from 'vitest';
import type { PropsWithChildren } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { BrowserClient } from '@/lib/nexus';
import { ClientProvider } from '@/lib/client-context';
import { useStrategyCompose } from '@/components/canvas/strategy-canvas/hooks/use-strategy-compose';
import { makeQueryClient } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { candidateDocument, servedIrDocument } from '@/test/simulation-fixture';

const STRATEGY_ID = 'user/demo';
const PATTERNS_PATH = '/v1/daemon/patterns';
const IR_PATH = `/v1/daemon/strategies/${encodeURIComponent(STRATEGY_ID)}/workflow-ir`;
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

/** Seed document plus the slot the nest op commits (server-side view: no edge). */
function nestedDocument(revision: number, bind: Record<string, unknown> = {}): WorkflowDocument {
  return {
    ...SEED,
    revision,
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
    http.get(IR_PATH, () => HttpResponse.json(SEED)),
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
        package: { id: 'prompt-step', version: '1.0.0' },
        expansion: 'expand_in_candidate',
      },
    });

    // The committed document carries no edge into the slot (the daemon writes
    // none); the interaction authored the connecting edge the walk needs.
    const committed = result.current.workingDocument!;
    const slot = committed.nodes.find((node) => node.id === 'prompt-step-slot');
    expect(slot?.kind).toBe('pattern_slot');
    expect(committed.revision).toBe(1);
    expect(committed.edges.some((edge) => edge.to === 'prompt-step-slot')).toBe(true);
    expect(result.current.unboundSlots).toEqual([
      { slotNodeId: 'prompt-step-slot', patternId: 'prompt-step', patternVersion: '1.0.0', slots: ['topic'] },
    ]);
  });

  it('bind_slot advances the revision and keeps the authored edge across the response', async () => {
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

  it('a stale base is the typed conflict: unchanged graph, no auto-retry, reload named unavailable (T4)', async () => {
    seedHandlers();
    let composeCalls = 0;
    useHandlers(
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
      reloadAvailable: false,
    });
    expect(result.current.workingDocument).toBe(before);
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

  it('does nothing while disabled (flag off) — no reads, no mutations', () => {
    useHandlers(
      http.get(PATTERNS_PATH, () => HttpResponse.json({ items: [], pagination: { limit: 100, has_more: false } })),
      http.get(IR_PATH, () => HttpResponse.json(candidateDocument())),
    );
    const { result } = renderHook(() => useStrategyCompose(STRATEGY_ID, false), {
      wrapper: makeWrapper(),
    });
    expect(result.current.workingDocument).toBeNull();
    expect(result.current.patternsQuery.fetchStatus).toBe('idle');
  });
});
