/**
 * note: `useStrategyCanvas` presents the rendered IR document to the flat edit form
 * and owns the active inspector section.
 */
import { describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { useStrategyCanvas } from './use-strategy-canvas';

vi.mock('@/lib/canvas/use-strategy-data', () => ({
  useActiveSession: () => undefined,
  usePresetSchedules: () => ({ data: [] }),
  useDerivedCreatorId: () => 'creator-1',
}));

/** note: the document the Canvas hands the hook — two states at revision 1. */
const DOCUMENT: WorkflowDocument = {
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
};

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('useStrategyCanvas', () => {
  it('presents the rendered document to the editing surface', () => {
    const { result } = renderHook(() => useStrategyCanvas('preset-1', DOCUMENT), { wrapper });

    expect(result.current.revision).toBe(1);
    expect(result.current.parsed?.manifest.states.map((state) => state.id)).toEqual(['s1', 's2']);
    expect(result.current.activeSection).toBe('state');
  });
});
