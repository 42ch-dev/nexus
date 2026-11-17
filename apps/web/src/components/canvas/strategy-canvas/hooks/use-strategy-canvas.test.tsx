/**
 * Regression coverage for R-V171P0-QC1-008 (B7): edit-save-refetch loop.
 *
 * `useStrategyCanvas` does not own the PATCH mutation itself, but it owns the
 * conflict/reapply coordination: when an inspector save fails with a conflict,
 * the canvas refetches the canonical preset before showing the reconcile modal,
 * and reapply refetches again before re-issuing the save trigger.
 */
import { describe, expect, it, vi } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { useStrategyCanvas, type StrategyDocumentSource } from './use-strategy-canvas';

const mocks = vi.hoisted(() => {
  const refetch = vi.fn(() => Promise.resolve({ data: undefined }));
  return { refetch };
});

vi.mock('@/lib/canvas/use-strategy-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-strategy-data')>();
  return {
    ...actual,
    useActiveSession: () => undefined as unknown as ReturnType<typeof actual.useActiveSession>,
    usePresetSchedules: () => ({ data: [] }) as unknown as ReturnType<typeof actual.usePresetSchedules>,
    useDerivedCreatorId: () => 'creator-1',
  };
});

/** The document source the Canvas hands the hook: two states at revision 1. */
function documentSource(): StrategyDocumentSource {
  return {
    document: {
      ir_version: 0,
      id: 'preset-1',
      version: 1,
      revision: 1,
      state: 'candidate',
      meta: {
        description: 'Preset under test',
        kind: 'creator',
        run_intents: ['work_init'],
        requires_capabilities: [],
      },
      entry: 's1',
      terminal: [],
      nodes: [
        { id: 's1', kind: 'outer_state', effect_class: 'pure', ui: { description: 'Original' } },
        { id: 's2', kind: 'outer_state', effect_class: 'pure', ui: { description: 'Second' } },
      ],
      edges: [{ id: 's1->s2', from: 's1', to: 's2', kind: 'linear' }],
      nests: [],
      assets: [],
      signals: [],
      roles: [],
      effect_summary: 'pure',
      provenance: { source: 'compose' },
    },
    refetch: mocks.refetch,
  };
}

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('useStrategyCanvas edit-save-refetch (R-V171P0-QC1-008 B7)', () => {
  it('refetches on conflict + reapply and replays the save trigger', async () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1', documentSource()), { wrapper });

    act(() => {
      // Use the document's own revision so the auto-clear effect does not
      // immediately dismiss the conflict before reapply is exercised.
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
