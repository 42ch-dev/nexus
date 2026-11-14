/**
 * Strategy canvas — compose path end-to-end and flag-off parity
 * (v239-p3-canvas-compose-ui T3/T3b/T4/T6/T7).
 *
 * Flag on (explicit selection over the content-creator default): the header
 * shows `Staging`; Add pattern → nest op guarded by the last-read revision;
 * Simulate walks the working document whose authored connecting edge wires
 * the slot (T3b); an unbound required slot gates Approve with its named
 * reason; Approve posts the real approve route and renders the frozen
 * snapshot. A 409 surfaces the conflict, never auto-retries.
 *
 * Flag off (T7): no compose reads fire, no Staging badge, and Approve keeps
 * the shipped reason-bearing disabled disclosure — asserted, not inspected.
 */
import { cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { BrowserClient } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { COMPOSE_FLAG_STORAGE_KEY } from '@/lib/canvas/strategy-compose-flag';
import { useHandlers } from '@/test/msw-server';
import { servedIrDocument, simulationTrajectory } from '@/test/simulation-fixture';
import { makeQueryClient, renderInApp } from '@/test/test-providers';

const shellProps = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));

vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: (props: Record<string, unknown>) => {
    shellProps.current = props;
    return null;
  },
  useNodeChangeHandler: () => () => {},
}));

const strategyState = vi.hoisted(() => ({
  graphQuery: {
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
    data: {
      revision: 1,
      parsed: {
        manifest: { preset: { id: 'novel-writing' }, states: [] },
        problems: [],
      },
    },
  },
}));

vi.mock('@/components/canvas/strategy-canvas/hooks/use-strategy-canvas', () => ({
  useStrategyCanvas: () => ({
    graphQuery: strategyState.graphQuery,
    activeSession: null,
    creatorId: 'c1',
    baseRevision: 1,
    activeScheduleId: undefined,
    draftEdges: [],
    onConnect: () => {},
    onReconnect: () => {},
    form: { label: '', description: '', nextTarget: '', promptBody: '' },
    setForm: () => {},
    saveStatuses: {},
    setSaveStatuses: () => {},
    setActiveSection: () => {},
    conflict: null,
    setConflict: () => {},
    saveTriggers: {},
    workingRevisionRef: { current: 1 },
    handleConflict: () => {},
    handleReapply: () => {},
    selectedDraftEdge: null,
    draftSourceState: null,
    commitDraft: () => {},
    isCommittingDraft: false,
    cancelDraft: () => {},
    commitKeyboardCreate: () => {},
    isCommittingKeyboardCreate: false,
    isReconnecting: false,
  }),
}));

vi.mock('@/components/canvas/strategy-canvas/inspectors/state-inspector', () => ({
  StateInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspectors/edge-inspector', () => ({
  EdgeInspector: () => null,
  DraftEdgeInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspectors/prompt-inspector', () => ({
  PromptInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspector-panel', () => ({
  InspectorPanel: () => null,
  StrategyConflictModal: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/state-machine', () => ({
  ValidationPanel: () => null,
  RevisionBadge: () => null,
  ArtifactsList: () => null,
  originalFormOf: () => ({ label: '', description: '', nextTarget: '', promptBody: '' }),
  templateRefOf: () => undefined,
  isSectionDirty: () => false,
}));
vi.mock('@/components/canvas/strategy-alt-view', () => ({
  StrategyAltView: () => null,
}));
vi.mock('@/components/canvas/strategy-nodes', () => ({
  strategyNodeTypes: {},
}));
vi.mock('@/components/canvas/idea-input', () => ({
  IdeaInput: () => null,
}));

