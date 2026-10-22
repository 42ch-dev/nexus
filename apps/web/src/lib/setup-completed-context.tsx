import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
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
  /**
   * Re-read the first-run state. The service-unavailable surface calls it after
   * a successful retry so a recovered service resolves the gate without a page
   * reload (v1219-onboarding T4).
   */
  refresh: () => Promise<void>;
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
  const mounted = useRef(false);

  const refresh = useCallback(async () => {
    if (initialCompleted !== undefined) return;
    try {
      const value = desktop
        ? await desktop.getSetupCompleted()
        : await isWorkspaceInitialized(client);
      if (mounted.current) setCompletedState(value);
    } catch {
      // Browser: the service could not be reached — the service-unavailable
      // surface owns this state (plan v1219-onboarding T4), so the gate stays
      // unresolved instead of silently claiming an initialized shell. Desktop:
      // a missing/refused IPC command is a host-contract break, not a service
      // outage; fail open so it never strands the app on the wizard.
      if (mounted.current && desktop) setCompletedState(true);
    } finally {
      if (mounted.current) setIsLoading(false);
    }
  }, [client, desktop, initialCompleted]);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

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
    <SetupCompletedContext.Provider
      value={{ completed, isLoading, setCompleted, markCompleted, refresh }}
    >
      {children}
    </SetupCompletedContext.Provider>
  );
}

export function useSetupCompleted(): SetupCompletedContextValue {
  const ctx = useContext(SetupCompletedContext);
  if (!ctx) throw new Error('useSetupCompleted must be used within SetupCompletedProvider');
  return ctx;
}
