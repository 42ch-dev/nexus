import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

/** Default section id — P2 extends the registry; P0 stabilizes the contract. */
export type SettingsSectionId = 'agent' | 'workspace' | 'appearance' | 'advanced';

export interface SettingsModalContextValue {
  open: boolean;
  activeSection: SettingsSectionId;
  openSettings: (
    defaultSection?: SettingsSectionId,
    invoker?: HTMLElement | null,
  ) => void;
  closeSettings: () => void;
}

const SettingsModalContext = createContext<SettingsModalContextValue | null>(null);

export function useSettingsModal(): SettingsModalContextValue {
  const ctx = useContext(SettingsModalContext);
  if (!ctx) {
    throw new Error('useSettingsModal must be used within SettingsModalProvider');
  }
  return ctx;
}

export function SettingsModalProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [activeSection, setActiveSection] = useState<SettingsSectionId>('agent');
  const invokerRef = useRef<HTMLElement | null>(null);

  const openSettings = useCallback(
    (defaultSection: SettingsSectionId = 'agent', invoker?: HTMLElement | null) => {
      invokerRef.current = invoker ?? null;
      setActiveSection(defaultSection);
      setOpen(true);
    },
    [],
  );

  const closeSettings = useCallback(() => {
    setOpen(false);
    const invoker = invokerRef.current;
    invokerRef.current = null;
    queueMicrotask(() => invoker?.focus());
  }, []);

  const value = useMemo(
    () => ({
      open,
      activeSection,
      openSettings,
      closeSettings,
    }),
    [open, activeSection, openSettings, closeSettings],
  );

  return (
    <SettingsModalContext.Provider value={value}>
      {children}
    </SettingsModalContext.Provider>
  );
}
