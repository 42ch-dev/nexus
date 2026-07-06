/**
 * Desktop daemon status bar — persistent footer strip for the desktop shell.
 *
 * V1.94 simplification: when the daemon is running, the status bar shows only
 * a restart-icon button (no pill, no state text, no enabled Start button). The
 * restart action still confirms because it interrupts running orchestration.
 * Degraded / stopped / error states are surfaced by the top-of-main-content
 * {@link MainBanner}, not this footer bar.
 *
 * Browser build: returns `null`.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import { useToast } from '@/lib/use-toast';

const STATUS_SYNC_INTERVAL_MS = 10_000;

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
      // Leave last-known status; the fallback re-sync will retry.
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
  if (state !== 'running') return null;

  const handleRestart = async () => {
    if (!desktop) return;
    const confirmed = window.confirm(
      'Restarting the daemon will interrupt any running orchestration. Continue?',
    );
    if (!confirmed) return;
    setIsLoading(true);
    try {
      await desktop.stopDaemon();
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
    <div className="flex items-center justify-end border-t border-gray-alpha-400 bg-background-100 px-4 py-2 md:px-6">
      <Button
        type="button"
        variant="tertiary"
        size="small"
        onClick={handleRestart}
        disabled={isLoading}
        aria-label="Restart daemon"
        title="Restart daemon"
      >
        <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} aria-hidden />
      </Button>
    </div>
  );
}
