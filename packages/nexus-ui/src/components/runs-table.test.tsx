import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';

import { RunsTable, type RunsTableCopy, type RunTableRow } from './runs-table';

const COPY: RunsTableCopy = {
  moduleColumn: 'Module',
  worldColumn: 'World',
  statusColumn: 'Status',
  startedColumn: 'Started',
  finishedColumn: 'Finished',
  runIdColumn: 'Run ID',
  openRunLabel: 'Open',
  copyIdLabel: 'Copy',
  emptyTitle: 'No runs yet',
  emptyDescription: 'Run this module to see history here.',
};

const ROWS: RunTableRow[] = [
  {
    runId: 'run_0001',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'The Lost City',
    status: 'succeeded',
    statusLabel: 'Needs review',
    startedAt: '2026-07-31 10:02',
    finishedAt: '2026-07-31 10:02',
  },
  {
    runId: 'run_0002',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'The Lost City',
    status: 'applied',
    statusLabel: 'Applied',
    startedAt: '2026-07-31 09:41',
    finishedAt: '2026-07-31 09:41',
  },
  {
    runId: 'run_0003',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'Echo Protocol',
    status: 'failed',
    statusLabel: 'Failed',
    startedAt: '2026-07-30 22:10',
  },
];

describe('RunsTable', () => {
  it('renders rows newest-first with module, world, status, and timestamps', () => {
    render(<RunsTable rows={ROWS} copy={COPY} />);

    const table = screen.getByTestId('runs-table');
    expect(table).toHaveTextContent('Basic Combat');
    expect(table).toHaveTextContent('v1.0.0');
    expect(table).toHaveTextContent('The Lost City');
    expect(table).toHaveTextContent('Echo Protocol');

    const badges = screen.getAllByTestId('run-status-badge');
    expect(badges.map((b) => b.getAttribute('data-status'))).toEqual([
      'succeeded',
      'applied',
      'failed',
    ]);
    expect(badges.map((b) => b.textContent)).toEqual(['Needs review', 'Applied', 'Failed']);

    // Missing finished timestamp renders an em dash.
    expect(screen.getByTestId('runs-table-row-run_0003')).toHaveTextContent('—');
  });

  it('renders correlation ids in monospace with a copy affordance', () => {
    render(<RunsTable rows={ROWS} copy={COPY} />);

    const idCell = screen.getByText('run_0001');
    expect(idCell.className).toContain('text-copy-13-mono');
    expect(screen.getByTestId('runs-table-copy-run_0001')).toHaveAttribute(
      'aria-label',
      'Copy',
    );
  });

  it('reports row opens through onOpenRun', () => {
    const onOpenRun = vi.fn();
    render(<RunsTable rows={ROWS} copy={COPY} onOpenRun={onOpenRun} />);

    fireEvent.click(screen.getByTestId('runs-table-open-run_0002'));
    expect(onOpenRun).toHaveBeenCalledWith('run_0002');
  });

  it('renders the caller-owned empty state when there are no rows', () => {
    render(<RunsTable rows={[]} copy={COPY} />);

    expect(screen.getByTestId('runs-table-empty')).toBeInTheDocument();
    expect(screen.getByText('No runs yet')).toBeInTheDocument();
    expect(screen.getByText('Run this module to see history here.')).toBeInTheDocument();
    expect(screen.queryByTestId('runs-table')).not.toBeInTheDocument();
  });
});
