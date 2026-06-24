import { useEffect, useRef, useState } from 'react';

import { Badge } from '@/components/ui/badge';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';

/**
 * Daemon health indicator — polls `GET /v1/local/runtime/health` and shows
 * connection state in the shell header. Demonstrates the `tauri-api` adapter
 * boundary end-to-end (BrowserClient → Vite dev proxy → daemon loopback).
 */
type HealthState =
  | { kind: 'unknown' }
  | { kind: 'connected'; version: string }
  | { kind: 'offline'; message: string };

const POLL_MS = 10_000;

export function DaemonHealthIndicator() {
  const client = useNexusClient();
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

  if (state.kind === 'unknown') {
    return <Badge variant="neutral">Checking daemon…</Badge>;
  }
  if (state.kind === 'connected') {
    return (
      <Badge variant="running" title={`Daemon v${state.version}`}>
        Daemon v{state.version}
      </Badge>
    );
  }
  return (
    <Badge variant="error" title={state.message}>
      Daemon offline
    </Badge>
  );
}
