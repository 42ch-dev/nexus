/**
 * Global service-health state for the service-unavailable surface
 * (plan v1219-onboarding T4).
 *
 * Probe: the unguarded `GET /v1/daemon/daemon/status`. A rejected probe means
 * the service cannot be reached at all; a non-empty `degraded.subsystems` means
 * it answers but cannot serve the API. Both collapse into one `reason` string
 * updated in place — this provider never raises toasts.
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
import { errorMessage } from '@/lib/error-message';
import { useSetupCompleted } from '@/lib/setup-completed-context';

/** `degraded.subsystems` → the `{reason}` slot in the product copy. */
const SUBSYSTEM_REASON: Record<string, string> = {
  db: '本地数据库未就绪',
  engine: '编排引擎未就绪',
};

interface ServiceStatusValue {
  /** Set while the service is unreachable or degraded; `null` when healthy. */
  reason: string | null;
  /** True while a probe (initial or retry) is in flight. */
  checking: boolean;
  /** Re-probe; a successful probe also re-reads the first-run gate. */
  retry: () => Promise<void>;
}

const ServiceStatusContext = createContext<ServiceStatusValue | null>(null);

export function ServiceStatusProvider({ children }: { children: ReactNode }) {
  const client = useNexusClient();
  const { completed, refresh } = useSetupCompleted();
  const [reason, setReason] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const mounted = useRef(false);

  /** @returns whether the service answered. */
  const probe = useCallback(async (): Promise<boolean> => {
    if (mounted.current) setChecking(true);
    try {
      const status = await client.daemonStatus();
      const subsystems = completed ? status.degraded.subsystems : [];
      if (mounted.current) {
        setReason(
          subsystems.length > 0
            ? subsystems.map((name) => SUBSYSTEM_REASON[name] ?? name).join('、')
            : null,
        );
      }
      return true;
    } catch (error) {
      if (mounted.current) setReason(errorMessage(error) || '无法连接本地服务');
      return false;
    } finally {
      if (mounted.current) setChecking(false);
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
