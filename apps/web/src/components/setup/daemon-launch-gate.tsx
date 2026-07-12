import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

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
  const { t } = useTranslation('setup');
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  const [daemonReady, setDaemonReady] = useState(() => !desktop);
  const [error, setError] = useState<string | null>(null);
  const [resetBusy, setResetBusy] = useState(false);

  useEffect(() => {
    if (!desktop) {
      setDaemonReady(true);
      return;
    }

    let cancelled = false;
    let unsub: (() => void) | undefined;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    let ready = false; // Guard: timeout callback must not race after markReady.
    const cap = desktop;

    function clearWaitTimeout() {
      if (timeoutId !== undefined) {
        clearTimeout(timeoutId);
        timeoutId = undefined;
      }
    }

    function markReady() {
      if (cancelled) return;
      ready = true;
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
        setError(status.detail ?? t('error.daemonNotResponding'));
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
            setError(errorMessage(err) || t('error.daemonNotResponding'));
          }
        }
      }
    }

    timeoutId = setTimeout(() => {
      if (cancelled || ready) return;
      // Timer already fired — clear the handle without clearTimeout (no-op).
      timeoutId = undefined;
      void cap
        .getDaemonStatus()
        .then((status) => {
          if (cancelled || ready) return;
          if (status.state === 'running' || status.state === 'degraded') {
            applyStatus(status);
            return;
          }
          if (status.state === 'error') {
            setDaemonReady(false);
            setError(status.detail ?? t('error.daemonNotResponding'));
            return;
          }
          setDaemonReady(false);
          setError(
            t('error.daemonSlowStart'),
          );
        })
        .catch(() => {
          if (cancelled || ready) return;
          setDaemonReady(false);
          setError(t('error.daemonStatusUnknown'));
        });
    }, WAIT_TIMEOUT_MS);

    void subscribe();
    return () => {
      cancelled = true;
      clearWaitTimeout();
      unsub?.();
    };
  }, [client, desktop, t]);

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
      // Keep the error until Restart (reload). Do not re-subscribe here —
      // that would re-run applyStatus and clear the reset-failure message.
      setError(errorMessage(err) || t('error.resetDatabaseFailed'));
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
