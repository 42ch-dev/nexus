import { useEffect, useState } from 'react';
import { Navigate } from 'react-router-dom';

import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useDesktopCapabilities } from '@/lib/client-context';
import { useNexusClient } from '@/lib/client-context';
import { DaemonReadySplash } from '@/components/setup/daemon-ready-splash';
import type { ReactNode } from 'react';

interface SetupGateProps {
  children: ReactNode;
}

/**
 * First-launch + per-launch gate.
 *
 * - If setup is not completed: redirect to `/setup`.
 * - If setup is completed: show a brief daemon-ready splash until the first
 *   health probe succeeds, then render the main UI shell.
 *
 * Browser build skips both gates (`setup_completed` defaults to true and the
 * splash resolves instantly).
 */
export function SetupGate({ children }: SetupGateProps) {
  const { completed, isLoading } = useSetupCompleted();
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  const [daemonReady, setDaemonReady] = useState(() => !desktop);
  const [error, setError] = useState<string | null>(null);

  // Skip the splash entirely in browser builds; desktop builds wait for the
  // first successful health probe or a terminal error from the sidecar.
  useEffect(() => {
    if (!desktop) {
      setDaemonReady(true);
      return;
    }
    let cancelled = false;
    let unsub: (() => void) | undefined;

    const cap = desktop;

    async function probe() {
      try {
        await client.health();
        if (!cancelled) setDaemonReady(true);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'Daemon is not responding.');
        }
      }
    }

    async function subscribe() {
      try {
        unsub = await cap.onDaemonStatusChanged((status) => {
          if (cancelled) return;
          if (status.state === 'running') {
            setDaemonReady(true);
            setError(null);
          } else if (status.state === 'error' || status.state === 'stopped') {
            setError(status.detail ?? `Daemon is ${status.state}.`);
          }
        });
      } catch {
        // Event subscription is optional; fall back to a one-time probe.
      }
      void probe();
    }

    void subscribe();
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [desktop, client]);

  if (isLoading) {
    return null;
  }

  if (!completed) {
    return <Navigate to="/setup" replace />;
  }

  if (!daemonReady) {
    return <DaemonReadySplash error={error} onRetry={() => window.location.reload()} />;
  }

  return <>{children}</>;
}
