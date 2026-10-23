/**
 * Hub summary hooks (plan v1220 T2) — one aggregate per 操作台 card.
 *
 * Each hook combines EXISTING browser-client queries into its card's view
 * model and exposes one `{ data, isLoading, error, retry }` state, so a failing
 * source only blanks its own card (per-card error isolation) and every card can
 * render the shared loading / error copy. No new wire contract (IA §6).
 */
import { useQuery } from '@tanstack/react-query';
import type { SessionSummary } from '@42ch/nexus-contracts';

import {
  filterVisiblePresets,
  filterVisibleSessions,
  useComputeModules,
  usePresets,
  useSavedAgentProfile,
  useScanAgents,
} from '@/api/queries';
import { resolveAgentBadge } from '@/components/layout/daemon-status-bar';
import { useConnectionConfig, useNexusClient } from '@/lib/client-context';
import type { NexusClient } from '@/lib/nexus';
import type { ConnectionConfig } from '@/lib/nexus/connection-storage';
import { queryKeys } from '@/lib/nexus/query-keys';

/** One card's data plus its section-scoped loading / error / retry state. */
export interface HubSectionState<T> {
  data: T | undefined;
  isLoading: boolean;
  error: Error | null;
  retry: () => void;
}

// ── 本地服务 ────────────────────────────────────────────────────────────────

export interface ServiceSummary {
  version: string;
  /** The URL the active client is pointed at (the service entry point). */
  entryUrl: string;
  degradedSubsystems: string[];
}

/**
 * The service entry URL: the active remote endpoint, else this document's
 * origin (the same-origin loopback the browser client targets by default).
 */
export function resolveEntryUrl(config: ConnectionConfig | null): string {
  if (config?.active === true && config.endpointUrl) return config.endpointUrl;
  return typeof window === 'undefined' ? '' : window.location.origin;
}

export function useServiceSummary(): HubSectionState<ServiceSummary> {
  const client = useNexusClient();
  const config = useConnectionConfig();
  const health = useQuery({
    queryKey: queryKeys.hub.service.health(),
    queryFn: () => client.health(),
  });
  const status = useQuery({
    queryKey: queryKeys.hub.service.status(),
    queryFn: () => client.daemonStatus(),
  });

  const data =
    health.data && status.data
      ? {
          version: health.data.version,
          entryUrl: resolveEntryUrl(config),
          degradedSubsystems: status.data.degraded.subsystems,
        }
      : undefined;

  return {
    data,
    isLoading: health.isLoading || status.isLoading,
    error: health.error ?? status.error,
    retry: () => {
      void health.refetch();
      void status.refetch();
    },
  };
}

// ── Agent（ACP） ────────────────────────────────────────────────────────────

export interface AgentSummary {
  /** `null` when no agent profile is saved yet. */
  displayName: string | null;
  version: string | null;
  connected: boolean;
}

export function useAgentSummary(): HubSectionState<AgentSummary> {
  const scan = useScanAgents({ filter: 'all' });
  const savedProfile = useSavedAgentProfile();

  // The card resolves a profile + a scan: both must be settled before it can
  // claim "no agent selected", and either failure surfaces its own reason.
  const data: AgentSummary | undefined =
    scan.data && !savedProfile.isLoading && !savedProfile.error
      ? savedProfile.profile
        ? resolveAgentBadge(savedProfile.profile, scan.data.agents)
        : { displayName: null, version: null, connected: false }
      : undefined;

  return {
    data,
    isLoading: scan.isLoading || savedProfile.isLoading,
    error: scan.error ?? savedProfile.error,
    retry: () => {
      void scan.refetch();
      savedProfile.refetch();
    },
  };
}

// ── Harness 摘要 ────────────────────────────────────────────────────────────

export interface HarnessSummary {
  presetCount: number;
  runningSessions: number;
  /** True when the session walk hit its page cap — the count is a lower bound. */
  sessionsTruncated: boolean;
}

