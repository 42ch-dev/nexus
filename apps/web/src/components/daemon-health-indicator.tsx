import { useEffect, useRef, useState } from 'react';
import { NavLink } from 'react-router';

import { DaemonHealthIndicatorChrome } from '@/components/layout/presentational/daemon-health-indicator-chrome';
import { useConnectionConfig, useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { useTranslation } from 'react-i18next';

/**
 * Daemon health indicator — polls `GET /v1/daemon/runtime/health` and shows
 * connection state in the shell header. In V1.92 P1 it also reflects whether
 * the active client is pointed at a remote endpoint, and links to the setup
 * screen when offline or in remote mode.
 */
type HealthState =
  | { kind: 'unknown' }
  | { kind: 'connected'; version: string }
  | { kind: 'offline'; message: string };

const POLL_MS = 10_000;

export function DaemonHealthIndicator() {
  const { t } = useTranslation('shell');
  const client = useNexusClient();
  const config = useConnectionConfig();
  const [state, setState] = useState<HealthState>({ kind: 'unknown' });
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const check = async () => {
      try {
        const health = await client.health();
        if (mounted.current) setState({ kind: 'connected', version: health.version });
      } catch (error) {
        if (!mounted.current) return;
        const message =
          error instanceof NexusClientError ? error.message : t('health.cannotReachLocalDaemon');
        setState({ kind: 'offline', message });
      } finally {
        if (mounted.current) timer = setTimeout(check, POLL_MS);
      }
    };

    void check();
    return () => {
      mounted.current = false;
      if (timer) clearTimeout(timer);
    };
  }, [client]);

  const isRemote = Boolean(config?.active && config.endpointUrl);

  return (
    <DaemonHealthIndicatorChrome
      state={state}
      isRemote={isRemote}
      href="/settings/advanced#connection"
      renderLink={({ href, children }) => (
        <NavLink to={href} className="focus-visible:outline-none">
          {children}
        </NavLink>
      )}
    />
  );
}
