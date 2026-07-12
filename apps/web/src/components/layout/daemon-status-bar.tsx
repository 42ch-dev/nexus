/**
 * Desktop daemon status bar — persistent footer strip for the desktop shell.
 *
 * V1.102: single-line footer — left = status dot + short title + soft Badge;
 * right = Restart control. No secondary description. Non-running states remain
 * surfaced by the top-of-main-content {@link MainBanner}, not this footer bar.
 *
 * Browser build: returns `null`.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import { useToast } from '@/lib/use-toast';

const STATUS_SYNC_INTERVAL_MS = 10_000;

export function DaemonStatusBar() {
  const { t } = useTranslation('shell');
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
    const confirmed = window.confirm(t('daemon.restartConfirm'));
    if (!confirmed) return;
    setIsLoading(true);
    try {
      await desktop.stopDaemon();
      await desktop.startDaemon();
      await refresh();
    } catch (err) {
      const message = errorMessage(err) || t('daemon.restartFailedFallback');
      toast({ variant: 'error', title: t('daemon.restartFailed'), description: message });
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div
      className="flex items-center justify-between gap-3 border-t border-gray-alpha-400 bg-background-100 px-4 py-2 md:px-6"
      data-testid="daemon-status-bar"
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-700" aria-hidden />
        <span className="truncate text-label-14 text-gray-1000">{t('daemon.running')}</span>
        <Badge variant="running" tone="soft">
          {t('daemon.healthy')}
        </Badge>
      </div>
      <Button
        type="button"
        variant="tertiary"
        size="small"
        onClick={handleRestart}
        disabled={isLoading}
        aria-label={t('daemon.restart')}
        title={t('daemon.restart')}
      >
        <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} aria-hidden />
      </Button>
    </div>
  );
}
