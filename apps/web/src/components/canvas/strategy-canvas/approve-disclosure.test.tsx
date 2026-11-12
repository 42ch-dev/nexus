/**
 * Approve disclosure — the five required limit elements, each asserted on its
 * own (`.mstar/research/p2-simulator-limits.md` §3; compass D5).
 *
 * A bare "simulated" badge satisfies none of these: every assertion reads a
 * trajectory value (level wording, the stubbed-judge count and ids, the effect
 * fence reading, the predicted-wait label) or the no-output-preview statement.
 */
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { simulationTrajectory } from '@/test/simulation-fixture';

import { ApproveDisclosure } from './approve-disclosure';

function renderDisclosure(
  overrides: Partial<React.ComponentProps<typeof ApproveDisclosure>> = {},
) {
  const props: React.ComponentProps<typeof ApproveDisclosure> = {
    open: true,
    onOpenChange: vi.fn(),
    status: 'ready',
    trajectory: simulationTrajectory(),
    error: null,
    onApprove: vi.fn(),
    ...overrides,
  };
  renderInApp(<ApproveDisclosure {...props} />);
  return props;
}

const approveButton = () => screen.getByRole('button', { name: 'Approve' });

describe('ApproveDisclosure — five disclosure elements', () => {
  it('1. renders simulation_level in words', () => {
    renderDisclosure();

    expect(screen.getByTestId('approve-simulation-level')).toHaveTextContent(
      'Reachability only; judge branches not explored',
    );
  });

  it('2. renders the stubbed_judges count with an expandable id list', () => {
    renderDisclosure({
      trajectory: simulationTrajectory({
        stubbed_judges: ['routing', 'quality_gate'],
      }),
    });

    expect(screen.getByTestId('approve-stubbed-judges')).toHaveTextContent(
      '2 judge branches not explored',
    );
    const details = screen
      .getByTestId('approve-stubbed-judges')
      .querySelector('details') as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(details).toHaveTextContent('routing');
    expect(details).toHaveTextContent('quality_gate');

    fireEvent.click(screen.getByText('Show stubbed judge ids'));
    expect(details.open).toBe(true);
  });

  it('3. labels effect_summary as a worst-case fence, not a forecast', () => {
    renderDisclosure({
      trajectory: simulationTrajectory({ effect_summary: 'external' }),
    });

    const summary = screen.getByTestId('approve-effect-summary');
    expect(summary).toHaveTextContent('external');
    expect(summary).toHaveTextContent(/worst-case effect fence/i);
    expect(summary).toHaveTextContent(/a fence, not a forecast/i);
  });

  it('4. labels predicted waits "predicted (static)"', () => {
    renderDisclosure();

    const waits = screen.getByTestId('approve-predicted-waits');
    expect(waits).toHaveTextContent('Predicted (static) waits: 1');
    expect(waits).toHaveTextContent('pause');
  });

  it('5. states that a passed simulation does not preview output', () => {
    renderDisclosure();

    expect(screen.getByTestId('approve-no-output-preview')).toHaveTextContent(
      'A passed simulation does not preview output',
    );
  });

  it('treats an unknown simulation_level as unsupported instead of assuming reachability', () => {
    renderDisclosure({
      trajectory: simulationTrajectory({ simulation_level: 'branch_explored' }),
    });

    expect(screen.getByTestId('approve-simulation-level')).toHaveTextContent(
      'branch_explored',
    );
    expect(screen.getByTestId('approve-simulation-level')).toHaveTextContent(
      /does not know this level/i,
    );
  });
});

describe('ApproveDisclosure — approval readiness', () => {
  it('enables Approve only for a fresh trajectory', () => {
    const props = renderDisclosure();

    expect(approveButton()).toBeEnabled();
    fireEvent.click(approveButton());
    expect(props.onApprove).toHaveBeenCalledTimes(1);
  });

  it('shows the trajectory as stale and disables Approve after a revision change', () => {
    renderDisclosure({ status: 'stale', isStale: true });

    expect(screen.getByTestId('approve-stale')).toBeInTheDocument();
    expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument();
    expect(approveButton()).toBeDisabled();
  });

  it('keeps a failed simulation distinct from "not simulated" and not approval-ready', () => {
    renderDisclosure({ status: 'failed', trajectory: null, error: new Error('judge collision') });

    expect(screen.getByTestId('approve-simulate-failed')).toHaveTextContent('judge collision');
    expect(screen.queryByTestId('approve-not-simulated')).not.toBeInTheDocument();
    expect(approveButton()).toBeDisabled();
  });

  it('reports the not-simulated candidate and disables Approve', () => {
    renderDisclosure({ status: 'idle', trajectory: null });

    expect(screen.getByTestId('approve-not-simulated')).toBeInTheDocument();
    expect(screen.queryByTestId('approve-simulate-failed')).not.toBeInTheDocument();
    expect(approveButton()).toBeDisabled();
  });

  it('offers a re-run for a stale trajectory', () => {
    const onRun = vi.fn();
    renderDisclosure({ status: 'stale', onRun });

    fireEvent.click(screen.getByRole('button', { name: 'Run simulation' }));
    expect(onRun).toHaveBeenCalledTimes(1);
  });

  it('disables Approve, and names why, when no approval write path is wired', () => {
    renderDisclosure({ onApprove: undefined });

    expect(approveButton()).toBeDisabled();
    expect(screen.getByTestId('approve-unavailable')).toHaveTextContent('I-000415');
    // The disclosure itself still works: the limit is visible without approval.
    expect(screen.getByTestId('approve-simulation-level')).toBeInTheDocument();
  });

  it('names a non-Error, non-string throw instead of swallowing it', () => {
    renderDisclosure({ status: 'failed', trajectory: null, error: { code: 'walk_refused' } });

    expect(screen.getByTestId('approve-simulate-failed')).toHaveTextContent('walk_refused');
  });
});
