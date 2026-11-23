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
   *
   * - `true` (wizard finish): optimistic React state first so SetupGate does not
   *   bounce `/works` → `/setup` while IPC is in flight; IPC failure rolls back.
   * - `false` (Settings Re-run R1): await IPC success, then sync React state so
   *   gated routes never see a stale `completed: true` after clear.
   */
  setCompleted: (value: boolean) => Promise<void>;
  /**
   * Safe with SetupGate because `true` is applied optimistically.
   */
  markCompleted: () => void;
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
        setCompletedState(value ? await isWorkspaceInitialized(client) : false);
        return;
      }
      if (value) {
        setCompletedState(true);
        try {
          await desktop.setSetupCompleted(true);
        } catch (err) {
          setCompletedState(false);
          throw err;
        }
        return;
      }

      await desktop.setSetupCompleted(false);
      setCompletedState(false);
    },
    [client, desktop],
  );

  const markCompleted = useCallback(() => {
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
