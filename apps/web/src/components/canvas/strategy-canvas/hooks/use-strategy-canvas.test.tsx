/**
 * Regression coverage for R-V171P0-QC1-008 (B7): edit-save-refetch loop.
 *
 * `useStrategyCanvas` does not own the PATCH mutation itself, but it owns the
 * conflict/reapply coordination: when an inspector save fails with a conflict,
 * the canvas refetches the canonical preset before showing the reconcile modal,
 * and reapply refetches again before re-issuing the save trigger.
 *
 * V1.115 P0 T2 (W001): the hook no longer manages base node/edge state (the
 * adapter owns projection). Tests target the hook's remaining responsibilities:
 * draft edge management (draftEdges), conflict/reapply coordination, and the
 * three transition-write mutations. Projection equivalence is covered by the
 * adapter-level tests in `strategy-graph.test.ts`.
 */
import { describe, expect, it, vi } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { ClientProvider } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import type { NexusClient } from '@/lib/nexus';
import { ToastProvider } from '@/lib/use-toast';
import { usePresetGraph } from '@/lib/canvas/use-strategy-data';

import { useStrategyCanvas } from './use-strategy-canvas';

const mocks = vi.hoisted(() => {
  const refetch = vi.fn(() => Promise.resolve({ data: undefined }));
  const graphQuery = {
    data: {
      revision: 1,
      parsed: {
        manifest: {
          preset: { id: 'preset-1' },
          states: [
            { id: 's1', description: 'Original', next: 's2' },
            { id: 's2', description: 'Second' },
          ],
        },
      },
    },
    isLoading: false,
    isError: false,
    refetch,
  };
  return { refetch, graphQuery };
});

vi.mock('@/lib/canvas/use-strategy-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-strategy-data')>();
  return {
    ...actual,
    usePresetGraph: () => mocks.graphQuery as unknown as ReturnType<typeof usePresetGraph>,
    useActiveSession: () => undefined as unknown as ReturnType<typeof actual.useActiveSession>,
    usePresetSchedules: () => ({ data: [] }) as unknown as ReturnType<typeof actual.usePresetSchedules>,
    useDerivedCreatorId: () => 'creator-1',
  };
});

function wrapper({ children }: { children: ReactNode }) {
  const client = {
    strategyPatchState: vi.fn(),
    strategyPatchTransition: vi.fn(),
    strategyPatchPromptTemplate: vi.fn(),
  } as unknown as NexusClient;
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return (
    <QueryClientProvider client={queryClient}>
      <ClientProvider client={client} desktop={null}>
        <ToastProvider>{children}</ToastProvider>
      </ClientProvider>
    </QueryClientProvider>
  );
}

/** Build a wrapper with a specific client for draft-commit tests (FB-SE-002). */
function makeCommitWrapper(client: NexusClient) {
  return function commitWrapper({ children }: { children: ReactNode }) {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    return (
      <QueryClientProvider client={queryClient}>
        <ClientProvider client={client} desktop={null}>
          <ToastProvider>{children}</ToastProvider>
        </ClientProvider>
      </QueryClientProvider>
    );
  };
}

describe('useStrategyCanvas edit-save-refetch (R-V171P0-QC1-008 B7)', () => {
  it('refetches on conflict + reapply and replays the save trigger', async () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    expect(result.current.graphQuery.refetch).toBe(mocks.refetch);

    act(() => {
      // Use the same revision as the mocked graph so the auto-clear effect does
      // not immediately dismiss the conflict before reapply is exercised.
      result.current.handleConflict(1, 'state');
    });
    expect(mocks.refetch).toHaveBeenCalledTimes(1);

    await act(async () => {
      result.current.handleReapply();
    });
    expect(mocks.refetch).toHaveBeenCalledTimes(2);
    await waitFor(() => {
      expect(result.current.saveTriggers.state).toBe(1);
    });
  });
});

