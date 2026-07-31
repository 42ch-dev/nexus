/**
 * Settings modal controller — V1.131 P2.
 *
 * URL is the open-state SSOT for `/settings/*` and `/modules`. The provider
 * owns last-safe-background, section selection, dirty-source registry, and
 * every close vector via `requestClose`.
 *
 * Route leave while Settings is open is dirty-aware: leaving a settings-driven
 * path with dirty sources opens the host discard confirm and restores the
 * Settings URL until the user confirms (BrowserRouter-compatible equivalent of
 * `useBlocker`; data-router migration is tracked separately).
 *
 * V1.147 P2 T3 — the context object is exported for null-safe consumption
 * (the Timeline canvas opens Settings → Modules with a World pre-fill deep
 * link; surfaces rendered outside the provider degrade gracefully instead of
 * throwing from `useSettingsModal`).
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
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
    search?: string,
  ) => void;
  selectSection: (section: SettingsSectionId, hash?: string) => void;
  requestClose: (reason?: SettingsCloseReason) => void;
  registerDirtySource: (key: string, dirty: boolean) => void;
  /** Host-owned discard confirmation. */
  discardConfirmOpen: boolean;
  confirmDiscard: () => void;
  cancelDiscard: () => void;
}

export const SettingsModalContext = createContext<SettingsModalContextValue | null>(
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

function isDirtyMap(map: Map<string, boolean>): boolean {
  for (const dirty of map.values()) {
    if (dirty) return true;
  }
  return false;
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
  /** Canonical Settings URL while the modal was last open. */
  const settingsPathWhileOpenRef = useRef<string | null>(null);
  /** Destination blocked by dirty route-leave (NavLink / Back / navigate). */
  const pendingLeaveRef = useRef<Location | null>(null);
  /** Skip background updates while restoring a blocked leave. */
  const suppressingBackgroundUpdateRef = useRef(false);

  const resolved = resolveSettingsLocation(location.pathname, location.hash);
  const open = resolved !== null;
  const activeSection = resolved?.section ?? DEFAULT_SETTINGS_SECTION;
  const sectionHash = resolved?.hash ?? '';

  // Normalize aliases and unknown sections to the canonical Settings path.
  // V1.147 P2 T3: `search` is preserved verbatim so deep links into a section
  // (`/settings/modules?module=…&run=…`) survive the canonicalization instead
  // of having their query params dropped on arrival.
  useEffect(() => {
    if (!resolved) return;
    const canonical = `${settingsLocationKey(resolved)}${location.search}`;
    const current = `${location.pathname}${location.search}${location.hash}`;
    if (current !== canonical) {
      navigate(canonical, { replace: true });
    }
  }, [location.hash, location.pathname, location.search, navigate, resolved]);

  // Track last safe non-settings location (direct Settings loads keep /works).
  useEffect(() => {
    if (isSettingsDrivenPath(location.pathname)) {
      if (!backgroundSeededRef.current) {
        backgroundSeededRef.current = true;
      }
      suppressingBackgroundUpdateRef.current = false;
      return;
    }
    if (suppressingBackgroundUpdateRef.current) {
      return;
    }
    backgroundSeededRef.current = true;
    setBackgroundLocation(location);
  }, [location]);

  const isDirty = useCallback(() => {
    return isDirtyMap(dirtyMapRef.current);
  }, [dirtyEpoch]);

  const clearDirtySources = useCallback(() => {
    dirtyMapRef.current.clear();
    setDirtyEpoch((epoch) => epoch + 1);
  }, []);

  // Remember the Settings URL while open so dirty leave can restore it.
  // V1.147 P2 T3: `search` is preserved so deep-linked sections (`?module=…`)
  // restore with their selection intact after a dirty-leave block.
  useEffect(() => {
    if (resolved) {
      settingsPathWhileOpenRef.current =
        `${settingsLocationKey(resolved)}${location.search}`;
    }
  }, [location.search, resolved]);

  // Dirty-aware route leave (BrowserRouter-compatible useBlocker equivalent).
  useLayoutEffect(() => {
    if (resolved) {
      return;
    }

    const settingsPath = settingsPathWhileOpenRef.current;
    if (!settingsPath) {
      return;
    }

    if (isDirtyMap(dirtyMapRef.current)) {
      pendingLeaveRef.current = location;
      suppressingBackgroundUpdateRef.current = true;
      setDiscardConfirmOpen(true);
      navigate(settingsPath, { replace: true });
      return;
    }

    // Clean leave — modal closes via open=false.
    settingsPathWhileOpenRef.current = null;
    pendingLeaveRef.current = null;
  }, [location, navigate, resolved]);

  const performClose = useCallback(() => {
    setDiscardConfirmOpen(false);
    clearDirtySources();
    pendingLeaveRef.current = null;
    settingsPathWhileOpenRef.current = null;
    suppressingBackgroundUpdateRef.current = false;
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
      search?: string,
    ) => {
      invokerRef.current = invoker ?? null;
      if (!isSettingsDrivenPath(location.pathname)) {
        setBackgroundLocation(location);
        backgroundSeededRef.current = true;
      }
      navigate(settingsPathFor(defaultSection, hash, search));
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
    const pending = pendingLeaveRef.current;
    pendingLeaveRef.current = null;
    settingsPathWhileOpenRef.current = null;
    suppressingBackgroundUpdateRef.current = false;

    if (pending) {
      setDiscardConfirmOpen(false);
      clearDirtySources();
      const invoker = invokerRef.current;
      invokerRef.current = null;
      navigate(
        {
          pathname: pending.pathname,
          search: pending.search,
          hash: pending.hash,
        },
        { replace: true, state: pending.state },
      );
      queueMicrotask(() => invoker?.focus());
      return;
    }

    performClose();
  }, [clearDirtySources, navigate, performClose]);

  const cancelDiscard = useCallback(() => {
    pendingLeaveRef.current = null;
    suppressingBackgroundUpdateRef.current = false;
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
