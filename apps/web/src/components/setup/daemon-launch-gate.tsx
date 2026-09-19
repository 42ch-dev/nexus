import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { DaemonReadySplash } from '@/components/setup/daemon-ready-splash';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { errorMessage as toErrorMessage } from '@/lib/error-message';
import type { TransportErrorKind } from '@/lib/nexus';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';

interface DaemonLaunchGateProps {
  children: ReactNode;
}

const WAIT_TIMEOUT_MS = 25_000;
/** Secondary health poll cadence while waiting for `running` (V1.125). */
const HEALTH_POLL_MS = 1_500;

/**
 * Map a desktop daemon status failure to a transport-error kind (V1.129 P1).
 *
 * The desktop launch gate does NOT consume the browser-client classifier —
 * its failures come from the desktop bridge `getDaemonStatus` / wait timeout. The kind
 * is therefore inferred from the path the gate took, not from a thrown
 * `NexusClientError`. This keeps the recovery UX consistent across surfaces
 * (same primitive, same copy table) without inventing a new error path.
 *
 * - `state: 'error'` → `daemon_down` (the local daemon is not reaching
 *   `running`; recovery copy points the user to start it / reset local DB).
 * - timeout → `timeout` (25s elapsed without `running`).
 * - status IPC unavailable → `unknown` (cannot classify; recovery generic).
 */
function kindForErrorState(errorState: 'daemon-error' | 'timeout' | 'unknown'): TransportErrorKind {
  switch (errorState) {
    case 'daemon-error':
      return 'daemon_down';
    case 'timeout':
      return 'timeout';
    case 'unknown':
      return 'unknown';
  }
}

/**
 * Outer application launch gate (V1.105 P0, V1.125 tightened; v1.192 P0-T8
 * Electron recovery honesty).
 *
 * Desktop: fullscreen splash until daemon status is `running` only. Browser:
 * instant pass. Health probes during wait are attach-race helpers — they do
 * not unlock without a `running` status (V1.125 AC-V1125-1).
 *
 * Happy path never calls `startDaemon` — main's `DesktopServiceController`
 * owns the service lifecycle. Recovery consumes the controller's REAL
 * readiness: a renderer reload never reruns main (the controller survives),
 * so retry reloads purely to re-mount the gate, and a confirmed reset
 * re-enters the wait against the live subscription — never assuming main
 * restarts, and a cancelled/failed reset shows no success reload.
 */
export function DaemonLaunchGate({ children }: DaemonLaunchGateProps) {
  const { t } = useTranslation('setup');
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  const [daemonReady, setDaemonReady] = useState(() => !desktop);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [errorKind, setErrorKind] = useState<TransportErrorKind | null>(null);
  const [resetBusy, setResetBusy] = useState(false);
  // Bumped after a CONFIRMED reset to re-enter the wait against the live
  // controller (re-subscribe + status poll). Never bumped on cancel/failure.
  const [resetNonce, setResetNonce] = useState(0);

  useEffect(() => {
    if (!desktop) {
      setDaemonReady(true);
      return;
    }

    let cancelled = false;
    let unsub: (() => void) | undefined;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    let pollId: ReturnType<typeof setInterval> | undefined;
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
      setErrorMessage(null);
      setErrorKind(null);
      clearWaitTimeout();
    }

    function applyStatus(status: DaemonStatus) {
      if (cancelled) return;
      if (status.state === 'running') {
        markReady();
      } else if (status.state === 'starting' || status.state === 'stopped') {
        // Main's DesktopServiceController owns start — keep waiting; do not
        // call startDaemon from the renderer.
        setDaemonReady(false);
        setErrorMessage(null);
        setErrorKind(null);
      } else if (status.state === 'error') {
        setDaemonReady(false);
        setErrorMessage(status.detail ?? t('error.daemonNotResponding'));
        setErrorKind(kindForErrorState('daemon-error'));
      }
    }

    /**
     * Health success is not sufficient to unlock (V1.125) — re-fetch status so
     * attach races can observe `running`. When status IPC is unavailable, keep
     * waiting; timeout surfaces the error UX.
     */
    async function probeForReady() {
      try {
        await client.health();
        if (cancelled || ready) return;
        try {
          const status = await cap.getDaemonStatus();
          if (!cancelled) applyStatus(status);
        } catch {
          // Status IPC unavailable — health alone must not unlock.
        }
      } catch {
        // Ignore — status events / timeout own failure UX during wait.
      }
    }

    function clearHealthPoll() {
      if (pollId !== undefined) {
        clearInterval(pollId);
        pollId = undefined;
      }
    }

    function startHealthPoll() {
      clearHealthPoll();
      pollId = setInterval(() => {
        void probeForReady();
      }, HEALTH_POLL_MS);
    }

    async function subscribe() {
      try {
        const status = await cap.getDaemonStatus();
        if (cancelled) return;

        if (status.state === 'running') {
          applyStatus(status);
          return;
        }

        applyStatus(status);
        startHealthPoll();

        const listen = await cap.onDaemonStatusChanged((next) => {
          applyStatus(next);
          if (next.state === 'running') {
            clearHealthPoll();
            return;
          }
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
        // Subscription unavailable — poll health and re-check status for `running`.
        startHealthPoll();
        void probeForReady();
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
          if (status.state === 'running') {
            applyStatus(status);
            return;
          }
          if (status.state === 'error') {
            setDaemonReady(false);
            setErrorMessage(status.detail ?? t('error.daemonNotResponding'));
            setErrorKind(kindForErrorState('daemon-error'));
            return;
          }
          setDaemonReady(false);
          setErrorMessage(t('error.daemonSlowStart'));
          setErrorKind(kindForErrorState('timeout'));
        })
        .catch(() => {
          if (cancelled || ready) return;
          setDaemonReady(false);
          setErrorMessage(t('error.daemonStatusUnknown'));
          setErrorKind(kindForErrorState('unknown'));
        });
    }, WAIT_TIMEOUT_MS);

    void subscribe();
    return () => {
      cancelled = true;
      clearWaitTimeout();
      clearHealthPoll();
      unsub?.();
    };
  }, [client, desktop, t, resetNonce]);

  function retry() {
    // Renderer reload only remounts the SPA — main's controller survives and
    // keeps owning the service. The re-mounted gate consumes the controller's
    // real readiness (status subscribe/poll); no startDaemon, no main rerun.
    window.location.reload();
  }

  async function resetLocalDatabase() {
    if (!desktop) return;
    setResetBusy(true);
    setErrorMessage(null);
    setErrorKind(null);
    try {
      await desktop.resetLocalDatabase();
      // Confirmed reset: the main-owned controller drives daemon recovery and
      // emits real status transitions. Re-enter the wait against the live
      // subscription — a renderer reload would not rerun main, so recovery
      // truth comes from the controller, not from navigation.
      setResetBusy(false);
      setResetNonce((n) => n + 1);
    } catch (err) {
      setResetBusy(false);
      // Cancelled/failed reset stays in recovery: surface the error, keep the
      // existing subscription, and never show a success reload.
      setErrorMessage(toErrorMessage(err) || t('error.resetDatabaseFailed'));
      setErrorKind(kindForErrorState('unknown'));
    }
  }

  if (!daemonReady) {
    return (
      <DaemonReadySplash
        errorKind={errorKind}
        errorMessage={errorMessage ?? undefined}
        onRetry={retry}
        onResetLocalDatabase={desktop ? () => void resetLocalDatabase() : undefined}
        resetBusy={resetBusy}
      />
    );
  }

  return <>{children}</>;
}
