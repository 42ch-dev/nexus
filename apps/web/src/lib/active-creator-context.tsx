import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { useCreators } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';

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

export function useDefaultProfileAutoSelect(
  activeCreatorId: string | null,
  setActiveCreatorId: (id: string | null) => void,
) {
  const client = useNexusClient();
  const creatorsQuery = useCreators({ limit: 100 });
  const items = creatorsQuery.data?.items;
  const resolved = useRef(false);

  useEffect(() => {
    if (resolved.current || !items || items.length === 0) return;
    const ids = new Set(items.map((c) => c.creator_id));
    if (activeCreatorId && ids.has(activeCreatorId)) {
      resolved.current = true;
      return;
    }

    let cancelled = false;
    void (async () => {
      // Prefer the daemon's active creator so 工作区 highlight matches config.toml.
      try {
        const active = await client.getActiveCreator();
        if (cancelled) return;
        if (active.creator_id && ids.has(active.creator_id)) {
          setActiveCreatorId(active.creator_id);
          resolved.current = true;
          return;
        }
      } catch {
        // No active creator on daemon — fall through to Default / first Profile.
      }
      if (cancelled) return;
      const defaults = items
        .filter((c) => (c.display_name ?? '').trim().toLowerCase() === 'default')
        .sort((a, b) => a.creator_id.localeCompare(b.creator_id));
      const selected = defaults[0] ?? items[0];
      if (selected) {
        setActiveCreatorId(selected.creator_id);
      }
      resolved.current = true;
    })();

    return () => {
      cancelled = true;
    };
  }, [items, activeCreatorId, setActiveCreatorId, client]);
}

/**
 * V1.130 T4: Render this inside the app tree where both ActiveCreatorProvider
 * and QueryClientProvider are available. It auto-selects the Default profile.
 * In tests without QueryClient, simply don't render this component.
 */
export function DefaultProfileCoordinator() {
  const activeCreatorId = useActiveCreatorId();
  const setActiveCreatorId = useSetActiveCreatorId();
  useDefaultProfileAutoSelect(activeCreatorId, setActiveCreatorId);
  return null;
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
