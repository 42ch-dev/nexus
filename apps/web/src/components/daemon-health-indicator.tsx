import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';

import { Badge } from '@/components/ui/badge';
import { useConnectionConfig, useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';

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
          error instanceof NexusClientError ? error.message : 'Cannot reach local daemon';
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

  const badge =
    state.kind === 'unknown' ? (
      <Badge variant="neutral">Checking daemon…</Badge>
    ) : state.kind === 'connected' ? (
      <Badge variant="running" title={`Daemon v${state.version}`}>
        {isRemote ? 'Remote daemon' : 'Daemon'} v{state.version}
      </Badge>
    ) : (
      <Badge variant="error" title={state.message}>
        {isRemote ? 'Remote daemon offline' : 'Daemon offline'}
      </Badge>
    );

  return (
    <Link to="/settings/connection" className="focus-visible:outline-none">
      {badge}
    </Link>
  );
}