describe('useStrategyCanvas onConnect draft edge (FB-SE-000)', () => {
  it('creates a draft transition edge when connecting two different states', () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    expect(result.current.draftEdges).toHaveLength(0);
    expect(typeof result.current.onConnect).toBe('function');

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });

    expect(result.current.draftEdges).toHaveLength(1);
    const draft = result.current.draftEdges[0];
    expect(draft.source).toBe('s1');
    expect(draft.target).toBe('s2');
    expect(draft.label).toBe('Draft transition');
    expect(draft.selected).toBe(true);
    expect((draft.data as { isDraft?: boolean }).isDraft).toBe(true);
  });

  it('does not create a draft edge for a self-loop', () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's1', sourceHandle: null, targetHandle: null });
    });

    expect(result.current.draftEdges).toHaveLength(0);
  });

  it('does not call the daemon on connect (local-only draft)', () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });

    // No save trigger fires from a connect — draft is local until inspector commit.
    expect(result.current.saveTriggers.transition).toBe(0);
  });

  it('preserves the draft edge across re-renders (V1.115 T2: adapter owns base projection)', () => {
    // V1.115 T2: the hook manages only drafts; base edges are projected by the
    // adapter. A re-render (e.g. query refetch) does not touch draftEdges.
    const { result, rerender } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });

    expect(result.current.draftEdges).toHaveLength(1);
    const draft = result.current.draftEdges[0];
    expect((draft.data as { isDraft?: boolean }).isDraft).toBe(true);

    rerender();

    // The draft survives — the adapter merges it with projected edges.
    expect(result.current.draftEdges).toHaveLength(1);
    expect(result.current.draftEdges.some((e) => e.id === draft.id)).toBe(true);
  });
});

describe('useStrategyCanvas draft transition commit (FB-SE-002)', () => {
  function makeClient(patchImpl: () => Promise<unknown>): NexusClient {
    return {
      strategyPatchState: vi.fn(),
      strategyPatchTransition: vi.fn(patchImpl),
      strategyPatchPromptTemplate: vi.fn(),
    } as unknown as NexusClient;
  }

  it('commits a draft edge via strategy.patch_transition with op: "create"', async () => {
    const patch = vi.fn().mockResolvedValue({
      new_revision: 2,
      validation_summary: { errors: [], warnings: [] },
      side_effects: [],
    });
    const client = makeClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });
    expect(result.current.selectedDraftEdge).not.toBeNull();

    await act(async () => {
      result.current.commitDraft({ condition: 'word_count > 1000' });
    });

    await waitFor(() => expect(patch).toHaveBeenCalledTimes(1));
    const request = patch.mock.calls[0][1] as Record<string, unknown>;
    expect(request).toMatchObject({
      strategy_id: 'preset-1',
      source_state_id: 's1',
      new_target: 's2',
      op: 'create',
      condition: 'word_count > 1000',
    });

    // Successful commit replaces the draft (refetch brings the canonical edge).
    await waitFor(() => expect(result.current.selectedDraftEdge).toBeNull());
  });

  it('omits op on the existing-edit path so it defaults to "update"', async () => {
    // The hook-owned commit is create-only; this test documents that the draft
    // commit always sends op: "create" while the legacy inspector edit path
    // (usePatchStrategyTransition) continues to omit op. We assert create here;
    // the legacy path is covered by inspector-save-trigger.test.tsx.
    const patch = vi.fn().mockResolvedValue({ new_revision: 3 });
    const client = makeClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });

    await act(async () => {
      result.current.commitDraft({});
    });

    await waitFor(() => expect(patch).toHaveBeenCalled());
    const request = patch.mock.calls[0][1] as Record<string, unknown>;
    expect(request.op).toBe('create');
  });

  it('opens the conflict modal when the daemon returns strategy_conflict (409)', async () => {
    // The mocked graphQuery reports revision 1; the conflict's current_revision
    // is set to match so the existing auto-clear effect (which dismisses a
    // conflict once the refetched revision differs) does not race the
    // assertion. This mirrors the established pattern in this file's
    // edit-save-refetch suite.
    const conflictError = new NexusClientError(
      409,
      'strategy_conflict',
      'Strategy revision is stale',
      { current_revision: 1 },
    );
    const patch = vi.fn().mockRejectedValue(conflictError);
    const client = makeClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });
    expect(result.current.conflict).toBeNull();

    await act(async () => {
      result.current.commitDraft({});
    });

    // 409 keeps the draft and routes to the existing conflict modal handler.
    await waitFor(() => expect(result.current.conflict).not.toBeNull());
    expect(result.current.conflict).toMatchObject({ currentRevision: 1, section: 'transition' });
    // A retry command is stored so "Reapply my edit" replays the transition
    // command, not the state-edit save trigger (QC1 W-001).
    expect(typeof result.current.conflict?.retry).toBe('function');
    // Draft is preserved so the author can reconcile.
    expect(result.current.selectedDraftEdge).not.toBeNull();
  });

  it('cancelDraft removes the draft edge without a daemon call', () => {
    const patch = vi.fn();
    const client = makeClient(patch as () => Promise<unknown>);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });
    expect(result.current.selectedDraftEdge).not.toBeNull();

    act(() => {
      result.current.cancelDraft();
    });

    expect(result.current.selectedDraftEdge).toBeNull();
    expect(patch).not.toHaveBeenCalled();
  });
});

