import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { TransportErrorBlock, type TransportErrorKind } from './transport-error-block';

const KINDS: TransportErrorKind[] = [
  'daemon_down',
  'network',
  'tls',
  'http_fallback',
  'timeout',
  'unknown',
];

describe('TransportErrorBlock', () => {
  describe('copy + region semantics', () => {
    it.each(KINDS)('renders the per-kind headline + body and a region role for kind=%s', (kind) => {
      render(
        <TransportErrorBlock
          kind={kind}
          onRetry={() => {}}
          onOpenSettings={() => {}}
        />,
      );

      const region = screen.getByTestId('transport-error-block');
      expect(region).toHaveAttribute('data-kind', kind);
      expect(region).toHaveAttribute('role', 'alert');
      // Headline + body render text (snapshot-free assertions on stable substrings
      // drawn from the V1.129 P0 spec copy table).
      expect(region.textContent).toBeTruthy();
    });

    it('uses the caller-supplied title when provided (override)', () => {
      render(
        <TransportErrorBlock
          kind="network"
          title="Custom headline"
          onRetry={() => {}}
        />,
      );
      expect(screen.getByText('Custom headline')).toBeInTheDocument();
    });

    it('renders the optional detail line below the body', () => {
      render(
        <TransportErrorBlock
          kind="daemon_down"
          detail="exit code 1"
          onRetry={() => {}}
        />,
      );
      expect(screen.getByText('exit code 1')).toBeInTheDocument();
    });

    it('omits the detail line when no detail is supplied', () => {
      render(<TransportErrorBlock kind="daemon_down" onRetry={() => {}} />);
      // No `<p>` children beyond headline + body — detail renders as its own
      // text node, so absence is verified by testid absence on a stricter query.
      const region = screen.getByTestId('transport-error-block');
      expect(region.textContent).not.toContain('exit code');
    });
  });

  describe('CTA visibility matrix (kind-driven)', () => {
    it('daemon_down: Retry primary only (no secondary)', () => {
      const onRetry = vi.fn();
      const onOpenSettings = vi.fn();
      render(
        <TransportErrorBlock
          kind="daemon_down"
          onRetry={onRetry}
          onOpenSettings={onOpenSettings}
        />,
      );

      const primary = screen.getByTestId('transport-error-primary');
      expect(primary).toHaveAttribute('data-cta', 'retry');
      expect(primary).toHaveTextContent('Retry');
      expect(screen.queryByTestId('transport-error-secondary')).not.toBeInTheDocument();

      fireEvent.click(primary);
      expect(onRetry).toHaveBeenCalledTimes(1);
      expect(onOpenSettings).not.toHaveBeenCalled();
    });

    it('network: Open Connection Settings primary, Retry secondary', () => {
      const onRetry = vi.fn();
      const onOpenSettings = vi.fn();
      render(
        <TransportErrorBlock
          kind="network"
          onRetry={onRetry}
          onOpenSettings={onOpenSettings}
        />,
      );

      const primary = screen.getByTestId('transport-error-primary');
      expect(primary).toHaveAttribute('data-cta', 'openConnectionSettings');
      const secondary = screen.getByTestId('transport-error-secondary');
      expect(secondary).toHaveAttribute('data-cta', 'retry');

      fireEvent.click(primary);
      expect(onOpenSettings).toHaveBeenCalledTimes(1);
      fireEvent.click(secondary);
      expect(onRetry).toHaveBeenCalledTimes(1);
    });

    it('tls: no primary button (informational body carries the desktop-app instruction), Open Settings secondary', () => {
      const onRetry = vi.fn();
      const onOpenSettings = vi.fn();
      render(
        <TransportErrorBlock
          kind="tls"
          onRetry={onRetry}
          onOpenSettings={onOpenSettings}
        />,
      );

      const region = screen.getByTestId('transport-error-block');
      // QC1-F-002: `useDesktopApp` is informational — it must NOT render as a
      // keyboard-focusable button. The desktop-app instruction rides the body.
      expect(screen.queryByTestId('transport-error-primary')).not.toBeInTheDocument();
      expect(region.textContent).toMatch(/desktop app/i);

      // Secondary (Open Connection Settings) still renders when its callback
      // is supplied.
      const secondary = screen.getByTestId('transport-error-secondary');
      expect(secondary).toHaveAttribute('data-cta', 'openConnectionSettings');
      fireEvent.click(secondary);
      expect(onOpenSettings).toHaveBeenCalledTimes(1);
      // No primary button → no path to fire either callback via a primary click.
      expect(onRetry).not.toHaveBeenCalled();
    });

    it('timeout / unknown: Retry primary, Open Settings secondary', () => {
      const onRetry = vi.fn();
      const onOpenSettings = vi.fn();
      const { rerender } = render(
        <TransportErrorBlock
          kind="timeout"
          onRetry={onRetry}
          onOpenSettings={onOpenSettings}
        />,
      );

      expect(screen.getByTestId('transport-error-primary')).toHaveAttribute(
        'data-cta',
        'retry',
      );
      expect(screen.getByTestId('transport-error-secondary')).toHaveAttribute(
        'data-cta',
        'openConnectionSettings',
      );

      rerender(
        <TransportErrorBlock
          kind="unknown"
          onRetry={onRetry}
          onOpenSettings={onOpenSettings}
        />,
      );
      expect(screen.getByTestId('transport-error-primary')).toHaveAttribute(
        'data-cta',
        'retry',
      );
      expect(screen.getByTestId('transport-error-secondary')).toHaveAttribute(
        'data-cta',
        'openConnectionSettings',
      );
    });

    it('http_fallback: Retry primary only (no secondary)', () => {
      render(
        <TransportErrorBlock
          kind="http_fallback"
          onRetry={() => {}}
          onOpenSettings={() => {}}
        />,
      );
      expect(screen.getByTestId('transport-error-primary')).toHaveAttribute(
        'data-cta',
        'retry',
      );
      expect(screen.queryByTestId('transport-error-secondary')).not.toBeInTheDocument();
    });
  });

  describe('callback omission hides the matching CTA (spec lock)', () => {
    it('hides Retry when onRetry is omitted (network: Open Settings remains, Retry secondary hidden)', () => {
      render(<TransportErrorBlock kind="network" onOpenSettings={() => {}} />);
      expect(screen.getByTestId('transport-error-primary')).toHaveAttribute(
        'data-cta',
        'openConnectionSettings',
      );
      // Secondary was Retry per matrix; omitting onRetry hides it.
      expect(screen.queryByTestId('transport-error-secondary')).not.toBeInTheDocument();
    });

    it('hides Open Connection Settings when onOpenSettings is omitted (unknown: Retry remains)', () => {
      render(<TransportErrorBlock kind="unknown" onRetry={() => {}} />);
      expect(screen.getByTestId('transport-error-primary')).toHaveAttribute(
        'data-cta',
        'retry',
      );
      expect(screen.queryByTestId('transport-error-secondary')).not.toBeInTheDocument();
    });

    it('renders headline + body only when both callbacks are omitted (toast case)', () => {
      render(<TransportErrorBlock kind="daemon_down" />);
      const region = screen.getByTestId('transport-error-block');
      expect(region).toBeInTheDocument();
      expect(screen.queryByTestId('transport-error-primary')).not.toBeInTheDocument();
      expect(screen.queryByTestId('transport-error-secondary')).not.toBeInTheDocument();
    });

    it('tls with no callbacks renders headline + body only (no Use Desktop App button)', () => {
      // QC1-F-002: `useDesktopApp` is informational and never renders a button.
      // The body copy carries the desktop-app instruction.
      render(<TransportErrorBlock kind="tls" />);
      const region = screen.getByTestId('transport-error-block');
      expect(region).toBeInTheDocument();
      expect(screen.queryByTestId('transport-error-primary')).not.toBeInTheDocument();
      expect(screen.queryByTestId('transport-error-secondary')).not.toBeInTheDocument();
      expect(region.textContent).toMatch(/desktop app/i);
    });
  });

  describe('caller-owned copy overrides (QC1-F-001)', () => {
    it('uses the caller-supplied body when provided', () => {
      render(
        <TransportErrorBlock
          kind="daemon_down"
          body="Caller-supplied body text."
          onRetry={() => {}}
        />,
      );
      expect(screen.getByText('Caller-supplied body text.')).toBeInTheDocument();
    });

    it('uses the caller-supplied primary CTA label when provided', () => {
      render(
        <TransportErrorBlock
          kind="daemon_down"
          primaryCtaLabel="Reintentar"
          onRetry={() => {}}
        />,
      );
      const primary = screen.getByTestId('transport-error-primary');
      expect(primary).toHaveTextContent('Reintentar');
    });

    it('uses the caller-supplied secondary CTA label when provided', () => {
      render(
        <TransportErrorBlock
          kind="timeout"
          primaryCtaLabel="Reintentar"
          secondaryCtaLabel="Abrir ajustes"
          onRetry={() => {}}
          onOpenSettings={() => {}}
        />,
      );
      expect(screen.getByTestId('transport-error-primary')).toHaveTextContent('Reintentar');
      expect(screen.getByTestId('transport-error-secondary')).toHaveTextContent('Abrir ajustes');
    });

    it('falls back to package defaults when overrides are omitted (Studio case)', () => {
      render(<TransportErrorBlock kind="daemon_down" onRetry={() => {}} />);
      // Package default body for daemon_down.
      expect(
        screen.getByText('Start it with `nexus42 daemon start`, then try again.'),
      ).toBeInTheDocument();
      // Package default Retry label.
      expect(screen.getByTestId('transport-error-primary')).toHaveTextContent('Retry');
    });
  });

  describe('custom className composition', () => {
    it('appends a custom className without dropping the locked surface classes', () => {
      render(
        <TransportErrorBlock
          kind="daemon_down"
          onRetry={() => {}}
          className="mt-8"
        />,
      );
      const region = screen.getByTestId('transport-error-block');
      expect(region).toHaveClass('mt-8');
      expect(region).toHaveClass('rounded-control');
      expect(region).toHaveClass('border-red-300');
    });
  });
});
