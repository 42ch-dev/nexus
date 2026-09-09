import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
} from 'react';

import { cn } from '../lib/cn';

interface TabsContextValue {
  value: string;
  onChange: (value: string) => void;
  baseId: string;
  triggerOrder: string[];
  registerTrigger: (value: string) => void;
  unregisterTrigger: (value: string) => void;
  setTriggerElement: (value: string, el: HTMLButtonElement | null) => void;
  focusTrigger: (value: string) => void;
  getTriggerId: (value: string) => string;
  getPanelId: (value: string) => string;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabs() {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error('Tabs components must be used inside <Tabs />');
  return ctx;
}

export interface TabsProps {
  value?: string;
  onValueChange?: (value: string) => void;
  defaultValue?: string;
  children: ReactNode;
  className?: string;
}

export function Tabs({ value, onValueChange, defaultValue, children, className }: TabsProps) {
  const [internal, setInternal] = useState(defaultValue ?? '');
  const controlled = value !== undefined;
  const active = controlled ? value : internal;
  const onChange = useCallback(
    (v: string) => {
      if (!controlled) setInternal(v);
      onValueChange?.(v);
    },
    [controlled, onValueChange],
  );

  const reactId = useId();
  const baseId = `tabs-${reactId.replace(/:/g, '')}`;
  const [triggerOrder, setTriggerOrder] = useState<string[]>([]);
  const triggerElements = useRef<Map<string, HTMLButtonElement>>(new Map());

  const registerTrigger = useCallback((triggerValue: string) => {
    setTriggerOrder((prev) => (prev.includes(triggerValue) ? prev : [...prev, triggerValue]));
  }, []);

  const unregisterTrigger = useCallback((triggerValue: string) => {
    setTriggerOrder((prev) => prev.filter((v) => v !== triggerValue));
    triggerElements.current.delete(triggerValue);
  }, []);

  const setTriggerElement = useCallback((triggerValue: string, el: HTMLButtonElement | null) => {
    if (el) triggerElements.current.set(triggerValue, el);
    else triggerElements.current.delete(triggerValue);
  }, []);

  const focusTrigger = useCallback((triggerValue: string) => {
    triggerElements.current.get(triggerValue)?.focus();
  }, []);

  const getTriggerId = useCallback((triggerValue: string) => `${baseId}-trigger-${triggerValue}`, [baseId]);
  const getPanelId = useCallback((triggerValue: string) => `${baseId}-panel-${triggerValue}`, [baseId]);

  const ctx = useMemo(
    () => ({
      value: active,
      onChange,
      baseId,
      triggerOrder,
      registerTrigger,
      unregisterTrigger,
      setTriggerElement,
      focusTrigger,
      getTriggerId,
      getPanelId,
    }),
    [
      active,
      onChange,
      baseId,
      triggerOrder,
      registerTrigger,
      unregisterTrigger,
      setTriggerElement,
      focusTrigger,
      getTriggerId,
      getPanelId,
    ],
  );

  return (
    <TabsContext.Provider value={ctx}>
      <div className={cn('flex flex-col gap-4', className)}>{children}</div>
    </TabsContext.Provider>
  );
}

export function TabsList({ children, className }: { children: ReactNode; className?: string }) {
  const { value, onChange, triggerOrder, focusTrigger } = useTabs();

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (triggerOrder.length === 0) return;

    const currentIndex = triggerOrder.indexOf(value);
    if (currentIndex === -1) return;

    let nextIndex: number | null = null;
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        nextIndex = (currentIndex + 1) % triggerOrder.length;
        break;
      case 'ArrowLeft':
      case 'ArrowUp':
        nextIndex = (currentIndex - 1 + triggerOrder.length) % triggerOrder.length;
        break;
      case 'Home':
        nextIndex = 0;
        break;
      case 'End':
        nextIndex = triggerOrder.length - 1;
        break;
      default:
        return;
    }

    event.preventDefault();
    const nextValue = triggerOrder[nextIndex];
    onChange(nextValue);
    focusTrigger(nextValue);
  };

  return (
    <div
      role="tablist"
      onKeyDown={handleKeyDown}
      className={cn(
        'inline-flex items-center gap-1 rounded-card border border-gray-alpha-400 bg-background-200 p-1',
        className,
      )}
    >
      {children}
    </div>
  );
}

export interface TabsTriggerProps {
  value: string;
  children: ReactNode;
  className?: string;
}

export function TabsTrigger({ value, children, className }: TabsTriggerProps) {
  const {
    value: active,
    onChange,
    registerTrigger,
    unregisterTrigger,
    setTriggerElement,
    getTriggerId,
    getPanelId,
  } = useTabs();
  const selected = active === value;
  const triggerId = getTriggerId(value);
  const panelId = getPanelId(value);

  useEffect(() => {
    registerTrigger(value);
    return () => unregisterTrigger(value);
  }, [value, registerTrigger, unregisterTrigger]);

  const handleRef = (el: HTMLButtonElement | null) => {
    setTriggerElement(value, el);
  };

  return (
    <button
      ref={handleRef}
      type="button"
      role="tab"
      id={triggerId}
      aria-selected={selected}
      aria-controls={panelId}
      tabIndex={selected ? 0 : -1}
      onClick={() => onChange(value)}
      className={cn(
        'h-8 rounded-control px-3 text-button-12 transition-colors duration-state ease-standard motion-reduce:transition-none focus-visible:outline-none',
        selected
          ? 'bg-background-100 text-gray-1000 shadow-card'
          : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
        className,
      )}
    >
      {children}
    </button>
  );
}

export interface TabsContentProps {
  value: string;
  children: ReactNode;
  className?: string;
  /** DOM ref forwarded to the underlying panel (React 19 ref-as-prop). */
  ref?: Ref<HTMLDivElement>;
}

export function TabsContent({ value, children, className, ref }: TabsContentProps) {
  const { value: active, getTriggerId, getPanelId } = useTabs();
  if (active !== value) return null;
  const triggerId = getTriggerId(value);
  const panelId = getPanelId(value);
  return (
    <div
      ref={ref}
      role="tabpanel"
      id={panelId}
      aria-labelledby={triggerId}
      className={className}
    >
      {children}
    </div>
  );
}
