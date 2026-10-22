import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';

import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { isWorkspaceInitialized } from '@/lib/first-run';

interface SetupCompletedContextValue {
  /** Whether the first-launch setup wizard has been completed. */
  completed: boolean;
  /** True while the first-run state is being resolved (IPC or service probe). */
  isLoading: boolean;
  /**
   * Persist and sync setup-completed state (desktop IPC when available).
   *
   * - `true` (wizard finish): optimistic React state first so SetupGate does not
   *   bounce `/works` → `/setup` while IPC is in flight; IPC failure rolls back.
   * - `false` (Settings Re-run R1): await IPC success, then sync React state so
   *   gated routes never see a stale `completed: true` after clear.
   */
  setCompleted: (value: boolean) => Promise<void>;
  /**
   * Mark setup as completed — fire-and-forget {@link setCompleted}(true).
   * Safe with SetupGate because `true` is applied optimistically.
   */
  markCompleted: () => void;
}

const SetupCompletedContext = createContext<SetupCompletedContextValue | null>(null);

export interface SetupCompletedProviderProps {
  children: ReactNode;
  /** Override the initial value (tests). */
  initialCompleted?: boolean;
}

/**
 * Tracks whether the first-launch setup wizard has been completed.
 *
 * Browser build: derived from the unguarded runtime status
 * (`workspace_initialized`, compass D21) — no per-browser marker, so a fresh
 * origin over LAN reaches the wizard and an initialized service skips it.
 *
 * Desktop build: reads the `get_setup_completed` desktop command on mount and
 * writes back via `set_setup_completed` when the user finishes the wizard or
 * re-runs setup from Settings.
 */
export function SetupCompletedProvider({
  children,
  initialCompleted,
}: SetupCompletedProviderProps) {
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  // Unresolved (no override) stays `false` + loading so SetupGate neither
  // paints the shell nor redirects to the wizard before the real answer lands.
  const [completed, setCompletedState] = useState(initialCompleted ?? false);
  const [isLoading, setIsLoading] = useState(initialCompleted === undefined);

  useEffect(() => {
    if (initialCompleted !== undefined) return;
    let cancelled = false;
    const resolved = desktop
      ? desktop.getSetupCompleted()
      : isWorkspaceInitialized(client);
    resolved
      .then((value) => {
        if (!cancelled) setCompletedState(value);
      })
      .catch(() => {
        if (cancelled) return;
        // Fail open: a missing IPC command or an unreachable service must not
        // hang the app on the wizard gate.
        // simplify: browser probe failure lands on the shell; T4 owns the
        // wizard-level degraded surface (plan v1219-onboarding T4).
        setCompletedState(true);
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [client, desktop, initialCompleted]);

  const setCompleted = useCallback(
    async (value: boolean) => {
      if (!desktop) {
        // Browser: there is no marker to write — re-reading the service is
        // what flips the derived gate (compass D21). `false` only reflects the
        // always-reachable wizard route.
        setCompletedState(value ? await isWorkspaceInitialized(client) : false);
        return;
      }
      if (value) {
        // Optimistic: wizard finish navigates to gated routes immediately.
        setCompletedState(true);
        try {
          await desktop.setSetupCompleted(true);
        } catch (err) {
          setCompletedState(false);
          throw err;
        }
        return;
      }

      // R1 clear: await IPC before React state so gated UI never sees stale true.
      await desktop.setSetupCompleted(false);
      setCompletedState(false);
    },
    [client, desktop],
  );

  const markCompleted = useCallback(() => {
    // Fire-and-forget by contract: a browser re-probe can reject (daemon
    // unreachable) and must not surface as an unhandled rejection — the gate
    // simply keeps its current value.
    void setCompleted(true).catch(() => undefined);
  }, [setCompleted]);

  return (
    <SetupCompletedContext.Provider value={{ completed, isLoading, setCompleted, markCompleted }}>
      {children}
    </SetupCompletedContext.Provider>
  );
}

export function useSetupCompleted(): SetupCompletedContextValue {
  const ctx = useContext(SetupCompletedContext);
  if (!ctx) throw new Error('useSetupCompleted must be used within SetupCompletedProvider');
  return ctx;
}
