import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { TransportErrorBlockFixtures } from '@/fixtures/transport-error-block';

describe('TransportErrorBlockFixtures', () => {
  it('renders the fixture root and the full kind matrix', () => {
    render(<TransportErrorBlockFixtures />);

    expect(screen.getByTestId('transport-error-block-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('transport-error-block-full-matrix')).toBeInTheDocument();

    const expectedKinds = [
      'daemon_down',
      'http_fallback',
      'network',
      'tls',
      'timeout',
      'unknown',
    ] as const;
    for (const kind of expectedKinds) {
      expect(screen.getByTestId(`transport-error-block-row-${kind}`)).toBeInTheDocument();
    }
  });

  it('renders one primitive per kind with matching data-kind', () => {
    render(<TransportErrorBlockFixtures />);
    const matrix = screen.getByTestId('transport-error-block-full-matrix');
    const blocks = within(matrix).getAllByTestId('transport-error-block');
    expect(blocks).toHaveLength(6);

    const kinds = blocks.map((b) => b.getAttribute('data-kind'));
    expect(new Set(kinds)).toEqual(
      new Set([
        'daemon_down',
        'http_fallback',
        'network',
        'tls',
        'timeout',
        'unknown',
      ]),
    );
  });

  it('renders both primary and secondary CTAs when callbacks are supplied', () => {
    render(<TransportErrorBlockFixtures />);
    const networkRow = screen.getByTestId('transport-error-block-row-network');
    // network matrix: Open Settings primary + Retry secondary.
    expect(within(networkRow).getByTestId('transport-error-primary')).toHaveAttribute(
      'data-cta',
      'openConnectionSettings',
    );
    expect(within(networkRow).getByTestId('transport-error-secondary')).toHaveAttribute(
      'data-cta',
      'retry',
    );
  });

  it('renders the callback-omission row with no CTAs for daemon_down', () => {
    render(<TransportErrorBlockFixtures />);
    const noCbs = screen.getByTestId('transport-error-block-no-callbacks');
    const blocks = within(noCbs).getAllByTestId('transport-error-block');
    expect(blocks).toHaveLength(2);

    // daemon_down with no callbacks → no CTAs at all.
    const daemon = blocks.find((b) => b.getAttribute('data-kind') === 'daemon_down');
    expect(daemon).toBeDefined();
    expect(daemon && within(daemon).queryByTestId('transport-error-primary')).not.toBeInTheDocument();

    // tls with no callbacks → Use Desktop App primary still renders (informational).
    const tls = blocks.find((b) => b.getAttribute('data-kind') === 'tls');
    expect(tls).toBeDefined();
    expect(tls && within(tls).getByTestId('transport-error-primary')).toHaveAttribute(
      'data-cta',
      'useDesktopApp',
    );
  });

  it('renders the detail-line variant', () => {
    render(<TransportErrorBlockFixtures />);
    expect(
      screen.getByText('Last daemon exit code: 1 (subprocess crashed during boot)'),
    ).toBeInTheDocument();
  });
});
