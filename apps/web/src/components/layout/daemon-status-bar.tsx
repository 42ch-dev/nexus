/**
 * Desktop daemon status bar — persistent footer strip for the desktop shell.
 *
 * DESIGN.md "Daemon Status Indicator" §6.5:
 *   - 5 states: starting / running / degraded / stopped / error.
 *   - Text + color (never color alone).
 *   - Manual Restart Daemon action; confirmation on stop because it interrupts
 *     running orchestration.
 *
 * Browser build: this component returns `null`; the header
 * `DaemonHealthIndicator` handles the passive browser health probe.
 */
import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import { useToast } from '@/lib/use-toast';

interface StateDisplay {
  label: string;
  helper: string;
}

function displayFor(status: DaemonStatus): StateDisplay {
  switch (status.state) {
    case 'starting':
      return {
        label: 'Daemon starting…',
        helper: status.detail ?? 'Nexus is starting the local daemon.',
      };
    case 'running':
      return {
        label: 'Daemon running',
        helper: status.detail ?? 'Local API is reachable on the configured port.',
      };
    case 'degraded':
      return {
        label: 'Daemon reconnecting',
        helper: status.detail ?? 'Nexus is retrying the local daemon connection.',
      };
    case 'stopped':
      return {
        label: 'Daemon stopped',
        helper: status.detail ?? 'Restart the daemon to use local workspace features.',
      };
    case 'error': {
      // Rust sets detail to a port-conflict message or the generic boot-failure
      // copy per daemon-runtime.md §12.2. Surface the distinction in the pill.
      const generic =
        status.detail ??
        'Nexus could not start its background service. Check the logs or try restarting.';
      const isPortConflict =
        typeof status.detail === 'string' && status.detail.includes('port') && status.detail.includes('already in use');
      return {
        label: isPortConflict ? 'Port unavailable' : 'Daemon did not start',
        helper: generic,
      };
    }
    default:
      return {
        label: 'Daemon status unknown',
        helper: 'Nexus is checking the local daemon.',
      };
  }
}

function statusPillClass(state: DaemonStatus['state']): string {
  switch (state) {
    case 'running':
      return 'bg-[color-mix(in_srgb,var(--color-green-700)_10%,transparent)] text-green-1000 border-[color-mix(in_srgb,var(--color-green-700)_30%,transparent)]';
    case 'starting':
      return 'bg-[color-mix(in_srgb,var(--color-teal-700)_10%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_30%,transparent)]';
    case 'degraded':
      return 'bg-[color-mix(in_srgb,var(--color-amber-700)_12%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)]';
    case 'stopped':
    case 'error':
      return 'bg-[color-mix(in_srgb,var(--color-red-700)_12%,transparent)] text-red-1000 border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)]';
    default:
      return 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300';
  }
}

export function DaemonStatusBar() {
  const desktop = useDesktopCapabilities();
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const { toast } = useToast();
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    if (!desktop) return;
    try {
      const next = await desktop.getDaemonStatus();
      if (mounted.current) setStatus(next);
    } catch {
      // Leave last-known status; the next poll will retry.
    }
  }, [desktop]);

  useEffect(() => {
    mounted.current = true;
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      if (!desktop) return;
      // Fetch initial status immediately, then subscribe to Rust-side events
      // for live updates (QC1-S1 replaces the 5 s React poll loop).
      await refresh();
      unlisten = await desktop.onDaemonStatusChanged((next) => {
        if (mounted.current) setStatus(next);
      });
    };

    void setup();
    return () => {
      mounted.current = false;
      unlisten?.();
    };
  }, [desktop, refresh]);

  if (!desktop) return null;

  const display = status ? displayFor(status) : { label: 'Daemon status unknown', helper: 'Nexus is checking the local daemon.' };
  const state = status?.state ?? 'starting';
  const actionLabel = state === 'running' || state === 'degraded' ? 'Restart Daemon' : 'Start Daemon';

  const handleAction = async () => {
    if (!desktop) return;
    const willRestart = state === 'running' || state === 'degraded';
    if (willRestart) {
      const confirmed = window.confirm(
        'Restarting the daemon will interrupt any running orchestration. Continue?',
      );
      if (!confirmed) return;
    }
    setIsLoading(true);
    try {
      if (willRestart) {
        // A real restart: stop (graceful SIGTERM → timeout → SIGKILL) then
        // start. Calling startDaemon() while running is a no-op because Rust
        // manager.start() early-returns when the state is Running/Starting.
        await desktop.stopDaemon();
      }
      await desktop.startDaemon();
      await refresh();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast({
        variant: 'error',
        title: 'Daemon action failed',
        description: message,
      });
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-4 border-t border-gray-alpha-400 bg-background-100 px-4 py-2 md:px-6">
      <div className="flex min-w-0 items-center gap-3">
        <span
          className={`inline-flex h-6 items-center rounded-pill border px-2 text-label-12 font-semibold whitespace-nowrap ${statusPillClass(state)}`}
        >
          {display.label}
        </span>
        <span className="truncate text-copy-13 text-gray-900">{display.helper}</span>
        {status?.port ? (
          <span className="hidden text-copy-13-mono text-gray-700 md:inline">port {status.port}</span>
        ) : null}
      </div>
      <Button
        variant="secondary"
        size="small"
        onClick={handleAction}
        disabled={isLoading}
        aria-label={actionLabel}
      >
        {isLoading ? 'Working…' : actionLabel}
      </Button>
    </div>
  );
}
