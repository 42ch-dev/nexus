import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';

import { useDesktopCapabilities } from '@/lib/client-context';

interface SetupCompletedContextValue {
  /** Whether the first-launch setup wizard has been completed. */
  completed: boolean;
  /** True while the desktop shell is being queried for the initial value. */
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
 * Browser build: always `completed: true` with no loading state — browser users
 * run their own daemon and skip the wizard.
 *
 * Desktop build: reads the `get_setup_completed` Tauri command on mount and
 * writes back via `set_setup_completed` when the user finishes the wizard or
 * re-runs setup from Settings.
 */
export function SetupCompletedProvider({
  children,
  initialCompleted,
}: SetupCompletedProviderProps) {
  const desktop = useDesktopCapabilities();
  const [completed, setCompletedState] = useState(initialCompleted ?? !desktop);
  const [isLoading, setIsLoading] = useState(Boolean(desktop) && initialCompleted === undefined);

  useEffect(() => {
    if (initialCompleted !== undefined || !desktop) return;
    let cancelled = false;
    desktop
      .getSetupCompleted()
      .then((value) => {
        if (cancelled) return;
        setCompletedState(value);
      })
      .catch(() => {
        if (cancelled) return;
        // Fail open: if the desktop command is missing, treat setup as done so
        // the app does not hang on the wizard gate.
        setCompletedState(true);
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, initialCompleted]);

  const setCompleted = useCallback(
    async (value: boolean) => {
      if (value) {
        // Optimistic: wizard finish navigates to gated routes immediately.
        setCompletedState(true);
        if (desktop) {
          try {
            await desktop.setSetupCompleted(true);
          } catch (err) {
            setCompletedState(false);
            throw err;
          }
        }
        return;
      }

      // R1 clear: await IPC before React state so gated UI never sees stale true.
      if (desktop) {
        await desktop.setSetupCompleted(false);
      }
      setCompletedState(false);
    },
    [desktop],
  );

  const markCompleted = useCallback(() => {
    void setCompleted(true);
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
