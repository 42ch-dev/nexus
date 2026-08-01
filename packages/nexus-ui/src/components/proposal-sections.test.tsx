import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';

import {
  ProposalSections,
  type ComputeProposals,
  type ProposalSectionsCopy,
} from './proposal-sections';

const COPY: ProposalSectionsCopy = {
  reportTitle: 'Report',
  knowledgeUpdatesTitle: 'Knowledge updates',
  timelineEventsTitle: 'Timeline events',
  newKnowledgeTitle: 'New knowledge',
  truncatedNote: 'This preview is shortened — open the Run for the full result.',
  untitledEventLabel: 'Untitled event',
  affectedEntriesLabel: (count) => `Affects ${count} entries`,
  newEntryLabel: 'New entry',
};

const FULL_PROPOSALS: ComputeProposals = {
  schema_version: 1,
  state_delta: [
    { op: 'sub', path: 'character.current_hp', target_key_block_id: 'char-brann', value: { amount: 6 } },
  ],
  timeline_events: [
    {
      title: 'Aria strikes Brann',
      summary: 'Brann takes 6 damage.',
      affected_key_block_ids: ['char-aria', 'char-brann'],
    },
  ],
  new_key_blocks: [{ title: 'Bruised rib', kind: 'injury' }],
  battle_report: {
    kind: 'combat',
    damage: 6,
    defender_hp_before: 20,
    defender_hp_after: 14,
  },
};

describe('ProposalSections', () => {
  it('renders all four sections when the envelope is fully populated', () => {
    render(<ProposalSections proposals={FULL_PROPOSALS} copy={COPY} />);

    expect(screen.getByTestId('proposal-section-report')).toBeInTheDocument();
    expect(screen.getByTestId('proposal-section-knowledge-updates')).toBeInTheDocument();
    expect(screen.getByTestId('proposal-section-timeline-events')).toBeInTheDocument();
    expect(screen.getByTestId('proposal-section-new-knowledge')).toBeInTheDocument();

    // Report: kind badge + key/value rows (kind key itself not repeated).
    expect(screen.getByTestId('proposal-report-kind')).toHaveTextContent('combat');
    expect(screen.getByText('damage')).toBeInTheDocument();
    expect(screen.getByText('6')).toBeInTheDocument();

    // Knowledge updates: op badge + target + path.
    const delta = screen.getByTestId('proposal-delta-0');
    expect(delta).toHaveTextContent('sub');
    expect(delta).toHaveTextContent('char-brann');
    expect(delta).toHaveTextContent('character.current_hp');

    // Timeline event: title, summary, affected count via caller formatter.
    const event = screen.getByTestId('proposal-event-evt_0');
    expect(event).toHaveTextContent('Aria strikes Brann');
    expect(event).toHaveTextContent('Brann takes 6 damage.');
    expect(event).toHaveTextContent('Affects 2 entries');

    // New knowledge: title picked from the entry.
    expect(screen.getByTestId('proposal-new-entry-0')).toHaveTextContent('Bruised rib');
  });

  it('hides empty sections', () => {
    render(
      <ProposalSections
        proposals={{
          state_delta: [],
          timeline_events: [],
          new_key_blocks: [],
          battle_report: {},
        }}
        copy={COPY}
      />,
    );

    expect(screen.queryByTestId('proposal-section-report')).not.toBeInTheDocument();
    expect(screen.queryByTestId('proposal-section-knowledge-updates')).not.toBeInTheDocument();
    expect(screen.queryByTestId('proposal-section-timeline-events')).not.toBeInTheDocument();
    expect(screen.queryByTestId('proposal-section-new-knowledge')).not.toBeInTheDocument();
  });

  it('renders the truncated note only when flagged', () => {
    const { rerender } = render(
      <ProposalSections proposals={FULL_PROPOSALS} copy={COPY} />,
    );
    expect(screen.queryByTestId('proposal-truncated-note')).not.toBeInTheDocument();

    rerender(<ProposalSections proposals={FULL_PROPOSALS} truncated copy={COPY} />);
    expect(screen.getByTestId('proposal-truncated-note')).toHaveTextContent(
      'This preview is shortened',
    );
  });

  it('supports optional per-event selection with evt_<index> ids', () => {
    const onToggleEvent = vi.fn();
    render(
      <ProposalSections
        proposals={FULL_PROPOSALS}
        copy={COPY}
        selectedEventIds={['evt_0']}
        onToggleEvent={onToggleEvent}
      />,
    );

    const toggle = screen.getByTestId('proposal-event-toggle-evt_0');
    expect(toggle).toBeChecked();
    fireEvent.click(toggle);
    expect(onToggleEvent).toHaveBeenCalledWith('evt_0');
  });

  it('falls back to caller copy for untitled events and unnamed entries', () => {
    render(
      <ProposalSections
        proposals={{
          state_delta: [],
          timeline_events: [{ summary: 'No title here.' }],
          new_key_blocks: [{ kind: 'misc' }],
          battle_report: {},
        }}
        copy={COPY}
      />,
    );

    expect(screen.getByText('Untitled event')).toBeInTheDocument();
    expect(screen.getByTestId('proposal-new-entry-0')).toHaveTextContent('New entry');
  });
});
