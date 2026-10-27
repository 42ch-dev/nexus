/**
 * Global service-health state for the service-unavailable surface
 * (plan v1219-onboarding T4).
 *
 * Probe: the unguarded `GET /v1/daemon/daemon/status`. A rejected probe means
 * the service cannot be reached; a non-empty `degraded.subsystems` means it
 * answers but cannot serve the API. Both collapse into one structured reason
 * updated in place — this provider never raises toasts, and the surface owns
 * the localization.
 *
 * The service derives `db` from `workspace_initialized`, and `engine` follows
 * it (`routes.ts` `degradedSubsystems`). A first run therefore reports both
 * degraded while the wizard is exactly what resolves that state, so degraded
 * subsystems only count once the first-run gate has resolved `completed`:
 * otherwise every fresh install would be told the service is unavailable
 * instead of seeing onboarding.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { useSetupCompleted } from '@/lib/setup-completed-context';

/**
 * Why the service-unavailable surface is showing.
 *
 * - `unreachable` — transport failure (no HTTP response). Rendered with a fixed
 *   localized reason; the raw fetch message ("Failed to fetch") is not copy.
 * - `server` — the service answered with an error body; its message is the
 *   actionable reason and is shown as-is.
 * - `subsystems` — the service answers but reports degraded subsystems; the
 *   surface localizes each name.
 * - `provider_unselected` — the workspace is ready and only the orchestration
 *   engine is degraded because no provider is selected yet. That is a setup
 *   step, not an unavailable service: the surface guides instead of retrying.
 */
export type ServiceDegradedReason =
  | { kind: 'unreachable' }
  | { kind: 'server'; message: string }
  | { kind: 'subsystems'; subsystems: string[] }
  | { kind: 'provider_unselected' };

interface ServiceStatusValue {
  /** Set while the service is unreachable or degraded; `null` when healthy. */
  reason: ServiceDegradedReason | null;
  /** True while a probe (initial or retry) is in flight. */
  checking: boolean;
  /** Re-probe; a successful probe also re-reads the first-run gate. */
  retry: () => Promise<void>;
}

const ServiceStatusContext = createContext<ServiceStatusValue | null>(null);

export function ServiceStatusProvider({ children }: { children: ReactNode }) {
  const client = useNexusClient();
  const { completed, refresh } = useSetupCompleted();
  const [reason, setReason] = useState<ServiceDegradedReason | null>(null);
  const [checking, setChecking] = useState(false);
  const mounted = useRef(false);
  // Monotonic probe generation: an in-flight probe whose generation is no
  // longer the latest (a newer probe started, or the component unmounted)
  // must not overwrite the newer outcome.
  const generation = useRef(0);

  /** @returns whether the service answered and this probe is still the latest. */
  const probe = useCallback(async (): Promise<boolean> => {
    const mine = ++generation.current;
    const isLatest = () => mine === generation.current && mounted.current;
    if (mounted.current) setChecking(true);
    try {
      const status = await client.daemonStatus();
      if (!isLatest()) return false;
      const subsystems = completed ? status.degraded.subsystems : [];
      if (subsystems.length === 1 && subsystems[0] === 'engine') {
        // A ready workspace whose only degraded subsystem is the engine has no
        // provider selected yet (the owner itself is established at startup).
        const runtime = await client.runtimeStatus().catch(() => null);
        if (!isLatest()) return false;
        if (runtime?.workspace_initialized) {
          setReason({ kind: 'provider_unselected' });
          return true;
        }
      }
      setReason(subsystems.length > 0 ? { kind: 'subsystems', subsystems } : null);
      return true;
    } catch (error) {
      if (isLatest()) {
        const serverMessage =
          error instanceof NexusClientError && error.status >= 400 ? error.message : '';
        setReason(serverMessage ? { kind: 'server', message: serverMessage } : { kind: 'unreachable' });
      }
      return false;
    } finally {
      if (isLatest()) setChecking(false);
    }
  }, [client, completed]);

  useEffect(() => {
    mounted.current = true;
    void probe();
    return () => {
      mounted.current = false;
    };
  }, [probe]);

  const retry = useCallback(async () => {
    // Re-reading the first-run gate only after a successful probe keeps a
    // recovered, initialized service from leaving the app on the wizard route.
    if (await probe()) await refresh();
  }, [probe, refresh]);

  return (
    <ServiceStatusContext.Provider value={{ reason, checking, retry }}>
      {children}
    </ServiceStatusContext.Provider>
  );
}

export function useServiceStatus(): ServiceStatusValue {
  const ctx = useContext(ServiceStatusContext);
  if (!ctx) throw new Error('useServiceStatus must be used within ServiceStatusProvider');
  return ctx;
}

/**
 * `null` when no provider is mounted. The shell banner uses this so a
 * standalone shell render (component tests) does not need the app-level
 * provider; production always mounts it above the route tree.
 */
export function useOptionalServiceStatus(): ServiceStatusValue | null {
  return useContext(ServiceStatusContext);
}
