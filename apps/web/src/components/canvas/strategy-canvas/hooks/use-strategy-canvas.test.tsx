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
import type { NexusClient } from '@/lib/nexus';
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
        {children}
      </ClientProvider>
    </QueryClientProvider>
  );
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
