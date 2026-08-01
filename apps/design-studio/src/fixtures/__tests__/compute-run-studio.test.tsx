import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ComputeRunStudioFixtures } from '@/fixtures/compute-run-studio';

describe('ComputeRunStudioFixtures', () => {
  it('renders the full variant matrix', () => {
    render(<ComputeRunStudioFixtures />);

    expect(screen.getByTestId('compute-run-studio-fixtures')).toBeInTheDocument();
    const variants = [
      'run-studio-variant-form-basic-combat',
      'run-studio-variant-form-kitchen-sink',
      'run-studio-variant-form-empty',
      'run-studio-variant-picker-empty',
      'run-studio-variant-inspector-success',
      'run-studio-variant-inspector-truncated',
      'run-studio-variant-inspector-failed',
      'run-studio-variant-runs-populated',
      'run-studio-variant-runs-empty',
    ] as const;
    for (const testId of variants) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
  });

  it('derives two entity pickers for the basic-combat manifest with fixture characters', () => {
    render(<ComputeRunStudioFixtures />);
    const form = screen.getByTestId('run-studio-form-basic-combat');

    const selects = within(form).getAllByTestId('entity-picker-select');
    expect(selects).toHaveLength(2);
    expect(within(form).getByText('Attacker')).toBeInTheDocument();
    expect(within(form).getByText('Defender')).toBeInTheDocument();
    for (const select of selects) {
      expect(select).toHaveTextContent('Aria');
      expect(select).toHaveTextContent('Brann');
    }
  });

  it('renders the kitchen-sink control derivation and the unsupported note', () => {
    render(<ComputeRunStudioFixtures />);
    const form = screen.getByTestId('run-studio-form-kitchen-sink');

    expect(within(form).getByTestId('run-form-field-mode').tagName).toBe('SELECT');
    expect(within(form).getByTestId('run-form-field-rounds')).toHaveAttribute('type', 'number');
    expect(within(form).getByTestId('run-form-field-allow_items')).toHaveAttribute(
      'type',
      'checkbox',
    );
    expect(within(form).getByTestId('run-form-field-note')).toBeInTheDocument();
    expect(within(form).getByTestId('run-form-field-tags-unsupported')).toBeInTheDocument();
  });

  it('renders the missing-schema and picker empty states with spec §6 copy', () => {
    render(<ComputeRunStudioFixtures />);

    expect(screen.getByTestId('run-studio-variant-form-empty')).toHaveTextContent(
      'Can’t run this module',
    );
    expect(screen.getByTestId('run-studio-variant-picker-empty')).toHaveTextContent(
      'No characters to run',
    );
  });

  it('renders the succeeded inspector with all four proposal sections and CTAs', () => {
    render(<ComputeRunStudioFixtures />);
    const inspector = screen.getByTestId('run-studio-inspector-success');

    expect(within(inspector).getByTestId('proposal-section-report')).toBeInTheDocument();
    expect(
      within(inspector).getByTestId('proposal-section-knowledge-updates'),
    ).toBeInTheDocument();
    expect(
      within(inspector).getByTestId('proposal-section-timeline-events'),
    ).toBeInTheDocument();
    expect(within(inspector).getByTestId('proposal-section-new-knowledge')).toBeInTheDocument();
    expect(within(inspector).getByTestId('run-studio-accept')).toHaveTextContent('Accept');
    expect(within(inspector).getByTestId('run-studio-discard')).toHaveTextContent('Discard');
  });

  it('renders the truncated note and the failed-run presentation', () => {
    render(<ComputeRunStudioFixtures />);

    expect(
      within(screen.getByTestId('run-studio-variant-inspector-truncated')).getByTestId(
        'proposal-truncated-note',
      ),
    ).toHaveTextContent('shortened');

    const failed = screen.getByTestId('run-studio-inspector-failed');
    expect(failed).toHaveTextContent('Run failed');
    expect(failed).toHaveTextContent('World unchanged');
    expect(within(failed).getByTestId('run-status-badge')).toHaveAttribute(
      'data-status',
      'failed',
    );
  });

  it('renders Runs rows for every lifecycle status, newest first, plus empty state', () => {
    render(<ComputeRunStudioFixtures />);
    const populated = screen.getByTestId('run-studio-variant-runs-populated');

    const badges = within(populated).getAllByTestId('run-status-badge');
    expect(badges.map((b) => b.getAttribute('data-status'))).toEqual([
      'running',
      'succeeded',
      'applied',
      'discarded',
      'failed',
    ]);
    expect(badges.map((b) => b.textContent)).toEqual([
      'Running',
      'Needs review',
      'Applied',
      'Discarded',
      'Failed',
    ]);
    expect(within(populated).getByTestId('run-studio-runs-filters')).toBeInTheDocument();

    expect(screen.getByTestId('runs-table-empty')).toHaveTextContent('No runs yet');
  });
});