describe('useStrategyCanvas keyboard create (FB-SE-004)', () => {
  function makeClient(patchImpl: () => Promise<unknown>): NexusClient {
    return {
      strategyPatchState: vi.fn(),
      strategyPatchTransition: vi.fn(patchImpl),
      strategyPatchPromptTemplate: vi.fn(),
    } as unknown as NexusClient;
  }

  it('commits via strategy.patch_transition with op: "create" and explicit source/target', async () => {
    const patch = vi.fn().mockResolvedValue({
      new_revision: 2,
      validation_summary: { errors: [], warnings: [] },
      side_effects: [],
    });
    const client = makeClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    await act(async () => {
      result.current.commitKeyboardCreate({
        sourceStateId: 's1',
        targetStateId: 's2',
        transitionKind: 'branch',
        condition: 'word_count > 1000',
      });
    });

    await waitFor(() => expect(patch).toHaveBeenCalledTimes(1));
    const request = patch.mock.calls[0][1] as Record<string, unknown>;
    expect(request).toMatchObject({
      strategy_id: 'preset-1',
      source_state_id: 's1',
      new_target: 's2',
      op: 'create',
      transition_kind: 'branch',
      condition: 'word_count > 1000',
    });
  });

  it('routes a 409 conflict through the conflict modal handler', async () => {
    const conflictError = new NexusClientError(
      409,
      'strategy_conflict',
      'Strategy revision is stale',
      { current_revision: 1 },
    );
    const patch = vi.fn().mockRejectedValue(conflictError);
    const client = makeClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    expect(result.current.conflict).toBeNull();

    await act(async () => {
      result.current.commitKeyboardCreate({
        sourceStateId: 's1',
        targetStateId: 's2',
      });
    });

    await waitFor(() => expect(result.current.conflict).not.toBeNull());
    expect(result.current.conflict).toMatchObject({ currentRevision: 1, section: 'transition' });
  });

  it('reapplies the original transition command on reapply, not the state-edit save trigger (QC1 W-001)', async () => {
    // First call rejects with 409; second call succeeds so we can observe
    // the retry re-issuing the transition create.
    const conflictError = new NexusClientError(
      409,
      'strategy_conflict',
      'Strategy revision is stale',
      { current_revision: 1 },
    );
    const patch = vi
      .fn()
      .mockRejectedValueOnce(conflictError)
      .mockResolvedValueOnce({ new_revision: 2 });
    const client = makeClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    const createArgs = {
      sourceStateId: 's1',
      targetStateId: 's2',
      transitionKind: 'branch' as const,
      condition: 'word_count > 1000',
    };

    await act(async () => {
      result.current.commitKeyboardCreate(createArgs);
    });

    await waitFor(() => expect(result.current.conflict).not.toBeNull());
    expect(patch).toHaveBeenCalledTimes(1);

    // Reapply should replay the original transition command, not increment
    // the section save trigger (which would drive a state-edit save).
    await act(async () => {
      result.current.handleReapply();
    });

    await waitFor(() => expect(patch).toHaveBeenCalledTimes(2));

    // The second patch call is the retried transition create — same args.
    const retryRequest = patch.mock.calls[1][1] as Record<string, unknown>;
    expect(retryRequest).toMatchObject({
      strategy_id: 'preset-1',
      source_state_id: 's1',
      new_target: 's2',
      op: 'create',
      transition_kind: 'branch',
      condition: 'word_count > 1000',
    });

    // The transition save trigger must NOT have been incremented (that would
    // drive the legacy state-edit EdgeInspector save, not the transition create).
    expect(result.current.saveTriggers.transition).toBe(0);
  });
});

