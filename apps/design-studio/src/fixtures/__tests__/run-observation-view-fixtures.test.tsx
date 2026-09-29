import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { RunObservationViewFixtures } from '@/fixtures/run-observation-view-fixtures';

/**
 * Studio fixture for the v1.201 P1 session run view: every state the surface
 * can render is present, and the disclosure/affordance contract of each one is
 * asserted rather than assumed. Theme coverage is the Surfaces pair view over
 * these single theme-following specimens, so the presence of a second
 * light/dark specimen tree would be drift, not coverage.
 */
const VARIANTS = [
  'run-view-variant-running',
  'run-view-variant-gap',
  'run-view-variant-replay',
  'run-view-variant-history-unavailable',
  'run-view-variant-refusal',
  'run-view-variant-transport-error',
  'run-view-variant-terminal',
  'run-view-variant-loading',
] as const;

describe('RunObservationViewFixtures', () => {
  it('renders every run-view variant the surface can reach', () => {
    render(<RunObservationViewFixtures />);

    expect(screen.getByTestId('run-observation-view-fixtures')).toBeInTheDocument();
    for (const testId of VARIANTS) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
      // Each variant states its own phase and observed-run identity.
      expect(screen.getByTestId(`${testId}-phase`)).toBeInTheDocument();
      expect(screen.getByTestId(`${testId}-identity`)).toHaveTextContent('Session run-1a2b3c4d');
    }
  });

  it('follows the document theme instead of hard-coding a light/dark specimen tree', () => {
    render(<RunObservationViewFixtures />);

    for (const testId of VARIANTS) {
      expect(screen.queryByTestId(`${testId}-light`)).not.toBeInTheDocument();
      expect(screen.queryByTestId(`${testId}-dark`)).not.toBeInTheDocument();
    }
  });

  it('shows the running live tail under its own heading', () => {
    render(<RunObservationViewFixtures />);
    const running = screen.getByTestId('run-view-variant-running');

    expect(within(running).getByTestId('run-view-variant-running-phase')).toHaveTextContent('Live');
    expect(within(running).getByTestId('run-view-running-live')).toHaveTextContent('Live');
    expect(within(running).getAllByRole('listitem')).toHaveLength(2);
    // Observation only: the live view offers no reconnect.
    expect(within(running).queryByRole('button', { name: 'Reconnect' })).toBeNull();
  });

  it('keeps the gap marker inline between the frames around it, with the reconnect affordance', () => {
    render(<RunObservationViewFixtures />);
    const gap = screen.getByTestId('run-view-variant-gap');

    expect(within(gap).getByTestId('run-view-variant-gap-phase')).toHaveTextContent('Gap');
    const marker = within(gap).getByTestId('run-view-gap-marker');
    expect(marker).toHaveTextContent('Events were missed');
    expect(marker).toHaveTextContent('Reconnect resumes from the last event received.');
    // Row order is the point: the marker sits where continuity was lost.
    const rows = within(gap).getAllByRole('listitem');
    expect(rows).toHaveLength(3);
    expect(rows[1]).toHaveTextContent('Events were missed');
    expect(within(gap).getByRole('button', { name: 'Reconnect' })).toBeInTheDocument();
  });

  it('separates the replayed span from the live tail and discloses the inferred boundary', () => {
    render(<RunObservationViewFixtures />);
    const replay = screen.getByTestId('run-view-variant-replay');

    const replayed = within(replay).getByTestId('run-view-replay-replayed');
    const live = within(replay).getByTestId('run-view-replay-live');
    expect(replayed).toHaveTextContent('Replayed history');
    expect(live).toHaveTextContent('Live');
    expect(within(replayed).getByTestId('run-view-boundary-note')).toHaveTextContent(
      'split by attach order',
    );
    // The tail heading is not inside the replayed section, and vice versa.
    expect(within(replayed).queryByText('Live')).toBeNull();
  });

  it('renders the lost-history notice above the frames the view had already retained', () => {
    render(<RunObservationViewFixtures />);
    const gone = screen.getByTestId('run-view-variant-history-unavailable');

    expect(within(gone).getByTestId('run-view-variant-history-unavailable-phase')).toHaveTextContent(
      'History unavailable',
    );
    expect(within(gone).getByTestId('run-view-history-unavailable-notice')).toHaveTextContent(
      'Server history unavailable',
    );
    // The retained frames stay on screen — the loss is disclosed, not hidden.
    expect(within(gone).getByTestId('run-view-history-retained')).toHaveTextContent('e1:42');
  });

  it('renders a typed refusal without a retry and an exhausted transport with the reconnect', () => {
    render(<RunObservationViewFixtures />);

    const refusal = screen.getByTestId('run-view-variant-refusal');
    expect(within(refusal).getByRole('alert')).toHaveTextContent('not a root run');
    // A refusal is deterministic: no retry, no reconnect.
    expect(within(refusal).queryByRole('button', { name: 'Try again' })).toBeNull();
    expect(within(refusal).queryByRole('button', { name: 'Reconnect' })).toBeNull();

    const transport = screen.getByTestId('run-view-variant-transport-error');
    expect(within(transport).getByRole('alert')).toHaveTextContent('socket closed');
    // Production shows exactly one reconnect for a transport failure: the error
    // block's own affordance. The header reconnect is gap-only.
    expect(within(transport).getAllByRole('button', { name: 'Reconnect' })).toHaveLength(1);
    expect(within(transport).queryByRole('button', { name: 'Try again' })).toBeNull();
  });

  it('renders the ended run with the frames retained and no reopen affordance', () => {
    render(<RunObservationViewFixtures />);
    const ended = screen.getByTestId('run-view-variant-terminal');

    expect(within(ended).getByTestId('run-view-variant-terminal-phase')).toHaveTextContent('Ended');
    expect(within(ended).getByTestId('run-view-terminal-live')).toHaveTextContent('Completed');
    // The stream is closed: neither the (gap-only) header reconnect nor a
    // transport retry is offered on a run that already ended.
    expect(within(ended).queryByRole('button', { name: 'Reconnect' })).toBeNull();
    expect(within(ended).queryByRole('button', { name: 'Try again' })).toBeNull();
  });

  it('shows the pre-frame stream state for a run with no events yet', () => {
    render(<RunObservationViewFixtures />);
    const loading = screen.getByTestId('run-view-variant-loading');

    expect(within(loading).getByTestId('run-view-variant-loading-phase')).toHaveTextContent(
      'Connecting',
    );
    expect(loading).toHaveTextContent('Connecting to the run stream…');
    expect(within(loading).queryAllByRole('listitem')).toHaveLength(0);
  });
});
