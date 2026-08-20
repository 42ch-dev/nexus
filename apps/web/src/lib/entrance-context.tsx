import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { useSearchParams } from 'react-router';

import { useDesktopCapabilities } from '@/lib/client-context';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import {
  DEFAULT_ENTRANCE,
  type EntranceId,
} from '@/components/layout/entrance-registry';

/** Browser persistence key (AR-16) — `nexus-` convention like `nexus-web-locale`. */
export const ENTRANCE_STORAGE_KEY = 'nexus-entrance';

interface EntranceContextValue {
  /** The resolved user-layer entrance for the current layout tree. */
  entrance: EntranceId;
  /** True while the desktop shell is being queried for the persisted value. */
  isLoading: boolean;
  /**
   * Persist and sync the entrance (localStorage / Tauri IPC). No optimistic
   * write: a landing-layout switch must not flash the wrong tree (AR-16).
   */
  setEntrance: (value: EntranceId) => Promise<void>;
}

const EntranceContext = createContext<EntranceContextValue | null>(null);

export interface EntranceProviderProps {
  children: ReactNode;
  /** Override the initial value (tests). */
  initialEntrance?: EntranceId;
}

/** Storage/ipc seam the provider resolves the persisted entrance through. */
interface EntrancePersister {
  read(): Promise<EntranceId | null>;
  write(value: EntranceId): Promise<void>;
}

/**
 * Desktop IPC seam (AR-16). T2 wires `get_entrance` / `set_entrance` into
 * {@link DesktopCapabilities}; the provider detects them at runtime so the
 * T1 → T2 wiring needs no provider change (browser keeps localStorage).
 */
interface EntranceIpcSeam {
  getEntrance?: () => Promise<EntranceId>;
  setEntrance?: (value: EntranceId) => Promise<void>;
}

function parseEntrance(value: string | null): EntranceId | null {
  return value === 'developer' || value === 'content-creator' ? value : null;
}

function browserPersister(): EntrancePersister {
  return {
    read: () => Promise.resolve(readBrowserStoredEntrance()),
    write: (value) => {
      window.localStorage.setItem(ENTRANCE_STORAGE_KEY, value);
      return Promise.resolve();
    },
  };
}

function readBrowserStoredEntrance(): EntranceId | null {
  if (typeof window === 'undefined') return null;
  return parseEntrance(window.localStorage.getItem(ENTRANCE_STORAGE_KEY));
}

/**
 * Resolves the user-layer entrance (AR-16/AR-20).
 *
 * Precedence: URL override (`?entrance=`, session-only, never written) >
 * persisted value (desktop IPC / browser `nexus-entrance`) > `DEFAULT_ENTRANCE`
 * (`content-creator`). Invalid URL values are ignored; a stored-but-unparseable
 * value resolves content-creator WITHOUT writing — persistence happens only
 * through {@link setEntrance} (wizard step / first-run page / switch control).
 * The provider does not rewrite the URL.
 */
export function EntranceProvider({ children, initialEntrance }: EntranceProviderProps) {
  const desktop = useDesktopCapabilities();
  const [searchParams] = useSearchParams();

  const ipc = desktop as (DesktopCapabilities & EntranceIpcSeam) | null;
  const hasIpcSeam = Boolean(ipc?.getEntrance && ipc.setEntrance);

  // Session-only URL override — evaluated once at init (AR-20).
  const [urlOverride] = useState(() => parseEntrance(searchParams.get('entrance')));

  // Stable identity per desktop — the mount effect must not re-run (and
  // re-apply a stale stored read) after `setEntrance` flips the state.
  const persister = useMemo<EntrancePersister>(() => {
    if (hasIpcSeam && ipc) {
      return {
        read: () => ipc.getEntrance!(),
        write: (value) => ipc.setEntrance!(value),
      };
    }
    return browserPersister();
  }, [desktop]);

  const [entrance, setEntranceState] = useState<EntranceId>(() => {
    if (initialEntrance !== undefined) return initialEntrance;
    if (urlOverride !== null) return urlOverride;
    // Browser: synchronous localStorage read so a returning user never paints
    // the wrong tree. Desktop (IPC-only) resolves async below — unset/stale
    // falls to content-creator without writing (AR-16).
    if (!hasIpcSeam) return readBrowserStoredEntrance() ?? DEFAULT_ENTRANCE;
    return DEFAULT_ENTRANCE;
  });
  const [isLoading, setIsLoading] = useState(
    () => hasIpcSeam && initialEntrance === undefined && urlOverride === null,
  );

  useEffect(() => {
    if (initialEntrance !== undefined || !hasIpcSeam || urlOverride !== null) {
      return;
    }
    let cancelled = false;
    persister
      .read()
      .then((stored) => {
        if (cancelled || stored === null) return;
        setEntranceState(stored);
      })
      .catch(() => {
        // Fail open (mirrors SetupCompletedProvider): command error keeps the
        // default (content-creator), no write.
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [hasIpcSeam, initialEntrance, persister, urlOverride]);

  const setEntrance = useCallback(
    async (value: EntranceId) => {
      await persister.write(value);
      setEntranceState(value);
    },
    [persister],
  );

  return (
    <EntranceContext.Provider value={{ entrance, isLoading, setEntrance }}>
      {children}
    </EntranceContext.Provider>
  );
}

export function useEntrance(): EntranceContextValue {
  const ctx = useContext(EntranceContext);
  if (!ctx) throw new Error('useEntrance must be used within EntranceProvider');
  return ctx;
}
