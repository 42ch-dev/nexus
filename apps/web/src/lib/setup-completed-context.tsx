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
  /** Mark setup as completed (persists via the desktop shell when available). */
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
 * writes back via `set_setup_completed` when the user finishes the wizard.
 */
export function SetupCompletedProvider({
  children,
  initialCompleted,
}: SetupCompletedProviderProps) {
  const desktop = useDesktopCapabilities();
  const [completed, setCompleted] = useState(initialCompleted ?? !desktop);
  const [isLoading, setIsLoading] = useState(Boolean(desktop) && initialCompleted === undefined);

  useEffect(() => {
    if (initialCompleted !== undefined || !desktop) return;
    let cancelled = false;
    desktop
      .getSetupCompleted()
      .then((value) => {
        if (cancelled) return;
        setCompleted(value);
      })
      .catch(() => {
        if (cancelled) return;
        // Fail open: if the desktop command is missing, treat setup as done so
        // the app does not hang on the wizard gate.
        setCompleted(true);
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, initialCompleted]);

  const markCompleted = useCallback(() => {
    setCompleted(true);
    if (desktop) {
      void desktop.setSetupCompleted(true);
    }
  }, [desktop]);

  return (
    <SetupCompletedContext.Provider value={{ completed, isLoading, markCompleted }}>
      {children}
    </SetupCompletedContext.Provider>
  );
}

export function useSetupCompleted(): SetupCompletedContextValue {
  const ctx = useContext(SetupCompletedContext);
  if (!ctx) throw new Error('useSetupCompleted must be used within SetupCompletedProvider');
  return ctx;
}
