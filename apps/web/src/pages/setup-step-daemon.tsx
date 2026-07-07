import { useEffect, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useNexusClient, useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';

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
    if (ready) return;

    let cancelled = false;
    let unsub: (() => void) | undefined;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;

    function applyStatus(status: DaemonStatus) {
      if (cancelled) return;
      if (status.state === 'running' || status.state === 'degraded') {
        setReady(true);
        setError(null);
        clearTimeout(timeoutId);
      } else if (status.state === 'starting') {
        setReady(false);
        setError(null);
      } else if (status.state === 'error' || status.state === 'stopped') {
        setReady(false);
        setError(status.detail ?? `Daemon is ${status.state}.`);
        clearTimeout(timeoutId);
      }
    }

    async function subscribe() {
      if (!desktop) {
        await probe();
        return;
      }
      try {
        const status = await desktop.getDaemonStatus();
        applyStatus(status);
        if (cancelled || status.state === 'running' || status.state === 'degraded') return;
        unsub = await desktop.onDaemonStatusChanged((status) => {
          applyStatus(status);
        });
      } catch {
        if (cancelled) return;
        await probe();
      }
    }

    async function probe() {
      try {
        await client.health();
        if (!cancelled) {
          setReady(true);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setReady(false);
          setError(errorMessage(err) || 'Could not reach the daemon.');
        }
      }
    }

    timeoutId = setTimeout(() => {
      if (cancelled) return;
      if (!desktop) {
        setError('Daemon is taking longer than expected to start.');
        return;
      }
      void desktop
        .getDaemonStatus()
        .then((status) => {
          if (cancelled) return;
          applyStatus(status);
          if (
            status.state !== 'running' &&
            status.state !== 'degraded' &&
            status.state !== 'error' &&
            status.state !== 'stopped'
          ) {
            setError(
              'Daemon is taking longer than expected to start. You can retry or reset the local database.',
            );
          }
        })
        .catch(() => {
          if (cancelled) return;
          setError('Could not determine daemon status. Try retrying.');
        });
    }, 25_000);

    void subscribe();
    return () => {
      cancelled = true;
      clearTimeout(timeoutId);
      unsub?.();
    };
  }, [client, desktop, ready, retryToken]);

  function retry() {
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

      <div className="flex min-h-[120px] flex-col items-center justify-center gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-6 text-center">
        {error ? (
          <>
            <p className="whitespace-pre-wrap break-words text-copy-14 text-red-800">{error}</p>
            <div className="flex flex-col items-center gap-2">
              <Button variant="secondary" onClick={retry}>
                <RefreshCw className="h-4 w-4" aria-hidden />
                Retry
              </Button>
              {desktop && (
                <>
                  <Button variant="tertiary" onClick={reset}>
                    Reset local database
                  </Button>
                  <p className="max-w-[320px] text-copy-12 text-gray-800">
                    This will clear the daemon&apos;s local state database (config, registry cache). Your creative files in the workspace are not affected.
                  </p>
                </>
              )}
            </div>
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

      <div className="flex flex-col gap-setup-wizard-surface-cta-container-gap mt-auto">
        <Button
          variant="primary"
          onClick={onNext}
          disabled={!ready}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          Continue
        </Button>
        <Button variant="tertiary" onClick={onBack} className="self-start">
          Back
        </Button>
      </div>
    </div>
  );
}
