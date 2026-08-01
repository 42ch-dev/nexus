import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it } from 'vitest';

import { RunStatusBadge, type RunStatus } from './run-status-badge';

const STATUSES: RunStatus[] = ['running', 'succeeded', 'failed', 'applied', 'discarded'];

describe('RunStatusBadge', () => {
  it('renders every lifecycle status with the caller-owned label', () => {
    render(
      <>
        {STATUSES.map((status) => (
          <RunStatusBadge key={status} status={status} label={`Label ${status}`} />
        ))}
      </>,
    );

    const badges = screen.getAllByTestId('run-status-badge');
    expect(badges).toHaveLength(STATUSES.length);
    expect(new Set(badges.map((b) => b.getAttribute('data-status')))).toEqual(
      new Set(STATUSES),
    );
    for (const status of STATUSES) {
      expect(screen.getByText(`Label ${status}`)).toBeInTheDocument();
    }
  });

  it('maps statuses to distinct semantic badge variants', () => {
    render(
      <>
        <RunStatusBadge status="succeeded" label="Needs review" />
        <RunStatusBadge status="applied" label="Applied" />
        <RunStatusBadge status="failed" label="Failed" />
      </>,
    );

    const [needsReview, applied, failed] = screen.getAllByTestId('run-status-badge');
    // succeeded → warning (amber family), applied → running (green), failed → error (red).
    expect(needsReview.className).toContain('amber');
    expect(applied.className).toContain('green');
    expect(failed.className).toContain('red');
  });
});
