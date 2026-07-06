import { useEffect, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useNexusClient, useDesktopCapabilities } from '@/lib/client-context';

interface SetupStepDaemonProps {
  onNext: () => void;
  onBack: () => void;
}

export function SetupStepDaemon({ onNext, onBack }: SetupStepDaemonProps) {
  const client = useNexusClient();
  const desktop = useDesktopCapabilities();
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
          setError(err instanceof Error ? err.message : 'Could not reach the daemon.');
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
      }
      await probe();
    }

    void subscribe();
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [client, desktop, ready]);

  function retry() {
    setError(null);
    if (desktop) {
      void desktop.startDaemon();
    } else {
      window.location.reload();
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
            <Button variant="secondary" onClick={retry}>
              <RefreshCw className="h-4 w-4" aria-hidden />
              Retry
            </Button>
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