/** Sessions requested per page while aggregating the running count. */
const HUB_SESSION_PAGE_SIZE = 100;

/**
 * Safety cap on the aggregation walk: 10 pages × 100 rows = 1000 sessions.
 * The card is a status summary, not an audit: a longer history stops at the cap
 * and is reported as truncated (a lower bound), never as an exact total.
 */
const HUB_SESSION_MAX_PAGES = 10;

/** Session rows the Hub walk may inspect before it reports a lower bound. */
export const HUB_SESSION_CAP = HUB_SESSION_MAX_PAGES * HUB_SESSION_PAGE_SIZE;

interface SessionAggregate {
  sessions: SessionSummary[];
  /** True when the cursor chain still had pages after the final allowed one. */
  truncated: boolean;
}

/**
 * Every visible session across the cursor pages, bounded by
 * {@link HUB_SESSION_MAX_PAGES}; `truncated` tells the card that the resulting
 * running count is a lower bound rather than an exact total.
 */
async function fetchAllVisibleSessions(client: NexusClient): Promise<SessionAggregate> {
  const sessions: SessionSummary[] = [];
  let cursor: string | undefined;

  for (let page = 0; page < HUB_SESSION_MAX_PAGES; page += 1) {
    const response = await client.listSessions({ limit: HUB_SESSION_PAGE_SIZE, cursor });
    sessions.push(...filterVisibleSessions(response.items));
    if (!response.pagination.has_more || !response.pagination.next_cursor) {
      return { sessions, truncated: false };
    }
    cursor = response.pagination.next_cursor;
  }

  return { sessions, truncated: true };
}

/** Terminal session statuses — mirrors the engine's `SessionStatus::is_terminal`. */
const TERMINAL_SESSION_STATUSES: Record<string, true> = {
  completed: true,
  failed: true,
  cancelled: true,
  interrupted: true,
};

/** Count live (non-terminal) sessions in a session list. */
export function countRunningSessions(sessions: readonly { status: string }[]): number {
  return sessions.filter((session) => !TERMINAL_SESSION_STATUSES[session.status]).length;
}

/**
 * Bounded visible-session aggregate shared by the Hub and the Harness catalog
 * (one query key, one cursor walk). `truncated` marks the row set as a lower
 * bound once the page cap is hit.
 */
export function useVisibleSessions() {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.hub.harnessSessions(),
    queryFn: () => fetchAllVisibleSessions(client),
    // The walk is up to ten serial requests. Without a staleness window every
    // remount (Hub ↔ Harness) replays the whole cursor chain, so it stays fresh
    // for a minute — the count is a status summary, not a live feed.
    staleTime: 60_000,
  });
}

export function useHarnessSummary(): HubSectionState<HarnessSummary> {
  const presets = usePresets();
  const sessions = useVisibleSessions();

  const data =
    presets.data && sessions.data
      ? {
          // Same visibility rule as the Harness catalog it summarizes.
          presetCount: filterVisiblePresets(presets.data).length,
          runningSessions: countRunningSessions(sessions.data.sessions),
          sessionsTruncated: sessions.data.truncated,
        }
      : undefined;

  return {
    data,
    isLoading: presets.isLoading || sessions.isLoading,
    error: presets.error ?? sessions.error,
    retry: () => {
      void presets.refetch();
      void sessions.refetch();
    },
  };
}

// ── 已装模块 ────────────────────────────────────────────────────────────────

export interface ModulesSummary {
  count: number;
  names: string[];
}

export function useModulesSummary(): HubSectionState<ModulesSummary> {
  const modules = useComputeModules();

  return {
    data: modules.data
      ? { count: modules.data.length, names: modules.data.map((module) => module.name) }
      : undefined,
    isLoading: modules.isLoading,
    error: modules.error,
    retry: () => void modules.refetch(),
  };
}
