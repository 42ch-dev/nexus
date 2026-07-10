import type { ReactNode } from 'react';

import { Badge } from '@/components/ui/badge';

type HealthState =
  | { kind: 'unknown' }
  | { kind: 'connected'; version: string }
  | { kind: 'offline'; message: string };

export interface DaemonHealthIndicatorChromeProps {
  state: HealthState;
  /** True when the active client is pointed at a remote endpoint. */
  isRemote?: boolean;
  /**
   * Optional href for the link wrapper. When omitted, the indicator renders as
   * a plain badge without a hit target.
   */
  href?: string;
  /** Optional link renderer for app routers (e.g. react-router NavLink). */
  renderLink?: (props: { href: string; children: ReactNode }) => ReactNode;
  'data-testid'?: string;
}

/**
 * Presentational daemon health indicator — header badge only.
 *
 * No polling, no daemon client, no routing. The host wrapper owns the health
 * fetch and the link target implementation.
 */
export function DaemonHealthIndicatorChrome({
  state,
  isRemote = false,
  href,
  renderLink,
  'data-testid': dataTestId,
}: DaemonHealthIndicatorChromeProps) {
  const badge =
    state.kind === 'unknown' ? (
      <Badge variant="neutral" data-testid={dataTestId}>Checking daemon…</Badge>
    ) : state.kind === 'connected' ? (
      <Badge
        variant="running"
        title={`Daemon v${state.version}`}
        data-testid={dataTestId}
      >
        {isRemote ? 'Remote daemon' : 'Daemon'} v{state.version}
      </Badge>
    ) : (
      <Badge
        variant="error"
        title={state.message}
        data-testid={dataTestId}
      >
        {isRemote ? 'Remote daemon offline' : 'Daemon offline'}
      </Badge>
    );

  if (!href) return badge;

  if (renderLink) {
    return renderLink({ href, children: badge });
  }

  return (
    <a
      href={href}
      className="focus-visible:outline-none"
      aria-label={state.kind === 'offline' ? 'Daemon offline — go to connection settings' : 'Daemon health'}
    >
      {badge}
    </a>
  );
}
