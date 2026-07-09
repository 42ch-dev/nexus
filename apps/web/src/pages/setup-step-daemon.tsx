import { useEffect, useState } from 'react';
import { ChevronLeft, Loader2, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useNexusClient, useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import { cn } from '@/lib/utils';
interface SetupStepDaemonProps {
  onNext: () => void;
  onBack: () => void;
}

export function SetupStepDaemon({ onNext, onBack }: SetupStepDaemonProps) {
  const client = useNexusClient();
  const desktop = useDesktopCapabilities();
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retryToken, setRetryToken] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;

    function clearWaitTimeout() {
      if (timeoutId !== undefined) {
        clearTimeout(timeoutId);
        timeoutId = undefined;
      }
    }

    function applyStatus(status: DaemonStatus) {
      if (cancelled) return;
      if (status.state === 'running' || status.state === 'degraded') {
        setReady(true);
        setError(null);
        clearWaitTimeout();
      } else if (status.state === 'starting') {
        setReady(false);
        setError(null);
      } else if (status.state === 'error') {
        setReady(false);
        setError(status.detail ?? `Daemon is ${status.state}.`);
        clearWaitTimeout();
      }
      // 'stopped' is handled inline in subscribe() — do not surface as error here
      // because the clean-state path auto-starts the daemon.
    }

    async function probe() {
      try {
        await client.health();
        if (!cancelled) {
          setReady(true);
          setError(null);
          clearWaitTimeout();
        }
      } catch (err) {
        if (!cancelled) {
          setReady(false);
          setError(errorMessage(err) || 'Could not reach the daemon.');
          clearWaitTimeout();
        }
      }
    }

    async function subscribe() {
      if (!desktop) {
        await probe();
        return;
      }
      try {
        const status = await desktop.getDaemonStatus();
        if (cancelled) return;

        // Fast path: already up — show running immediately; no subscription needed.
        if (status.state === 'running' || status.state === 'degraded') {
          setReady(true);
          setError(null);
          clearWaitTimeout();
          return;
        }

        if (status.state === 'starting') {
          setReady(false);
          setError(null);
        }

        // Clean-state path (stopped) or existing-install crash recovery
        // (error): auto-start the daemon before subscribing.  Don't surface
        // an 'error' detail optimistically — the auto-start may recover, and
        // if it doesn't, the catch block below surfaces the actionable error.
        if (status.state === 'stopped' || status.state === 'error') {
          try {
            await desktop.startDaemon();
          } catch (err) {
            // Surface the sidecar-launch failure immediately — the status
            // subscription won't help when the daemon process isn't running,
            // and the 25s timeout message is too generic.
            if (!cancelled) {
              setError(
                `Could not start the local service: ${errorMessage(err) || 'unknown error'}. ` +
                  'Retry or reset the local database.',
              );
              clearWaitTimeout();
            }
            return;
          }
          if (cancelled) return;

          // Re-probe once after start so Back re-entry / fast boot does not
          // wait solely on the next event emission.
          const afterStart = await desktop.getDaemonStatus();
          if (cancelled) return;
          if (afterStart.state === 'running' || afterStart.state === 'degraded') {
            applyStatus(afterStart);
            return;
          }
          applyStatus(afterStart);
        }

        // At most one active subscription for this mount (cleanup unsubscribes).
        const listen = await desktop.onDaemonStatusChanged((next) => {
          applyStatus(next);
        });
        // B1: if cleanup ran while await was in flight, unlisten immediately —
        // otherwise the listener leaks across remount/retry.
        if (cancelled) {
          listen();
          return;
        }
        unsub = listen;
      } catch {
        if (cancelled) return;
        await probe();
      }
    }

    timeoutId = setTimeout(() => {
      if (cancelled) return;
      if (!desktop) {
        setReady(false);
        setError('Daemon is taking longer than expected to start.');
        return;
      }
      void desktop
        .getDaemonStatus()
        .then((status) => {
          if (cancelled) return;
          if (status.state === 'running' || status.state === 'degraded') {
            applyStatus(status);
            return;
          }
          if (status.state === 'error') {
            applyStatus(status);
            return;
          }
          // B2: starting/stopped (or any other non-ready state) after the
          // bounded wait must surface Retry — never leave a permanent spinner.
          setReady(false);
          setError(
            'Daemon is taking longer than expected to start. You can retry or reset the local database.',
          );
          clearWaitTimeout();
        })
        .catch(() => {
          if (cancelled) return;
          setReady(false);
          setError('Could not determine daemon status. Try retrying.');
          clearWaitTimeout();
        });
    }, 25_000);

    void subscribe();
    return () => {
      cancelled = true;
      clearWaitTimeout();
      unsub?.();
    };
    // ready is intentionally omitted: remount / retryToken owns re-subscribe.
    // Including ready caused effect re-entry that unsubscribed then early-returned.
  }, [client, desktop, retryToken]);

  function retry() {
    setReady(false);
    setError(null);
    setRetryToken((n) => n + 1);
    if (desktop) {
      void desktop.startDaemon();
    } else {
      window.location.reload();
    }
  }

  async function reset() {
    if (!desktop) return;
    setReady(false);
    setError(null);
    try {
      await desktop.resetLocalDatabase();
      await desktop.startDaemon();
      // Force the effect to re-run so it re-subscribes (or probes) after reset.
      setRetryToken((n) => n + 1);
    } catch (err) {
      setError(errorMessage(err) || 'Failed to reset local database.');
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Start the daemon</h2>
        <p className="text-copy-14 text-gray-900">
          Nexus runs a local daemon that manages your workspace, agents, and creative projects.
        </p>
      </div>

      <div
        className={cn(
          'flex min-h-[120px] flex-col gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-6',
          error ? 'items-stretch justify-center' : 'items-center justify-center text-center',
        )}
      >
        {error ? (
          <>
            <div className="flex justify-center">
              <Button variant="secondary" onClick={retry}>
                <RefreshCw className="h-4 w-4" aria-hidden />
                Retry
              </Button>
            </div>
            <p className="whitespace-pre-wrap break-words text-left text-copy-12 leading-snug text-red-800">
              {error}
            </p>
            {desktop && (
              <div className="flex flex-col items-center gap-2">
                <Button variant="tertiary" onClick={reset}>
                  Reset local database
                </Button>
                <p className="max-w-[320px] text-center text-copy-12 text-gray-800">
                  This will clear the daemon&apos;s local state database (config, registry cache). Your creative files in the workspace are not affected.
                </p>
              </div>
            )}
          </>
        ) : ready ? (
          <p className="text-copy-14 text-green-800">Daemon is running.</p>
        ) : (
          <>
            <Loader2 className="h-6 w-6 animate-spin text-blue-700" aria-hidden />
            <p className="text-copy-14 text-gray-900">Starting daemon…</p>
          </>
        )}
      </div>

      <div
        className="mt-auto flex items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        <Button variant="tertiary" onClick={onBack} aria-label="Back" className="px-2">
          <ChevronLeft className="h-4 w-4" aria-hidden="true" />
        </Button>
        <Button
          variant="primary"
          onClick={onNext}
          disabled={!ready}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          Continue
        </Button>
      </div>
    </div>
  );
}