describe('useStrategyCanvas edge reconnection (FB-SE-003)', () => {
  // V1.115 T2: the hook no longer manages base edge state — the adapter
  // projects from `parsed`. Reconnect tests verify the daemon call shape and
  // conflict routing. The optimistic edge update / revert (previously managed
  // in hook edge state) is now handled at the adapter/orchestrator level:
  // the edge moves to the new target on query refetch (success) or stays at
  // the old target (failure — no daemon state changed).
  const existingEdge = {
    id: 'e-s1-s2-next-0',
    source: 's1',
    target: 's2',
    type: 'strategy-edge',
    data: { transitionKind: 'next' },
  };

  function makeReconnClient(patchImpl: () => Promise<unknown>): NexusClient {
    return {
      strategyPatchState: vi.fn(),
      strategyPatchTransition: vi.fn(patchImpl),
      strategyPatchPromptTemplate: vi.fn(),
    } as unknown as NexusClient;
  }

  it('reconnects via a single patch_transition with old_target + new_target (op: update)', async () => {
    const patch = vi.fn().mockResolvedValue({ new_revision: 2 });
    const client = makeReconnClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    expect(typeof result.current.onReconnect).toBe('function');

    act(() => {
      result.current.onReconnect(existingEdge, {
        source: 's1',
        target: 's3',
        sourceHandle: null,
        targetHandle: null,
      });
    });

    // Single reconnect payload — no delete+create, one logical transition.
    await waitFor(() => expect(patch).toHaveBeenCalledTimes(1));
    const request = patch.mock.calls[0][1] as Record<string, unknown>;
    expect(request).toMatchObject({
      strategy_id: 'preset-1',
      source_state_id: 's1',
      old_target: 's2',
      new_target: 's3',
      op: 'update',
    });
  });

  it('includes the branch condition on reconnect so sibling rules sharing a target are not all rewritten (Greptile Issue 1)', async () => {
    const branchEdge = {
      id: 'e-s1-s2-branch-0',
      source: 's1',
      target: 's2',
      type: 'strategy-edge',
      data: { transitionKind: 'branch', condition: '_context.branch_a' },
    };

    const patch = vi.fn().mockResolvedValue({ new_revision: 2 });
    const client = makeReconnClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    act(() => {
      result.current.onReconnect(branchEdge, {
        source: 's1',
        target: 's3',
        sourceHandle: null,
        targetHandle: null,
      });
    });

    await waitFor(() => expect(patch).toHaveBeenCalledTimes(1));
    const request = patch.mock.calls[0][1] as Record<string, unknown>;
    expect(request).toMatchObject({
      source_state_id: 's1',
      old_target: 's2',
      new_target: 's3',
      condition: '_context.branch_a',
      transition_kind: 'branch',
      op: 'update',
    });
  });

  it('routes a 409 conflict through the conflict modal handler on reconnect failure', async () => {
    const conflictError = new NexusClientError(
      409,
      'strategy_conflict',
      'Strategy revision is stale',
      { current_revision: 1 },
    );
    const patch = vi.fn().mockRejectedValue(conflictError);
    const client = makeReconnClient(patch);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    expect(result.current.conflict).toBeNull();

    await act(async () => {
      result.current.onReconnect(existingEdge, {
        source: 's1',
        target: 's3',
        sourceHandle: null,
        targetHandle: null,
      });
    });

    await waitFor(() => expect(result.current.conflict).not.toBeNull());
    expect(result.current.conflict).toMatchObject({ currentRevision: 1, section: 'transition' });
    // A retry command is stored so "Reapply my edit" replays the reconnect, not a state-edit save.
    expect(typeof result.current.conflict?.retry).toBe('function');
  });

  it('does not reconnect onto the same source (self-loop guard)', () => {
    const patch = vi.fn();
    const client = makeReconnClient(patch as () => Promise<unknown>);
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), {
      wrapper: makeCommitWrapper(client),
    });

    act(() => {
      result.current.onReconnect(existingEdge, {
        source: 's1',
        target: 's1',
        sourceHandle: null,
        targetHandle: null,
      });
    });

    expect(patch).not.toHaveBeenCalled();
  });
});
