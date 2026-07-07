import { useEffect, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useNexusClient, useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';

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

    async function probe() {
      try {
        await client.health();
        if (!cancelled) setReady(true);
      } catch (err) {
        if (!cancelled) {
          setError(errorMessage(err) || 'Could not reach the daemon.');
        }
      }
    }

    async function subscribe() {
      if (!desktop) {
        await probe();
        return;
      }
      try {
        unsub = await desktop.onDaemonStatusChanged((status) => {
          if (cancelled) return;
          if (status.state === 'running') {
            setReady(true);
            setError(null);
          } else if (status.state === 'error' || status.state === 'stopped') {
            setError(status.detail ?? `Daemon is ${status.state}.`);
          }
        });
      } catch {
        // Fall back to polling.
        await probe();
      }
    }

    void subscribe();
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [client, desktop, ready, retryToken]);

  function retry() {
    setError(null);
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
            <p className="text-copy-14 text-red-800">{error}</p>
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

      <div className="flex justify-between">
        <Button variant="tertiary" onClick={onBack}>Back</Button>
        <Button variant="primary" onClick={onNext} disabled={!ready}>
          Continue
        </Button>
      </div>
    </div>
  );
}
