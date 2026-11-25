/**
 *
 * Regression for the stale-inspector selection bug, exercised through the
 * production path: `StrategyCanvas` gates the inspector body on the
 * orchestrator's `selectedState` and renders `surface.inspector` — the memo
 * whose `renderInspector(node)` previously ignored its argument and read
 * `ctxRef.current`, which the orchestrator assigns only after
 * `useCanvasSurface` ran, so it carried the previous render's selection.
 * Broken behaviour: first selection renders an empty editing body
 * (`ctx.selectedState` undefined) and a re-selection keeps showing the
 * PREVIOUS state's sections (the memoised node and the adapter's data
 * disagree). CanvasShell is the only surface mock — the selection change is
 * driven through the same `onNodesChange` prop the shell receives, shaped
 * like a real React Flow click (the target selects, the others deselect).
 */
import { act, cleanup, fireEvent, screen } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { NodeChangeset } from '@xyflow/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { BrowserClient } from '@/lib/nexus';
import { COMPOSE_FLAG_STORAGE_KEY } from '@/lib/canvas/strategy-compose-flag';
import { useHandlers } from '@/test/msw-server';
import { servedIrDocument } from '@/test/simulation-fixture';
import { renderInApp } from '@/test/test-providers';

const shellProps = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));

vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: (props: { children?: React.ReactNode } & Record<string, unknown>) => {
    shellProps.current = props;
    return <>{props.children}</>;
  },
  useNodeChangeHandler: () => () => {},
}));

vi.mock('@/components/canvas/idea-input', () => ({
  IdeaInput: () => null,
}));

const STRATEGY_ID = 'novel-writing';
const APPLIED_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/applied`;
const SESSIONS_PATH = '/v1/daemon/orchestration/sessions';
const SCHEDULES_PATH = '/v1/daemon/orchestration/schedules';

const DOCUMENT = servedIrDocument({ id: STRATEGY_ID });

function setupCanvas() {
  window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
  useHandlers(
    http.get(SESSIONS_PATH, () => HttpResponse.json({ items: [] })),
    http.get(SCHEDULES_PATH, () => HttpResponse.json({ items: [] })),
    http.get(APPLIED_PATH, () =>
      HttpResponse.json({
        current_apply: {
          apply_id: 'app_1',
          approval_id: 'apr_42',
          content_hash: 'hash-1',
          applied_at: '2026-11-13T01:00:00Z',
          supersedes_apply_id: null,
        },
        newest_approval: null,
        applied_document: DOCUMENT,
        applied_document_refusal: null,
      }),
    ),
  );
  return renderInApp(<StrategyCanvas presetId={STRATEGY_ID} />, { client: new BrowserClient() });
}

/**
 * A React Flow single click selects the target and deselects everything
 * else in one changeset; replicate that through the exact prop the shell owns.
 */
function clickSelectNode(id: string) {
  const nodes = shellProps.current.nodes as Array<{ id: string }>;
  act(() => {
    (shellProps.current.onNodesChange as (changes: NodeChangeset) => void)(
      new NodeChangeset(
        nodes.map((node) => ({
          id: node.id,
          type: 'select' as const,
          selected: node.id === id,
        })),
      ),
    );
  });
}

describe('strategy inspector — selection opens the body through the production gate', () => {
  afterEach(() => {
    cleanup();
    window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
  });

  it('selection -> edit -> section body renders the state fields', async () => {
    setupCanvas();
    await screen.findByText('rev 0');

    expect(screen.queryByTestId('state-structural-disabled')).not.toBeInTheDocument();

    clickSelectNode('draft');
    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    expect(screen.getByTestId('state-structural-disabled')).toBeInTheDocument();
    expect(screen.getByLabelText('State inspector')).toBeInTheDocument();
    // 'draft' has exactly one outgoing linear edge, so the transition section shows.
    expect(screen.getByTestId('transition-structural-disabled')).toBeInTheDocument();
  });

  it('re-selection re-renders the new node even though the memoised node and the adapter ctx disagree', async () => {
    setupCanvas();
    await screen.findByText('rev 0');

    clickSelectNode('draft');
    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(screen.getByTestId('transition-structural-disabled')).toBeInTheDocument();

    // On this render the inspector memo recomputes with node='done' while
    // ctxRef.current still carries the previous render's selection ('draft').
    // The broken implementation read the ref and kept showing 'draft'.
    clickSelectNode('done');

    expect(screen.getByTestId('state-structural-disabled')).toBeInTheDocument();
    // 'done' is terminal with no outgoing edges: the transition section must
    // disappear. Under the stale-ctx bug it persists (from 'draft').
    expect(screen.queryByTestId('transition-structural-disabled')).not.toBeInTheDocument();
  });
});
