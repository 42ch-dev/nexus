/**
 * note: Strategy canvas — the Simulate action reachability (v1.237 P2).
 *
 * The disclosure is only useful if a creator can open it from the open
 * strategy. One action must resolve the installed preset as its candidate IR,
 * run the static walk, and open the existing disclosure with the real
 * trajectory; a failed IR read or walk lands on the same failed state.
 */
import { cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import { delay, http, HttpResponse } from 'msw';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import {
  candidateDocument,
  servedIrDocument,
  simulationTrajectory,
} from '@/test/simulation-fixture';
import { renderInApp } from '@/test/test-providers';

vi.mock('@/components/canvas/strategy-canvas/hooks/use-strategy-applied', async () => {
  const { candidateDocument } = await import('@/test/simulation-fixture');
  const document = candidateDocument({ id: 'novel-writing' });
  return {
    useStrategyApplied: () => ({
      data: {
        current_apply: {
          apply_id: 'app_1',
          approval_id: 'apr_42',
          content_hash: 'hash-1',
          applied_at: '2026-11-13T01:00:00Z',
          supersedes_apply_id: null,
        },
        newest_approval: null,
        applied_document: document,
        applied_document_refusal: null,
      },
      isLoading: false,
      isError: false,
      error: null,
      isPending: false,
      refetch: () => Promise.resolve(undefined),
    }),
  };
});
vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: () => null,
  useNodeChangeHandler: () => () => {},
}));

vi.mock('@/components/canvas/strategy-canvas/hooks/use-strategy-canvas', () => ({
  useStrategyCanvas: () => ({
    parsed: undefined,
    revision: 1,
    activeSession: null,
    creatorId: 'c1',
    activeScheduleId: undefined,
    form: { label: '', description: '', nextTarget: '', promptBody: '' },
    setForm: () => {},
    activeSection: 'state',
    setActiveSection: () => {},
  }),
}));

vi.mock('@/components/canvas/strategy-canvas/inspectors/state-inspector', () => ({
  StateInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspectors/edge-inspector', () => ({
  EdgeInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspectors/prompt-inspector', () => ({
  PromptInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspector-panel', () => ({
  InspectorPanel: () => null,
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
const SIMULATE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/simulate`;

function renderCanvas() {
  renderInApp(<StrategyCanvas presetId={STRATEGY_ID} />, { client: new BrowserClient() });
}

afterEach(() => cleanup());

describe('StrategyCanvas — Simulate action', () => {
  it('reads the strategy IR, walks it, and opens the disclosure with the trajectory', async () => {
    const requests: { ir: number; simulateBody: unknown } = { ir: 0, simulateBody: null };
    const document = servedIrDocument({ id: STRATEGY_ID });
    useHandlers(
      http.get(IR_PATH, () => {
        requests.ir += 1;
        return HttpResponse.json(document);
      }),
      http.post(SIMULATE_PATH, async ({ request }) => {
        requests.simulateBody = await request.json();
        return HttpResponse.json(simulationTrajectory());
      }),
    );

    renderCanvas();
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));

    await waitFor(() => expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument());
    expect(requests.ir).toBe(1);
    expect(requests.simulateBody).toMatchObject({ document: { id: STRATEGY_ID } });
    expect(screen.getByTestId('approve-simulation-level')).toHaveTextContent(
      'Reachability only; judge branches not explored',
    );
    expect(screen.getByTestId('approve-stubbed-judges')).toBeInTheDocument();
    expect(screen.getByTestId('approve-effect-summary')).toBeInTheDocument();
    expect(screen.getByTestId('approve-predicted-waits')).toBeInTheDocument();
    expect(screen.getByTestId('approve-no-output-preview')).toBeInTheDocument();
  });

  it('uses the existing failed state when the IR read is refused', async () => {
    useHandlers(
      http.get(IR_PATH, () =>
        HttpResponse.json(
          { error: { code: 'not_found', message: 'Preset is not installed' } },
          { status: 404 },
        ),
      ),
    );

    renderCanvas();
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));

    await waitFor(() => expect(screen.getByTestId('approve-simulate-failed')).toBeInTheDocument());
    expect(screen.queryByTestId('approve-simulation-level')).not.toBeInTheDocument();
  });

  it('re-opens a fresh preset trajectory without walking it again', async () => {
    const irDocument = servedIrDocument({ id: STRATEGY_ID });
    const simulate = vi.fn(() => HttpResponse.json(simulationTrajectory()));
    let irReads = 0;
    useHandlers(
      http.get(IR_PATH, () => {
        irReads += 1;
        return HttpResponse.json(irDocument);
      }),
      http.post(SIMULATE_PATH, simulate),
    );

    renderCanvas();
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(irReads).toBe(2));

    expect(simulate).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument();
    expect(screen.queryByTestId('approve-stale')).not.toBeInTheDocument();
  });

  it('re-walks an installed preset whose IR moved on between opens', async () => {
    let irDocument = servedIrDocument({ id: STRATEGY_ID });
    const walkedEntries: string[] = [];
    useHandlers(
      http.get(IR_PATH, () => HttpResponse.json(irDocument)),
      http.post(SIMULATE_PATH, async ({ request }) => {
        const body = (await request.json()) as { document: { entry: string } };
        walkedEntries.push(body.document.entry);
        // why: Hold the re-walk in flight so the stale surface is observable.
        if (walkedEntries.length === 2) await delay(100);
        return HttpResponse.json(simulationTrajectory());
      }),
    );

    renderCanvas();
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    // note: The route keeps `revision: 0`; only the document content moves.
    irDocument = servedIrDocument({ id: STRATEGY_ID, entry: 'routing' });
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));

    // note: While its replacement is walked the surface reports progress; the stale
    // banner stays visible from the held snapshot's own staleness.
    await waitFor(() => expect(screen.getByTestId('approve-stale')).toBeInTheDocument());
    expect(screen.getByText('Running the static simulation…')).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.queryByTestId('approve-stale')).not.toBeInTheDocument(),
    );
    expect(walkedEntries).toEqual(['draft', 'routing']);
  });

  it('re-opens a fresh candidate trajectory without walking it again', async () => {
    const simulate = vi.fn(() => HttpResponse.json(simulationTrajectory()));
    useHandlers(http.post(SIMULATE_PATH, simulate));

    renderInApp(
      <StrategyCanvas
        presetId={STRATEGY_ID}
        candidate={candidateDocument({ id: STRATEGY_ID, revision: 1 })}
      />,
      { client: new BrowserClient() },
    );
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument());

    expect(simulate).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('approve-stale')).not.toBeInTheDocument();
  });

  it('re-runs a stale trajectory instead of offering a dead control', async () => {
    const simulate = vi.fn(() => HttpResponse.json(simulationTrajectory()));
    useHandlers(http.post(SIMULATE_PATH, simulate));

    const { rerender } = renderInApp(
      <StrategyCanvas
        presetId={STRATEGY_ID}
        candidate={candidateDocument({ id: STRATEGY_ID, revision: 1 })}
      />,
      { client: new BrowserClient() },
    );
    fireEvent.click(screen.getByRole('button', { name: 'Simulate…' }));
    await waitFor(() => expect(simulate).toHaveBeenCalledTimes(1));

    rerender(
      <StrategyCanvas
        presetId={STRATEGY_ID}
        candidate={candidateDocument({ id: STRATEGY_ID, entry: 'routing' })}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('approve-stale')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Run simulation' }));
    await waitFor(() => expect(simulate).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByTestId('approve-stale')).not.toBeInTheDocument());
  });
});
