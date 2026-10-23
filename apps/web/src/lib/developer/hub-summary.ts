/**
 * Hub summary hooks (plan v1220 T2) — one aggregate per 操作台 card.
 *
 * Each hook combines EXISTING browser-client queries into its card's view
 * model and exposes one `{ data, isLoading, error, retry }` state, so a failing
 * source only blanks its own card (per-card error isolation) and every card can
 * render the shared loading / error copy. No new wire contract (IA §6).
 */
import { useQuery } from '@tanstack/react-query';

import {
  useComputeModules,
  usePresets,
  useSavedAgentProfile,
  useScanAgents,
  useSessions,
} from '@/api/queries';
import { resolveAgentBadge } from '@/components/layout/daemon-status-bar';
import { useConnectionConfig, useNexusClient } from '@/lib/client-context';
import type { ConnectionConfig } from '@/lib/nexus/connection-storage';

/** One card's data plus its section-scoped loading / error / retry state. */
export interface HubSectionState<T> {
  data: T | undefined;
  isLoading: boolean;
  error: Error | null;
  retry: () => void;
}

// ── 本地服务 ────────────────────────────────────────────────────────────────

export interface ServiceSummary {
  /** `ok` when the daemon reports itself healthy. */
  status: string;
  version: string;
  /** The URL the active client is pointed at (the service entry point). */
  entryUrl: string;
  degradedSubsystems: string[];
  degradedReasons: string[];
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
    queryKey: ['hub', 'service', 'health'],
    queryFn: () => client.health(),
  });
  const status = useQuery({
    queryKey: ['hub', 'service', 'status'],
    queryFn: () => client.daemonStatus(),
  });

  const data =
    health.data && status.data
      ? {
          status: health.data.status,
          version: health.data.version,
          entryUrl: resolveEntryUrl(config),
          degradedSubsystems: status.data.degraded.subsystems,
          degradedReasons: status.data.degraded.reasons,
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
  const profile = useSavedAgentProfile();

  const data: AgentSummary | undefined = scan.data
    ? profile
      ? resolveAgentBadge(profile, scan.data.agents)
      : { displayName: null, version: null, connected: false }
    : undefined;

  return {
    data,
    isLoading: scan.isLoading,
    error: scan.error,
    retry: () => void scan.refetch(),
  };
}

// ── Harness 摘要 ────────────────────────────────────────────────────────────

export interface HarnessSummary {
  presetCount: number;
  runningSessions: number;
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

export function useHarnessSummary(): HubSectionState<HarnessSummary> {
  const presets = usePresets();
  const sessions = useSessions();

  const data =
    presets.data && sessions.data
      ? {
          presetCount:
            presets.data.embedded.length +
            presets.data.system.length +
            presets.data.user.length,
          runningSessions: countRunningSessions(sessions.data),
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
