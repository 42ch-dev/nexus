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
  isEntranceId,
  type EntranceId,
} from '@/components/layout/entrance-registry';

/** Browser persistence key (AR-16) — `nexus-` convention like `nexus-web-locale`. */
export const ENTRANCE_STORAGE_KEY = 'nexus-entrance';

interface EntranceContextValue {
  /** The resolved user-layer entrance for the current layout tree. */
  entrance: EntranceId;
  /** True while the desktop shell is being queried for the persisted value. */
  isLoading: boolean;
  isFirstRun: boolean;
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

interface EntranceIpcSeam {
  getEntrance?: () => Promise<EntranceId>;
  setEntrance?: (value: EntranceId) => Promise<void>;
}

function parseEntrance(value: string | null): EntranceId | null {
  return isEntranceId(value) ? value : null;
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

  const [urlOverride] = useState(() => parseEntrance(searchParams.get('entrance')));

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
    if (!hasIpcSeam) return readBrowserStoredEntrance() ?? DEFAULT_ENTRANCE;
    return DEFAULT_ENTRANCE;
  });
  const [isLoading, setIsLoading] = useState(
    () => hasIpcSeam && initialEntrance === undefined && urlOverride === null,
  );
  const [hasStoredEntrance, setHasStoredEntrance] = useState(() => {
    if (initialEntrance !== undefined) return true;
    if (!hasIpcSeam) return readBrowserStoredEntrance() !== null;
    return false;
  });

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
        setHasStoredEntrance(true);
      })
      .catch(() => {
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
      setHasStoredEntrance(true);
    },
    [persister],
  );

  const isFirstRun = !hasIpcSeam && !hasStoredEntrance;

  return (
    <EntranceContext.Provider value={{ entrance, isLoading, isFirstRun, setEntrance }}>
      {children}
    </EntranceContext.Provider>
  );
}

export function useEntrance(): EntranceContextValue {
  const ctx = useContext(EntranceContext);
  if (!ctx) throw new Error('useEntrance must be used within EntranceProvider');
  return ctx;
}
