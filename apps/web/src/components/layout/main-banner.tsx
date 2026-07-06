import { useCallback, useEffect, useRef, useState } from 'react';
import { RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import { useToast } from '@/lib/use-toast';

const STATUS_SYNC_INTERVAL_MS = 10_000;

/**
 * Top-of-main-content banner for degraded / stopped / error daemon states.
 *
 * V1.94: the daemon status bar no longer shows state pills in the main UI. When
 * the daemon is not running, this banner surfaces the failure with plain-language
 * detail and a Restart CTA. It is rendered inside the main content area by
 * {@link RootLayout}.
 */
export function MainBanner() {
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
      // Leave last-known status.
    }
  }, [desktop]);

  useEffect(() => {
    mounted.current = true;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let syncInterval: ReturnType<typeof setInterval> | undefined;

    const setup = async () => {
      if (!desktop) return;
      await refresh();
      if (cancelled) return;
      unlisten = await desktop.onDaemonStatusChanged((next) => {
        if (mounted.current) setStatus(next);
      });
      if (cancelled) {
        unlisten();
        unlisten = undefined;
        return;
      }
      syncInterval = setInterval(() => {
        void refresh();
      }, STATUS_SYNC_INTERVAL_MS);
    };

    void setup();
    return () => {
      cancelled = true;
      mounted.current = false;
      unlisten?.();
      if (syncInterval) clearInterval(syncInterval);
    };
  }, [desktop, refresh]);

  if (!desktop) return null;

  const state = status?.state ?? 'starting';
  if (state === 'running') return null;

  const { title, description } = messageFor(status);

  const handleRestart = async () => {
    if (!desktop) return;
    setIsLoading(true);
    try {
      if (state !== 'stopped') {
        await desktop.stopDaemon();
      }
      await desktop.startDaemon();
      await refresh();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast({ variant: 'error', title: 'Daemon restart failed', description: message });
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="border-b border-gray-alpha-400 bg-amber-700/10 px-6 py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col">
          <span className="text-copy-14 font-semibold text-gray-1000">{title}</span>
          {description && <span className="text-copy-13 text-gray-900">{description}</span>}
        </div>
        <Button
          type="button"
          variant="primary"
          size="small"
          onClick={handleRestart}
          disabled={isLoading}
        >
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} aria-hidden />
          {isLoading ? 'Restarting…' : 'Restart Daemon'}
        </Button>
      </div>
    </div>
  );
}

function messageFor(status: DaemonStatus | null): { title: string; description?: string } {
  if (!status) {
    return { title: 'Checking daemon…' };
  }
  switch (status.state) {
    case 'starting':
      return {
        title: 'Daemon starting…',
        description: status.detail ?? 'Nexus is starting the local daemon.',
      };
    case 'degraded':
      return {
        title: 'Daemon reconnecting',
        description: status.detail ?? 'Nexus is retrying the local daemon connection.',
      };
    case 'stopped':
      return {
        title: 'Daemon stopped',
        description: status.detail ?? 'Restart the daemon to use local workspace features.',
      };
    case 'error': {
      const isPortConflict =
        typeof status.detail === 'string' &&
        status.detail.includes('port') &&
        status.detail.includes('already in use');
      return {
        title: isPortConflict ? 'Port unavailable' : 'Daemon did not start',
        description:
          status.detail ??
          'Nexus could not start its background service. Check the logs or try restarting.',
      };
    }
    default:
      return { title: 'Daemon status unknown' };
  }
}