const STRATEGY_ID = 'novel-writing';
const IR_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/workflow-ir`;
const CANDIDATE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/candidate`;
const COMPOSE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/compose`;
const SIMULATE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/simulate`;
const APPROVE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/approve`;
const APPLY_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/apply`;
const APPLIED_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/applied`;
const PATTERNS_PATH = '/v1/daemon/patterns';

const SEED = servedIrDocument({ id: STRATEGY_ID });

function patternItems(requiredSlot: boolean) {
  return [
    {
      package_dir: 'prompt-step',
      id: 'prompt-step',
      version: '1.0.0',
      description: 'Prompt step',
      content_hash: 'abc',
      effect_class: 'external',
      slots: requiredSlot ? [{ name: 'topic', kind: 'string', required: true }] : [],
      requires_capabilities: [],
      available: true,
      reason: null,
    },
  ];
}

function nestedResponse(bind: Record<string, unknown> = {}) {
  return {
    new_revision: 1,
    document: {
      ...SEED,
      revision: 1,
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
      // The daemon writes the connecting edge `nest` commits (`attach_to`).
      edges: [
        ...SEED.edges,
        { id: 'draft->prompt-step-slot', from: 'draft', to: 'prompt-step-slot', kind: 'linear' },
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
    },
    validation_summary: [] as unknown[],
  };
}

function renderCanvas(queryClient?: QueryClient) {
  renderInApp(<StrategyCanvas presetId={STRATEGY_ID} />, {
    client: new BrowserClient(),
    queryClient,
  });
}

beforeEach(() => {
  window.localStorage.setItem(COMPOSE_FLAG_STORAGE_KEY, 'on');
});

afterEach(() => {
  cleanup();
  window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
});

describe('StrategyCanvas — compose path (flag on)', () => {
  it('shows Staging, nests a pattern guarded by the last-read revision, and simulates the wired slot', async () => {
    const composeBodies: unknown[] = [];
    const simulateBodies: unknown[] = [];
    useHandlers(
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patternItems(true), pagination: { limit: 100, has_more: false } }),
      ),
      http.post(COMPOSE_PATH, async ({ request }) => {
        composeBodies.push(await request.json());
        return HttpResponse.json(nestedResponse());
      }),
      http.post(SIMULATE_PATH, async ({ request }) => {
        simulateBodies.push(await request.json());
        return HttpResponse.json(
          simulationTrajectory({ nodes_visited: ['prompt-step-slot', 'done', 'draft', 'routing'] }),
        );
      }),
    );

    renderCanvas();
    expect(await screen.findByTestId('strategy-staging')).toBeInTheDocument();
    expect(screen.getByTestId('strategy-add-pattern')).toBeInTheDocument();
    // One structural authority: the YAML transition author is gone on the compose path,
    // and no canvas gesture can start a structural edit outside the candidate.
    expect(screen.queryByRole('button', { name: 'Create' })).not.toBeInTheDocument();
    expect(shellProps.current.onConnect).toBeUndefined();
    expect(shellProps.current.onReconnect).toBeUndefined();

    // Nest: pick the pattern and commit.
    fireEvent.click(screen.getByTestId('strategy-add-pattern'));
    fireEvent.click(await screen.findByTestId('pattern-option-prompt-step'));
    fireEvent.click(screen.getByTestId('pattern-nest'));

    await waitFor(() => expect(composeBodies).toHaveLength(1));
    expect(composeBodies[0]).toMatchObject({
      base_revision: 0,
      op: 'nest',
      payload: {
        parent_node: 'prompt-step-slot',
        attach_to: 'draft',
        package: { id: 'prompt-step', version: '1.0.0' },
      },
    });

    // Simulate walks the committed document — the edge `nest` wrote from the
    // canvas's attach target reaches the slot, so no client-authored edge (T3b).
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(simulateBodies).toHaveLength(1));
    const walked = simulateBodies[0];
    if (typeof walked !== 'object' || walked === null || !('document' in walked)) {
      throw new Error('simulate body is not a document request');
    }
    const walkedDoc = walked.document;
    if (typeof walkedDoc !== 'object' || walkedDoc === null || !('edges' in walkedDoc)) {
      throw new Error('simulate body has no edges');
    }
    const walkedEdges = walkedDoc.edges;
    if (!Array.isArray(walkedEdges)) throw new Error('simulate edges are not a list');
    expect(
      walkedEdges.some(
        (edge) =>
          typeof edge === 'object' &&
          edge !== null &&
          'to' in edge &&
          edge.to === 'prompt-step-slot',
      ),
    ).toBe(true);

    // The unbound required slot gates Approve with its named reason.
    await waitFor(() => expect(screen.getByTestId('approve-gate')).toBeInTheDocument());
    expect(screen.getByTestId('approve-gate')).toHaveTextContent(
      'Bind slot topic on prompt-step before approval.',
    );
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
  });

  it('approves through the real route and renders the frozen snapshot', async () => {
    const approveBodies: unknown[] = [];
    useHandlers(
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patternItems(false), pagination: { limit: 100, has_more: false } }),
      ),
      http.post(COMPOSE_PATH, () => HttpResponse.json(nestedResponse())),
      http.post(SIMULATE_PATH, () =>
        HttpResponse.json(simulationTrajectory({ nodes_visited: ['prompt-step-slot'] })),
      ),
      http.post(APPROVE_PATH, async ({ request }) => {
        approveBodies.push(await request.json());
        return HttpResponse.json({
          approval_id: 'apr_42',
          strategy_id: STRATEGY_ID,
          candidate_revision: 1,
          document: nestedResponse().document,
          content_hash: 'hash-9',
          trajectory: simulationTrajectory({ ir_content_hash: 'hash-9' }),
          pins: [{ id: 'prompt-step', version: '1.0.0', content_hash: 'abc' }],
          approver: 'creator-1',
          approved_at: '2026-11-13T00:00:00Z',
        });
      }),
    );

    renderCanvas();
    fireEvent.click(await screen.findByTestId('strategy-add-pattern'));
    fireEvent.click(await screen.findByTestId('pattern-option-prompt-step'));
    fireEvent.click(screen.getByTestId('pattern-nest'));
    await waitFor(() => expect(screen.queryByTestId('pattern-nest')).not.toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Approve' })).toBeEnabled());

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => expect(approveBodies).toHaveLength(1));
    expect(approveBodies[0]).toMatchObject({ base_revision: 1, trajectory: { ir_content_hash: 'hash-1' } });

    await waitFor(() => expect(screen.getByTestId('approve-approved')).toBeInTheDocument());
    expect(screen.getByTestId('approve-approved')).toHaveTextContent('apr_42');
    expect(screen.getByTestId('approve-pin')).toHaveTextContent('prompt-step@1.0.0');
    expect(screen.getByTestId('approve-simulation-identity')).toHaveTextContent('hash-9');
    expect(screen.getByTestId('approve-approved')).toHaveTextContent('does not publish, apply');
  });

  it('invalidates the applied-status read after a successful approve (the D9 badge source)', async () => {
    useHandlers(
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patternItems(false), pagination: { limit: 100, has_more: false } }),
      ),
      http.post(COMPOSE_PATH, () => HttpResponse.json(nestedResponse())),
      http.post(SIMULATE_PATH, () =>
        HttpResponse.json(simulationTrajectory({ nodes_visited: ['prompt-step-slot'] })),
      ),
      http.get(APPLIED_PATH, () =>
        HttpResponse.json({ current_apply: null, newest_approval: null }),
      ),
      http.post(APPROVE_PATH, () =>
        HttpResponse.json({
          approval_id: 'apr_42',
          strategy_id: STRATEGY_ID,
          candidate_revision: 1,
          document: nestedResponse().document,
          content_hash: 'hash-9',
          trajectory: simulationTrajectory({ ir_content_hash: 'hash-9' }),
          pins: [{ id: 'prompt-step', version: '1.0.0', content_hash: 'abc' }],
          approver: 'creator-1',
          approved_at: '2026-11-13T00:00:00Z',
        }),
      ),
    );

    const queryClient = makeQueryClient();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    renderCanvas(queryClient);
    fireEvent.click(await screen.findByTestId('strategy-add-pattern'));
    fireEvent.click(await screen.findByTestId('pattern-option-prompt-step'));
    fireEvent.click(screen.getByTestId('pattern-nest'));
    await waitFor(() => expect(screen.queryByTestId('pattern-nest')).not.toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Approve' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => expect(screen.getByTestId('approve-approved')).toBeInTheDocument());

    // Approving moves newest_approval; without this invalidation the badge
    // keeps comparing against the stale read.
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: queryKeys.strategies.applied(STRATEGY_ID),
      }),
    );
  });

  it('a second approve dismisses the previous apply block instead of stacking a stale one', async () => {
    let approveCalls = 0;
    useHandlers(
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patternItems(false), pagination: { limit: 100, has_more: false } }),
      ),
      http.post(COMPOSE_PATH, () => HttpResponse.json(nestedResponse())),
      http.post(SIMULATE_PATH, () =>
        HttpResponse.json(simulationTrajectory({ nodes_visited: ['prompt-step-slot'] })),
      ),
      http.get(APPLIED_PATH, () =>
        HttpResponse.json({ current_apply: null, newest_approval: null }),
      ),
      http.post(APPROVE_PATH, () => {
        approveCalls += 1;
        return HttpResponse.json({
          approval_id: approveCalls === 1 ? 'apr_42' : 'apr_43',
          strategy_id: STRATEGY_ID,
          candidate_revision: approveCalls,
          document: nestedResponse().document,
          content_hash: `hash-${approveCalls}`,
          trajectory: simulationTrajectory({ ir_content_hash: `hash-${approveCalls}` }),
          pins: [{ id: 'prompt-step', version: '1.0.0', content_hash: 'abc' }],
          approver: 'creator-1',
          approved_at: '2026-11-13T00:00:00Z',
        });
      }),
      http.post(APPLY_PATH, async ({ request }) => {
        const body = (await request.json()) as { approval_content_hash: string };
        return HttpResponse.json({
          apply_id: 'app_1',
          approval_id: 'apr_42',
          content_hash: body.approval_content_hash,
          applied_at: '2026-11-13T01:00:00Z',
          supersedes_apply_id: null,
        });
      }),
    );

    renderCanvas();
    fireEvent.click(await screen.findByTestId('strategy-add-pattern'));
    fireEvent.click(await screen.findByTestId('pattern-option-prompt-step'));
    fireEvent.click(screen.getByTestId('pattern-nest'));
    await waitFor(() => expect(screen.queryByTestId('pattern-nest')).not.toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Approve' })).toBeEnabled());

    // Approve A → apply A → approve B: B's block replaces A's, they never stack.
    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => expect(screen.getByTestId('approve-approved')).toHaveTextContent('apr_42'));
    fireEvent.click(screen.getByRole('button', { name: 'Apply snapshot' }));
    await waitFor(() => expect(screen.getByTestId('apply-applied')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => expect(screen.getByTestId('approve-approved')).toHaveTextContent('apr_43'));
    // The stale "This approval is the running identity" block from apply A is gone.
    expect(screen.queryByTestId('apply-applied')).toBeNull();
  });

  it('surfaces a 409 conflict, leaves the graph alone, and never auto-retries', async () => {
    let composeCalls = 0;
    let candidateReads = 0;
    useHandlers(
      http.get(CANDIDATE_PATH, () => {
        candidateReads += 1;
        return HttpResponse.json(candidateReads === 1 ? SEED : { ...SEED, revision: 7 });
      }),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patternItems(true), pagination: { limit: 100, has_more: false } }),
      ),
      http.post(COMPOSE_PATH, () => {
        composeCalls += 1;
        return HttpResponse.json(
          {
            error: {
              code: 'strategy_candidate_conflict',
              message: 'Candidate changed before this edit. Reload revision 7 and reapply the change.',
              details: {
                strategy_id: STRATEGY_ID,
                base_revision: 0,
                current_revision: 7,
                conflicting_path: '$',
                recovery_hint: 'reload the candidate and reapply',
              },
            },
          },
          { status: 409 },
        );
      }),
    );

    renderCanvas();
    fireEvent.click(await screen.findByTestId('strategy-add-pattern'));
    fireEvent.click(await screen.findByTestId('pattern-option-prompt-step'));
    fireEvent.click(screen.getByTestId('pattern-nest'));

    await waitFor(() => expect(screen.getByTestId('compose-conflict')).toBeInTheDocument());
    expect(screen.getByTestId('compose-conflict')).toHaveTextContent(
      'Candidate changed before this edit. Reload revision 7 and reapply the change.',
    );
    // The recovery is a real reload of the canonical candidate at the revision
    // the daemon holds, so the user can reapply the edit against those bytes.
    expect(screen.getByTestId('compose-conflict-reload')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('compose-conflict-reload'));
    await waitFor(() =>
      expect(screen.queryByTestId('compose-conflict')).not.toBeInTheDocument(),
    );
    expect(candidateReads).toBe(2);
    // Nothing auto-retried the refused op.
    expect(composeCalls).toBe(1);
  });

  it('renders the refusal alert when the 409 recovery read fails', async () => {
    let candidateReads = 0;
    useHandlers(
      http.get(CANDIDATE_PATH, () => {
        candidateReads += 1;
        return candidateReads === 1
          ? HttpResponse.json(SEED)
          : HttpResponse.json(
              { error: { code: 'internal_error', message: 'candidate read failed' } },
              { status: 500 },
            );
      }),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: patternItems(true), pagination: { limit: 100, has_more: false } }),
      ),
      http.post(COMPOSE_PATH, () =>
        HttpResponse.json(
          {
            error: {
              code: 'strategy_candidate_conflict',
              message: 'Candidate changed before this edit. Reload revision 7 and reapply the change.',
              details: { strategy_id: STRATEGY_ID, base_revision: 0, current_revision: 7 },
            },
          },
          { status: 409 },
        ),
      ),
    );

    renderCanvas();
    fireEvent.click(await screen.findByTestId('strategy-add-pattern'));
    fireEvent.click(await screen.findByTestId('pattern-option-prompt-step'));
    fireEvent.click(screen.getByTestId('pattern-nest'));

    await waitFor(() => expect(screen.getByTestId('compose-conflict')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('compose-conflict-reload'));

    // The failed recovery is named in the dialog instead of a rejected void click.
    const refusal = await screen.findByTestId('compose-refusal');
    expect(refusal).toHaveAttribute('role', 'alert');
    expect(refusal).toHaveTextContent('candidate read failed');
    // The conflict it could not clear is still on screen.
    expect(screen.getByTestId('compose-conflict')).toBeInTheDocument();
    expect(candidateReads).toBe(2);
  });

  it('names a failed pattern listing instead of presenting it as an empty registry', async () => {
    useHandlers(
      http.get(CANDIDATE_PATH, () => HttpResponse.json(SEED)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json(
          { error: { code: 'internal', message: 'registry read failed' } },
          { status: 500 },
        ),
      ),
    );

    renderCanvas();
    fireEvent.click(await screen.findByTestId('strategy-add-pattern'));

    expect(await screen.findByTestId('pattern-picker-error')).toHaveTextContent(
      'registry read failed',
    );
  });

  it('refuses to claim completeness at the approve gate when the listing is unreadable', async () => {
    useHandlers(
      // The candidate does pin a required slot, but the failed listing leaves
      // its slots unreadable: the gate must name the failure, not "[nothing] unbound".
      http.get(CANDIDATE_PATH, () => HttpResponse.json(nestedResponse().document)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json(
          { error: { code: 'internal', message: 'registry read failed' } },
          { status: 500 },
        ),
      ),
      http.post(SIMULATE_PATH, () => HttpResponse.json(simulationTrajectory())),
    );

    renderCanvas();

    // The disclosure opens on Simulate; the gate names the listing failure.
    fireEvent.click(await screen.findByRole('button', { name: 'Simulate…' }));
    expect(await screen.findByTestId('approve-gate')).toHaveTextContent(
      'The pattern listing is unreadable: registry read failed.',
    );
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
  });

  it('keeps Approve enabled when a background refetch of the listing fails with items retained', async () => {
    let patternReads = 0;
    useHandlers(
      // The slot is bound, so the first read's items prove the candidate complete.
      http.get(CANDIDATE_PATH, () => HttpResponse.json(nestedResponse({ topic: 'x' }).document)),
      http.get(PATTERNS_PATH, () => {
        patternReads += 1;
        return patternReads === 1
          ? HttpResponse.json({
              items: patternItems(true),
              pagination: { limit: 100, has_more: false },
            })
          : HttpResponse.json(
              { error: { code: 'internal', message: 'registry reread failed' } },
              { status: 500 },
            );
      }),
      http.post(SIMULATE_PATH, () => HttpResponse.json(simulationTrajectory())),
    );

    const queryClient = makeQueryClient();
    renderCanvas(queryClient);

    fireEvent.click(await screen.findByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Approve' })).toBeEnabled());

    // The refetch fails, yet the last read's items are still in hand: the slots
    // are known, so the gate must not claim them unknown.
    await queryClient.refetchQueries({ queryKey: ['canvas', 'strategy-compose', 'patterns'] });
    await waitFor(() => expect(patternReads).toBe(2));

    expect(screen.queryByTestId('approve-gate')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Approve' })).toBeEnabled();
  });
});

describe('StrategyCanvas — flag-off parity (T7)', () => {
  it('renders exactly as today: no compose reads, no Staging, disabled reason-bearing Approve', async () => {
    window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
    let irReads = 0;
    let patternReads = 0;
    let composeCalls = 0;
    useHandlers(
      http.get(IR_PATH, () => {
        irReads += 1;
        return HttpResponse.json(SEED);
      }),
      http.get(PATTERNS_PATH, () => {
        patternReads += 1;
        return HttpResponse.json({ items: [], pagination: { limit: 100, has_more: false } });
      }),
      http.post(COMPOSE_PATH, () => {
        composeCalls += 1;
        return HttpResponse.json(nestedResponse());
      }),
      http.post(SIMULATE_PATH, () => HttpResponse.json(simulationTrajectory())),
    );

    renderCanvas();

    // No Staging badge, no Add pattern entry point.
    expect(screen.queryByTestId('strategy-staging')).not.toBeInTheDocument();
    expect(screen.queryByTestId('strategy-add-pattern')).not.toBeInTheDocument();
    // The compose seed/patterns reads never fire with the flag off.
    expect(irReads).toBe(0);
    expect(patternReads).toBe(0);

    // The legacy transition author and both connection gestures stay exactly where they were.
    expect(screen.getByRole('button', { name: 'Create' })).toBeInTheDocument();
    expect(typeof shellProps.current.onConnect).toBe('function');
    expect(typeof shellProps.current.onReconnect).toBe('function');

    // Today's Simulate behaviour: resolve the installed preset IR and walk it.
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument());
    expect(irReads).toBe(1);

    // Approve stays the disabled disclosure with its shipped reason.
    expect(screen.getByTestId('approve-unavailable')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
    expect(composeCalls).toBe(0);
  });
});
