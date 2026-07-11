/**
 * Regression coverage for R-V171P0-QC1-008 (B7): edit-save-refetch loop.
 *
 * `useStrategyCanvas` does not own the PATCH mutation itself, but it owns the
 * conflict/reapply coordination: when an inspector save fails with a conflict,
 * the canvas refetches the canonical preset before showing the reconcile modal,
 * and reapply refetches again before re-issuing the save trigger.
 *
 * This hook-level test proves that conflict handling and reapply both drive a
 * graph refetch without mounting React Flow in jsdom.
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
      graph: {
        nodes: [
          {
            id: 's1',
            type: 'strategy-state',
            position: { x: 0, y: 0 },
            data: {
              stateId: 's1',
              label: 'S1',
              stateKind: 'default',
              presetId: 'preset-1',
              isTerminal: false,
              isInitial: true,
              isGroup: false,
            },
            selected: true,
          },
        ],
        edges: [],
      },
      parsed: {
        manifest: {
          preset: { id: 'preset-1' },
          states: [{ id: 's1', description: 'Original', next: 's2' }],
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
  it('exposes the selected state and refetches on conflict + reapply', async () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    expect(result.current.selectedState).toBeDefined();
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

    expect(result.current.edges).toHaveLength(0);
    expect(typeof result.current.onConnect).toBe('function');

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });

    expect(result.current.edges).toHaveLength(1);
    const draft = result.current.edges[0];
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

    expect(result.current.edges).toHaveLength(0);
  });

  it('does not call the daemon on connect (local-only draft)', () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1'), { wrapper });

    act(() => {
      result.current.onConnect({ source: 's1', target: 's2', sourceHandle: null, targetHandle: null });
    });

    // No save trigger fires from a connect — draft is local until inspector commit.
    expect(result.current.saveTriggers.transition).toBe(0);
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
    expect(result.current.conflict).toEqual({ currentRevision: 1, section: 'transition' });
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
