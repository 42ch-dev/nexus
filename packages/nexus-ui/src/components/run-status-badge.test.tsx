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
    // succeeded → warning, applied → running, failed → error. v1.183 P0
    // (R-V1121P1QC1-S001): soft variants consume the projected
    // nexus-ui-badge-soft-* tokens, so the semantic variant name — not the
    // raw hue — is the stable class contract.
    expect(needsReview.className).toContain('nexus-ui-badge-soft-warning');
    expect(applied.className).toContain('nexus-ui-badge-soft-running');
    expect(failed.className).toContain('nexus-ui-badge-soft-error');
  });
});
