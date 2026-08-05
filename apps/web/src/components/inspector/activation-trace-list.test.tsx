/**
 * ActivationTraceList — P1 T1 (DF-76) coverage.
 *
 * Fixture `MomentInspectResponse`-shaped trace entries covering: a fired entry
 * with a slot, a missed entry (no slot), and a hopped entry (forward-compatible
 * hop fields — not emitted by the current wire, spec §2 H4), plus the empty
 * trace case.
 */
import { screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ActivationTraceList, type ActivationTraceEntryWithHop } from '@/components/inspector/activation-trace-list';
import { renderInApp } from '@/test/test-providers';

const TRACE: ActivationTraceEntryWithHop[] = [
  {
    entry_id: 'king-entry',
    canonical_name: "King's court",
    reason: 'primary-any (literal): matched key [king]',
    accepted: true,
  },
  {
    entry_id: 'bandit-entry',
    canonical_name: 'Bandit gang',
    reason: 'no matching keys',
    accepted: false,
  },
  {
    entry_id: 'queen-hop',
    canonical_name: "Queen's retinue",
    reason: 'relation hop from queen-entry',
    accepted: true,
    hop_depth: 1,
    hop_origin_entry_id: 'queen-entry',
  },
];

const SLOT_BY_ENTRY = new Map([
  ['king-entry', 'world.before'],
  ['queen-hop', 'default'],
]);

describe('ActivationTraceList — T1', () => {
  it('renders a fired entry with a Fired badge, its slot, and the reason', () => {
    renderInApp(<ActivationTraceList trace={TRACE} slotByEntry={SLOT_BY_ENTRY} />);

    const row = screen.getByTestId('trace-entry-king-entry');
    expect(within(row).getByText(/Fired/)).toBeInTheDocument();
    expect(within(row).getByText("King's court")).toBeInTheDocument();
    expect(within(row).getByTestId('trace-slot-king-entry')).toHaveTextContent('world.before');
    expect(within(row).getByTestId('trace-reason-king-entry')).toHaveTextContent(
      'primary-any (literal): matched key [king]',
    );
  });

  it('renders a missed entry with a Missed badge, no slot, and the reason', () => {
    renderInApp(<ActivationTraceList trace={TRACE} slotByEntry={SLOT_BY_ENTRY} />);

    const row = screen.getByTestId('trace-entry-bandit-entry');
    expect(within(row).getByText(/Missed/)).toBeInTheDocument();
    expect(within(row).getByText('Bandit gang')).toBeInTheDocument();
    // Missed entries carry no slot (post stage-gate routing only covers accepted entries).
    expect(within(row).queryByTestId('trace-slot-bandit-entry')).toBeNull();
    expect(within(row).getByTestId('trace-reason-bandit-entry')).toHaveTextContent('no matching keys');
  });

  it('renders hop depth/origin for a hopped entry when the entry carries hop fields', () => {
    renderInApp(<ActivationTraceList trace={TRACE} slotByEntry={SLOT_BY_ENTRY} />);

    const row = screen.getByTestId('trace-entry-queen-hop');
    expect(within(row).getByText(/Fired/)).toBeInTheDocument();
    expect(within(row).getByTestId('trace-slot-queen-hop')).toHaveTextContent('default');
    const hop = within(row).getByTestId('trace-hop-queen-hop');
    expect(hop).toHaveTextContent('Hop depth: 1');
    expect(hop).toHaveTextContent('Hop origin: queen-entry');
  });

  it('does not render hop metadata for entries without hop fields', () => {
    renderInApp(<ActivationTraceList trace={TRACE} slotByEntry={SLOT_BY_ENTRY} />);

    const row = screen.getByTestId('trace-entry-king-entry');
    expect(within(row).queryByTestId('trace-hop-king-entry')).toBeNull();
  });

  it('renders the empty copy when the trace has no entries', () => {
    renderInApp(<ActivationTraceList trace={[]} slotByEntry={new Map()} />);

    expect(screen.getByTestId('trace-empty')).toHaveTextContent('No activation trace entries');
  });
});
