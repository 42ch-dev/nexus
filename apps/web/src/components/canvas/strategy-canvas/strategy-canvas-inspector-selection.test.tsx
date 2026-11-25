/**
 *
 * Regression for the stale-inspector selection bug: the orchestrator assigns
 * `ctxRef.current` AFTER `useCanvasSurface` runs, so on the selection-change
 * render the adapter context still holds the previous render's selection
 * (`ctx.selectedNode`/`ctx.selectedState` disagree with the node the hook's
 * memo passes). The inspector must follow the memo-keyed node argument, so
 * the harness below mirrors the orchestrator's assignment order exactly and
 * drives selection through `onNodesChange`. Broken behaviour: first selection
 * renders an empty body (`ctx.selectedState` undefined) and a re-selection
 * keeps showing the PREVIOUS state's sections (e.g. a transition target
 * input) until the selection identity stops changing.
 */
import { useMemo, useRef } from 'react';
import { act, render, screen } from '@testing-library/react';
import { NodeChangeset, type Node } from '@xyflow/react';
import { describe, expect, it } from 'vitest';

import type { UseCanvasSurfaceResult } from '../use-canvas-surface';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '../use-canvas-surface';
import type { StrategyNodeData, StrategyEdgeData } from '@/lib/canvas/strategy-graph';
import { parsedPresetFromIr } from '@/lib/canvas/ir-preset';
import { candidateDocument } from '@/test/simulation-fixture';

import {
  createStrategyCanvasAdapter,
  type StrategyCanvasAdapterContext,
  type StrategySurfaceGraph,
} from './strategy-canvas-adapter';

type Surface = UseCanvasSurfaceResult<StrategyNodeData, StrategyEdgeData>;

/** Mirrors `StrategyCanvas`: the ctxRef write happens after the surface hook. */
function SurfaceHarness({
  exposed,
}: {
  exposed: { current: Surface | null };
}) {
  const ctxRef = useRef<StrategyCanvasAdapterContext>({
    form: { label: '', description: '', nextTarget: '', promptBody: '' },
    onChange: () => {},
    setActiveSection: () => {},
    selectedState: undefined,
    promptTemplateRef: undefined,
    selectedNode: null,
    parsed: undefined,
    danglingTargets: [],
  });

  const adapter = useMemo(() => createStrategyCanvasAdapter(ctxRef), [ctxRef]);

  const query = useMemo<CanvasSurfaceQueryResult<StrategySurfaceGraph>>(
    () => ({
      data: {
        revision: 1,
        activeSession: null,
        trajectory: null,
        document: candidateDocument(),
      },
      isLoading: false,
      isError: false,
      error: null,
      refetch: () => {},
    }),
    [],
  );

  const surface = useCanvasSurface(adapter, query);

  const selectedNode = surface.selectedNode as Node<StrategyNodeData> | null;
  const stateId = selectedNode?.data.stateId;
  const parsed = parsedPresetFromIr(candidateDocument());
  const selectedState = stateId
    ? parsed.manifest.states.find((s) => s.id === stateId)
    : undefined;

  // Same timing as the orchestrator: assigned after useCanvasSurface ran, so
  // the inspector memo already computed with the previous render's ctx.
  ctxRef.current = {
    form: { label: '', description: '', nextTarget: '', promptBody: '' },
    onChange: () => {},
    setActiveSection: () => {},
    selectedState,
    promptTemplateRef: undefined,
    selectedNode,
    parsed,
    danglingTargets: ctxRef.current.danglingTargets,
  };

  exposed.current = surface;

  return <div data-testid="inspector-body">{surface.inspector}</div>;
}

function selectNode(exposed: { current: Surface | null }, id: string) {
  const surface = exposed.current!;
  const changes = surface.nodes.map((n) => ({
    id: n.id,
    type: 'select' as const,
    selected: n.id === id,
  }));
  act(() => {
    surface.onNodesChange(new NodeChangeset(changes));
  });
}

describe('strategy inspector — selection renders in the same pass', () => {
  it('opens the inspector body with the state fields for a fresh selection', () => {
    const exposed = { current: null as Surface | null };
    const { unmount } = render(<SurfaceHarness exposed={exposed} />);

    expect(screen.queryByTestId('state-structural-disabled')).not.toBeInTheDocument();

    selectNode(exposed, 'draft');

    expect(screen.getByTestId('state-structural-disabled')).toBeInTheDocument();
    expect(screen.getByLabelText(/label/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/description/i)).toBeInTheDocument();
    // 'draft' has exactly one outgoing linear edge → the transition section shows.
    expect(screen.getByTestId('transition-structural-disabled')).toBeInTheDocument();

    unmount();
  });

  it('re-selection re-renders the new node even though the memoised node and the adapter ctx disagree', () => {
    const exposed = { current: null as Surface | null };
    const { unmount } = render(<SurfaceHarness exposed={exposed} />);

    selectNode(exposed, 'draft');
    expect(screen.getByTestId('transition-structural-disabled')).toBeInTheDocument();

    // On this render the inspector memo recomputes with node='done' while
    // ctxRef.current still carries the previous render's selection ('draft').
    // The broken implementation read the ref and kept showing 'draft'.
    selectNode(exposed, 'done');

    expect(screen.getByTestId('state-structural-disabled')).toBeInTheDocument();
    // 'done' is terminal with no outgoing edges: the transition section must
    // disappear. Under the stale-ctx bug it persists (from 'draft').
    expect(screen.queryByTestId('transition-structural-disabled')).not.toBeInTheDocument();

    unmount();
  });
});
