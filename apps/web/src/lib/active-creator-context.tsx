import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';

const STORAGE_KEY = 'nexus:activeCreatorId';

interface ActiveCreatorContextValue {
  activeCreatorId: string | null;
  setActiveCreatorId: (id: string | null) => void;
}

const ActiveCreatorContext = createContext<ActiveCreatorContextValue | null>(null);

function readStoredCreatorId(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    return window.localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function writeStoredCreatorId(id: string | null): void {
  if (typeof window === 'undefined') return;
  try {
    if (id === null) {
      window.localStorage.removeItem(STORAGE_KEY);
    } else {
      window.localStorage.setItem(STORAGE_KEY, id);
    }
  } catch {
    // Storage may be disabled in some environments; persistence is best-effort.
  }
}

export interface ActiveCreatorProviderProps {
  children: ReactNode;
  initialCreatorId?: string | null;
}

/**
 * Provides the active creator id selected by the footer profile switcher.
 *
 * The value is persisted to `localStorage` (browser) / the Tauri store
 * equivalent (desktop) and restored on reload. Components that need the active
 * creator read from {@link useActiveCreatorId}; components that change it read
 * from {@link useSetActiveCreatorId}.
 */
export function ActiveCreatorProvider({
  children,
  initialCreatorId,
}: ActiveCreatorProviderProps) {
  const [activeCreatorId, setActiveCreatorIdState] = useState<string | null>(() => {
    if (initialCreatorId !== undefined) return initialCreatorId;
    return readStoredCreatorId();
  });

  const setActiveCreatorId = useCallback((id: string | null) => {
    setActiveCreatorIdState(id);
    writeStoredCreatorId(id);
  }, []);

  // Sync across tabs (best-effort).
  useEffect(() => {
    function onStorage(event: StorageEvent) {
      if (event.key === STORAGE_KEY) {
        setActiveCreatorIdState(event.newValue);
      }
    }
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  return (
    <ActiveCreatorContext.Provider value={{ activeCreatorId, setActiveCreatorId }}>
      {children}
    </ActiveCreatorContext.Provider>
  );
}

export function useActiveCreatorId(): string | null {
  const ctx = useContext(ActiveCreatorContext);
  if (!ctx) throw new Error('useActiveCreatorId must be used within ActiveCreatorProvider');
  return ctx.activeCreatorId;
}

export function useSetActiveCreatorId(): (id: string | null) => void {
  const ctx = useContext(ActiveCreatorContext);
  if (!ctx) throw new Error('useSetActiveCreatorId must be used within ActiveCreatorProvider');
  return ctx.setActiveCreatorId;
}
