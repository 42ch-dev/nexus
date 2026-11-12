/**
 * useSimulationTrajectory — the hook calls the real simulate route, and a
 * candidate that changes after the run turns the trajectory stale. A refused
 * simulate is its own `failed` state, never `idle`.
 */
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { delay, http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { candidateDocument, servedIrDocument, simulationTrajectory } from '@/test/simulation-fixture';
import { renderInApp } from '@/test/test-providers';

import { useSimulationTrajectory } from './use-simulation-trajectory';

const SIMULATE_PATH = '/v1/daemon/strategies/:strategyId/simulate';
const IR_PATH = '/v1/daemon/strategies/novel-writing/workflow-ir';

function Harness({ entry = 'draft' }: { entry?: string }) {
  const candidate = candidateDocument({ entry });
  const simulation = useSimulationTrajectory(candidate);
  return (
    <div>
      <span data-testid="status">{simulation.status}</span>
      <span data-testid="level">{simulation.trajectory?.simulation_level ?? 'none'}</span>
      <button type="button" onClick={() => void simulation.run(candidate)}>
        Run
      </button>
    </div>
  );
}

/** A document held across renders so a test can mutate it mid-request. */
function MutableHarness({ doc }: { doc: WorkflowDocument }) {
  const simulation = useSimulationTrajectory(doc);
  return (
    <div>
      <span data-testid="status">{simulation.status}</span>
      <button type="button" onClick={() => void simulation.run(doc)}>
        Run
      </button>
    </div>
  );
}

function PresetHarness() {
  const simulation = useSimulationTrajectory(undefined);
  return (
    <div>
      <span data-testid="preset-status">{simulation.status}</span>
      <span data-testid="preset-level">{simulation.trajectory?.simulation_level ?? 'none'}</span>
      <button type="button" onClick={() => void simulation.runStrategy('novel-writing')}>
        Walk preset
      </button>
    </div>
  );
}

describe('useSimulationTrajectory', () => {
  it('re-reads a fetched IR: unchanged content keeps the trajectory, changed content re-walks it', async () => {
    let doc = servedIrDocument({ id: 'novel-writing' });
    let walks = 0;
    const posts = vi.fn(async () => {
      walks += 1;
      // Hold the re-walk in flight so the stale surface is observable.
      if (walks === 2) await delay(100);
      return HttpResponse.json(simulationTrajectory());
    });
    useHandlers(
      http.get('/v1/daemon/strategies/novel-writing/workflow-ir', () => HttpResponse.json(doc)),
      http.post(SIMULATE_PATH, posts),
    );

    renderInApp(<PresetHarness />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Walk preset' }));
    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('ready'));

    // The route serves `revision: 0` and no hash both times; the content is
    // equal, so the trajectory in hand still describes the preset.
    fireEvent.click(screen.getByRole('button', { name: 'Walk preset' }));
    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('ready'));
    expect(posts).toHaveBeenCalledTimes(1);

    // Same revision, different content: only the document can say it moved.
    doc = servedIrDocument({ id: 'novel-writing', entry: 'routing' });
    fireEvent.click(screen.getByRole('button', { name: 'Walk preset' }));

    // The replacement walk reports progress rather than the held snapshot's
    // readiness; the stale banner comes from `isStale`, not from `status`.
    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('running'));
    // Await the second POST before reading the settled total: a bare `ready`
    // wait can resolve before the re-walk has been issued at all.
    await waitFor(() => expect(posts).toHaveBeenCalledTimes(2));
    expect(posts).toHaveBeenCalledTimes(2);
    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('ready'));
  });

  it('posts the candidate and exposes the returned trajectory', async () => {
    let requestBody: unknown = null;
    useHandlers(
      http.post(SIMULATE_PATH, async ({ request }) => {
        requestBody = await request.json();
        return HttpResponse.json(simulationTrajectory());
      }),
    );

    renderInApp(<Harness />, { client: new BrowserClient() });
    expect(screen.getByTestId('status')).toHaveTextContent('idle');

    fireEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'));
    expect(screen.getByTestId('level')).toHaveTextContent('reachability');
    expect(requestBody).toMatchObject({ document: { id: 'user/demo', entry: 'draft' } });
  });

  it('marks the trajectory stale when the candidate content changes after the run', async () => {
    useHandlers(http.post(SIMULATE_PATH, () => HttpResponse.json(simulationTrajectory())));

    const view = renderInApp(<Harness />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'));

    view.rerender(<Harness entry="routing" />);

    expect(screen.getByTestId('status')).toHaveTextContent('stale');
  });

  it('reports a refused simulate as failed, not as a never-simulated candidate', async () => {
    useHandlers(
      http.post(SIMULATE_PATH, () =>
        HttpResponse.json(
          { error: { code: 'invalid_input', message: 'judge has no default edge' } },
          { status: 400 },
        ),
      ),
    );

    renderInApp(<Harness />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('failed'));
    expect(screen.getByTestId('level')).toHaveTextContent('none');
  });

  it('ignores a second run while a walk is in flight', async () => {
    const posts = vi.fn(async () => {
      await delay(50);
      return HttpResponse.json(simulationTrajectory());
    });
    useHandlers(http.post(SIMULATE_PATH, posts));

    renderInApp(<Harness />, { client: new BrowserClient() });
    const run = screen.getByRole('button', { name: 'Run' });
    fireEvent.click(run);
    fireEvent.click(run);

    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'));
    expect(posts).toHaveBeenCalledTimes(1);
  });

  it('keeps the trajectory in hand when the re-read fails, and retries on the next open', async () => {
    const posts = vi.fn(() => HttpResponse.json(simulationTrajectory()));
    let irFails = false;
    let doc = servedIrDocument({ id: 'novel-writing' });
    useHandlers(
      http.get(IR_PATH, () =>
        irFails
          ? HttpResponse.json(
              { error: { code: 'unavailable', message: 'ir read failed' } },
              { status: 503 },
            )
          : HttpResponse.json(doc),
      ),
      http.post(SIMULATE_PATH, posts),
    );

    renderInApp(<PresetHarness />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Walk preset' }));
    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('ready'));

    // The preset moved on and the re-read that would replace the trajectory fails.
    doc = servedIrDocument({ id: 'novel-writing', entry: 'routing' });
    irFails = true;
    fireEvent.click(screen.getByRole('button', { name: 'Walk preset' }));

    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('failed'));
    // The good trajectory survives the failed re-read instead of being wiped.
    expect(screen.getByTestId('preset-level')).toHaveTextContent('reachability');

    // The next open re-reads and succeeds.
    irFails = false;
    fireEvent.click(screen.getByRole('button', { name: 'Walk preset' }));
    await waitFor(() => expect(screen.getByTestId('preset-status')).toHaveTextContent('ready'));
    expect(posts).toHaveBeenCalledTimes(2);
  });

  it('records the identity taken before the request, not the document after it', async () => {
    const doc = candidateDocument();
    useHandlers(
      http.post(SIMULATE_PATH, async () => {
        // A mutation while the walk is in flight is not what the server walked.
        doc.entry = 'routing';
        return HttpResponse.json(simulationTrajectory());
      }),
    );

    renderInApp(<MutableHarness doc={doc} />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('stale'));
  });
});
