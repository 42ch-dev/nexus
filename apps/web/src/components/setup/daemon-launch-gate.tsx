import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';

import { DaemonReadySplash } from '@/components/setup/daemon-ready-splash';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';

interface DaemonLaunchGateProps {
  children: ReactNode;
}

const WAIT_TIMEOUT_MS = 25_000;

/**
 * Outer application launch gate (V1.105 P0).
 *
 * Desktop: fullscreen splash until the daemon is Ready (health or status
 * running/degraded). Browser: instant pass.
 *
 * Happy path never calls `startDaemon` — Tauri `.setup()` always starts the
 * sidecar (D2). Recovery: reload (retry) or `resetLocalDatabase` then reload
 * (no post-reset `startDaemon`; reload re-runs always-start).
 */
export function DaemonLaunchGate({ children }: DaemonLaunchGateProps) {
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  const [daemonReady, setDaemonReady] = useState(() => !desktop);
  const [error, setError] = useState<string | null>(null);
  const [retryToken, setRetryToken] = useState(0);
  const [resetBusy, setResetBusy] = useState(false);

  useEffect(() => {
    if (!desktop) {
      setDaemonReady(true);
      return;
    }

    let cancelled = false;
    let unsub: (() => void) | undefined;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    const cap = desktop;

    function clearWaitTimeout() {
      if (timeoutId !== undefined) {
        clearTimeout(timeoutId);
        timeoutId = undefined;
      }
    }

    function markReady() {
      if (cancelled) return;
      setDaemonReady(true);
      setError(null);
      clearWaitTimeout();
    }

    function applyStatus(status: DaemonStatus) {
      if (cancelled) return;
      if (status.state === 'running' || status.state === 'degraded') {
        markReady();
      } else if (status.state === 'starting' || status.state === 'stopped') {
        // D2: Tauri `.setup()` owns start — keep waiting; do not call startDaemon.
        setDaemonReady(false);
        setError(null);
      } else if (status.state === 'error') {
        setDaemonReady(false);
        setError(status.detail ?? `Daemon is ${status.state}.`);
      }
    }

    /** Health success → Ready. Failure is non-terminal while still waiting. */
    async function probeForReady() {
      try {
        await client.health();
        markReady();
      } catch {
        // Ignore — status events / timeout own failure UX during wait.
      }
    }

    async function subscribe() {
      try {
        const status = await cap.getDaemonStatus();
        if (cancelled) return;

        if (status.state === 'running' || status.state === 'degraded') {
          applyStatus(status);
          return;
        }

        applyStatus(status);

        const listen = await cap.onDaemonStatusChanged((next) => {
          applyStatus(next);
          if (next.state === 'running' || next.state === 'degraded') return;
          // Opportunistic health check after non-ready events (attach races).
          void probeForReady();
        });
        if (cancelled) {
          listen();
          return;
        }
        unsub = listen;

        void probeForReady();
      } catch {
        if (cancelled) return;
        // Subscription unavailable — fall back to health-only wait.
        try {
          await client.health();
          markReady();
        } catch (err) {
          if (!cancelled) {
            setDaemonReady(false);
            setError(errorMessage(err) || 'Daemon is not responding.');
          }
        }
      }
    }

    timeoutId = setTimeout(() => {
      if (cancelled) return;
      void cap
        .getDaemonStatus()
        .then((status) => {
          if (cancelled) return;
          if (status.state === 'running' || status.state === 'degraded') {
            applyStatus(status);
            return;
          }
          if (status.state === 'error') {
            setDaemonReady(false);
            setError(status.detail ?? `Daemon is ${status.state}.`);
            clearWaitTimeout();
            return;
          }
          setDaemonReady(false);
          setError(
            'Daemon is taking longer than expected to start. You can retry or reset the local database.',
          );
          clearWaitTimeout();
        })
        .catch(() => {
          if (cancelled) return;
          setDaemonReady(false);
          setError('Could not determine daemon status. Try retrying.');
          clearWaitTimeout();
        });
    }, WAIT_TIMEOUT_MS);

    void subscribe();
    return () => {
      cancelled = true;
      clearWaitTimeout();
      unsub?.();
    };
  }, [client, desktop, retryToken]);

  function retry() {
    // Reload re-enters Tauri `.setup()` always-start (D2). No startDaemon.
    window.location.reload();
  }

  async function resetLocalDatabase() {
    if (!desktop) return;
    setResetBusy(true);
    setError(null);
    try {
      await desktop.resetLocalDatabase();
      // Explicit D2 decision: do NOT call startDaemon after reset — reload
      // re-runs `.setup()` which always starts/attaches the sidecar.
      window.location.reload();
    } catch (err) {
      setResetBusy(false);
      setError(errorMessage(err) || 'Failed to reset local database.');
      setRetryToken((n) => n + 1);
    }
  }

  if (!daemonReady) {
    return (
      <DaemonReadySplash
        error={error}
        onRetry={retry}
        onResetLocalDatabase={desktop ? () => void resetLocalDatabase() : undefined}
        resetBusy={resetBusy}
      />
    );
  }

  return <>{children}</>;
}
