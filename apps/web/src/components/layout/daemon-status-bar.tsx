/**
 * Desktop daemon status bar — persistent footer strip for the desktop shell.
 *
 * V1.117 P2 (T3): single-line footer — left = state dot + "Daemon running"
 * label + lowercase `running` tag; right = clickable agent badge (name+version
 * or placeholder, navigates to `/settings/agent`) + Restart control. Non-running
 * states remain surfaced by the top-of-main-content {@link MainBanner}, not here.
 *
 * Browser build: returns `null`.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import type { DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import { useToast } from '@/lib/use-toast';
import { launchCommandMatches, useAgentProfile, useScanAgents } from '@/api/queries';
import { resolveCatalogItem } from '@/lib/agent-catalog';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

interface AgentBadgeInfo {
  displayName: string;
  version: string | null;
}

/**
 * Resolve a saved agent profile onto the latest scan result for the status bar
 * badge (AD-P2-4). Prefers an installed scan entry that matches the profile by
 * name or launch command so the overrides `displayName` + version are applied;
 * falls back to the raw profile name with no version when nothing matches.
 */
function resolveAgentBadge(
  profile: { name: string; launchCommand?: string },
  agents: AgentScanEntry[],
): AgentBadgeInfo {
  const byName = agents.find((a) => a.installed && a.name === profile.name);
  const customCommand = profile.launchCommand?.trim();
  const byCommand = customCommand
    ? agents.find(
        (a) =>
          a.installed &&
          !!a.launch_command &&
          launchCommandMatches(customCommand, a.launch_command),
      )
    : null;
  const entry = byName ?? byCommand ?? null;
  if (entry) {
    const item = resolveCatalogItem(entry);
    return { displayName: item.displayName, version: item.version ?? null };
  }
  return { displayName: profile.name, version: null };
}

const STATUS_SYNC_INTERVAL_MS = 10_000;

export function DaemonStatusBar() {
  const { t } = useTranslation('shell');
  const desktop = useDesktopCapabilities();
  const navigate = useNavigate();
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const { toast } = useToast();
  const mounted = useRef(true);

  // Reuse the React Query scan cache (shared with the Settings/Setup scan hook
  // via the agentHost scan query key). This is a lightweight PATH scan — global
  // staleTime dedupes refetches. Do not block the bar on scan resolution; the
  // badge falls back to the placeholder / raw profile name (AD-P2-4).
  const scan = useScanAgents({ filter: 'all' });
  const agents = scan.data?.agents ?? [];

  // V1.120 P1 (T1): the saved profile is React-Query-backed so the Settings
  // Agent Save handler can invalidate `queryKeys.agentProfile` and the badge
  // refreshes immediately after a save — no 10s poll wait (AD-P1-1). Browser
  // build disables the hook (`desktop === null`) and `data` stays undefined.
  const profileQuery = useAgentProfile();
  const profile = profileQuery.data ?? null;

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

  const badge = profile ? resolveAgentBadge(profile, agents) : null;
  const badgeLabel = badge
    ? badge.version
      ? `${badge.displayName} v${badge.version}`
      : badge.displayName
    : t('daemon.agentBadge.empty');

  return (
    <div
      className="flex items-center justify-between gap-3 border-t border-gray-alpha-400 bg-background-100 px-4 py-2 md:px-6"
      data-testid="daemon-status-bar"
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-700" aria-hidden />
        <span className="truncate text-label-14 text-gray-1000">{t('daemon.running')}</span>
        <Badge variant="running" tone="soft">
          {t('daemon.runningTag')}
        </Badge>
      </div>
      <div className="flex min-w-0 items-center gap-1">
        <button
          type="button"
          onClick={() => navigate('/settings/agent')}
          title={badgeLabel}
          data-testid="daemon-status-agent-badge"
          className="max-w-[40vw] truncate rounded-control px-2 py-0.5 text-label-14 text-gray-900 transition-colors duration-state ease-standard motion-reduce:transition-none hover:bg-gray-alpha-100 hover:text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-500"
        >
          {badgeLabel}
        </button>
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
    </div>
  );
}
