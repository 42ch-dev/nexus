/**
 * Settings modal controller — V1.131 P2.
 *
 * URL is the open-state SSOT for `/settings/*` and `/modules`. The provider
 * owns last-safe-background, section selection, dirty-source registry, and
 * every close vector via `requestClose`.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import {
  useLocation,
  useNavigate,
  type Location,
} from 'react-router-dom';

import {
  DEFAULT_SETTINGS_BACKGROUND_PATH,
  DEFAULT_SETTINGS_SECTION,
  isSettingsDrivenPath,
  resolveSettingsLocation,
  settingsLocationKey,
  settingsPathFor,
  type SettingsCloseReason,
  type SettingsSectionId,
} from '@/components/layout/settings-section-registry';

export type { SettingsCloseReason, SettingsSectionId };

export interface SettingsModalContextValue {
  open: boolean;
  activeSection: SettingsSectionId;
  /** Hash without `#` when the active section uses an in-page anchor. */
  sectionHash: string;
  /** Last non-settings location rendered behind the modal. */
  backgroundLocation: Location;
  openSettings: (
    defaultSection?: SettingsSectionId,
    invoker?: HTMLElement | null,
    hash?: string,
  ) => void;
  selectSection: (section: SettingsSectionId, hash?: string) => void;
  requestClose: (reason?: SettingsCloseReason) => void;
  registerDirtySource: (key: string, dirty: boolean) => void;
  /** Host-owned discard confirmation. */
  discardConfirmOpen: boolean;
  confirmDiscard: () => void;
  cancelDiscard: () => void;
}

const SettingsModalContext = createContext<SettingsModalContextValue | null>(
  null,
);

export function useSettingsModal(): SettingsModalContextValue {
  const ctx = useContext(SettingsModalContext);
  if (!ctx) {
    throw new Error('useSettingsModal must be used within SettingsModalProvider');
  }
  return ctx;
}

function defaultBackgroundLocation(): Location {
  return {
    pathname: DEFAULT_SETTINGS_BACKGROUND_PATH,
    search: '',
    hash: '',
    state: null,
    key: 'default',
  };
}

export function SettingsModalProvider({ children }: { children: ReactNode }) {
  const location = useLocation();
  const navigate = useNavigate();
  const invokerRef = useRef<HTMLElement | null>(null);
  const dirtyMapRef = useRef(new Map<string, boolean>());
  const [dirtyEpoch, setDirtyEpoch] = useState(0);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [backgroundLocation, setBackgroundLocation] = useState<Location>(
    defaultBackgroundLocation,
  );
  // Seed background from the first non-settings location once per session.
  const backgroundSeededRef = useRef(false);

  const resolved = resolveSettingsLocation(location.pathname, location.hash);
  const open = resolved !== null;
  const activeSection = resolved?.section ?? DEFAULT_SETTINGS_SECTION;
  const sectionHash = resolved?.hash ?? '';

  // Normalize aliases and unknown sections to the canonical Settings path.
  useEffect(() => {
    if (!resolved) return;
    const canonical = settingsLocationKey(resolved);
    const current = location.hash
      ? `${location.pathname}${location.hash}`
      : location.pathname;
    if (current !== canonical) {
      navigate(canonical, { replace: true });
    }
  }, [location.hash, location.pathname, navigate, resolved]);

  // Track last safe non-settings location (direct Settings loads keep /works).
  useEffect(() => {
    if (isSettingsDrivenPath(location.pathname)) {
      if (!backgroundSeededRef.current) {
        backgroundSeededRef.current = true;
      }
      return;
    }
    backgroundSeededRef.current = true;
    setBackgroundLocation(location);
  }, [location]);

  const isDirty = useCallback(() => {
    for (const dirty of dirtyMapRef.current.values()) {
      if (dirty) return true;
    }
    return false;
  }, [dirtyEpoch]);

  const clearDirtySources = useCallback(() => {
    dirtyMapRef.current.clear();
    setDirtyEpoch((epoch) => epoch + 1);
  }, []);

  const performClose = useCallback(() => {
    setDiscardConfirmOpen(false);
    clearDirtySources();
    const target = backgroundLocation;
    const invoker = invokerRef.current;
    invokerRef.current = null;
    navigate(
      {
        pathname: target.pathname,
        search: target.search,
        hash: target.hash,
      },
      { replace: true, state: target.state },
    );
    queueMicrotask(() => invoker?.focus());
  }, [backgroundLocation, clearDirtySources, navigate]);

  const openSettings = useCallback(
    (
      defaultSection: SettingsSectionId = DEFAULT_SETTINGS_SECTION,
      invoker?: HTMLElement | null,
      hash?: string,
    ) => {
      invokerRef.current = invoker ?? null;
      if (!isSettingsDrivenPath(location.pathname)) {
        setBackgroundLocation(location);
        backgroundSeededRef.current = true;
      }
      navigate(settingsPathFor(defaultSection, hash));
    },
    [location, navigate],
  );

  const selectSection = useCallback(
    (section: SettingsSectionId, hash?: string) => {
      navigate(settingsPathFor(section, hash));
    },
    [navigate],
  );

  const requestClose = useCallback(
    (_reason: SettingsCloseReason = 'button') => {
      if (!open) return;
      if (isDirty()) {
        setDiscardConfirmOpen(true);
        return;
      }
      performClose();
    },
    [isDirty, open, performClose],
  );

  const registerDirtySource = useCallback((key: string, dirty: boolean) => {
    const prev = dirtyMapRef.current.get(key) ?? false;
    if (prev === dirty) return;
    if (dirty) {
      dirtyMapRef.current.set(key, true);
    } else {
      dirtyMapRef.current.delete(key);
    }
    setDirtyEpoch((epoch) => epoch + 1);
  }, []);

  const confirmDiscard = useCallback(() => {
    performClose();
  }, [performClose]);

  const cancelDiscard = useCallback(() => {
    setDiscardConfirmOpen(false);
  }, []);

  const value = useMemo(
    () => ({
      open,
      activeSection,
      sectionHash,
      backgroundLocation,
      openSettings,
      selectSection,
      requestClose,
      registerDirtySource,
      discardConfirmOpen,
      confirmDiscard,
      cancelDiscard,
    }),
    [
      open,
      activeSection,
      sectionHash,
      backgroundLocation,
      openSettings,
      selectSection,
      requestClose,
      registerDirtySource,
      discardConfirmOpen,
      confirmDiscard,
      cancelDiscard,
    ],
  );

  return (
    <SettingsModalContext.Provider value={value}>
      {children}
    </SettingsModalContext.Provider>
  );
}
