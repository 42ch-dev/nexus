/**
 * useSimulationTrajectory — the hook calls the real simulate route, and a
 * candidate that changes after the run turns the trajectory stale. A refused
 * simulate is its own `failed` state, never `idle`.
 */
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { candidateDocument, simulationTrajectory } from '@/test/simulation-fixture';
import { renderInApp } from '@/test/test-providers';

import { useSimulationTrajectory } from './use-simulation-trajectory';

const SIMULATE_PATH = '/v1/daemon/strategies/:strategyId/simulate';

function Harness({ revision }: { revision: number }) {
  const candidate = candidateDocument({ revision, content_hash: `hash-${revision}` });
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

describe('useSimulationTrajectory', () => {
  it('posts the candidate and exposes the returned trajectory', async () => {
    let requestBody: unknown = null;
    useHandlers(
      http.post(SIMULATE_PATH, async ({ request }) => {
        requestBody = await request.json();
        return HttpResponse.json(simulationTrajectory());
      }),
    );

    renderInApp(<Harness revision={1} />, { client: new BrowserClient() });
    expect(screen.getByTestId('status')).toHaveTextContent('idle');

    fireEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'));
    expect(screen.getByTestId('level')).toHaveTextContent('reachability');
    expect(requestBody).toMatchObject({ document: { id: 'user/demo', revision: 1 } });
  });

  it('marks the trajectory stale when the candidate revision changes after the run', async () => {
    useHandlers(http.post(SIMULATE_PATH, () => HttpResponse.json(simulationTrajectory())));

    const view = renderInApp(<Harness revision={1} />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'));

    view.rerender(<Harness revision={2} />);

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

    renderInApp(<Harness revision={1} />, { client: new BrowserClient() });
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('failed'));
    expect(screen.getByTestId('level')).toHaveTextContent('none');
  });
});
