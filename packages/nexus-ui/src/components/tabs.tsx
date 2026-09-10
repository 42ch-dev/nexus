import {
  createContext,
  useCallback,
  useContext,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';

import { cn } from '../lib/cn';

interface TabsContextValue {
  value: string;
  onChange: (value: string) => void;
  baseId: string;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabs() {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error('Tabs components must be used inside <Tabs />');
  return ctx;
}

// Escape the escape marker too, keeping arbitrary string values distinct and IDREF-safe.
function tabValueId(value: string) {
  return value.replace(/[^a-zA-Z0-9]/gu, (character) => `_${character.codePointAt(0)!.toString(16)}_`);
}

function listTabs(list: HTMLDivElement) {
  const tabs: HTMLButtonElement[] = [];
  for (const tab of list.querySelectorAll<HTMLButtonElement>('button[role="tab"][data-tabs-value]')) {
    if (tab.closest('[role="tablist"]') === list) tabs.push(tab);
  }
  return tabs;
}

function updateTabStops(list: HTMLDivElement) {
  const tabs = listTabs(list);
  const entry = tabs.find((tab) => tab.getAttribute('aria-selected') === 'true') ?? tabs[0];
  for (const tab of tabs) {
    const tabIndex = tab === entry ? 0 : -1;
    if (tab.tabIndex !== tabIndex) tab.tabIndex = tabIndex;
  }
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
  const onChange = useCallback((next: string) => {
    if (!controlled) setInternal(next);
    onValueChange?.(next);
  }, [controlled, onValueChange]);
  const baseId = useId();
  const ctx = useMemo(() => ({ value: active, onChange, baseId }), [active, onChange, baseId]);
  return (
    <TabsContext.Provider value={ctx}>
      <div className={cn('flex flex-col gap-4', className)}>{children}</div>
    </TabsContext.Provider>
  );
}

export function TabsList({ children, className }: { children: ReactNode; className?: string }) {
  const { onChange } = useTabs();
  const listRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    updateTabStops(listRef.current!);
  });

  useLayoutEffect(() => {
    const list = listRef.current!;
    const sync = () => updateTabStops(list);
    // Descendants can reorder independently of this component. Observe only membership
    // and selection, not the tabindex writes, to keep the fallback entry current.
    const observer = new MutationObserver(sync);
    observer.observe(list, { childList: true, subtree: true, attributes: true, attributeFilter: ['aria-selected'] });
    return () => observer.disconnect();
  }, []);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight' && event.key !== 'Home' && event.key !== 'End') return;
    if ((event.target as Element).closest('[role="tablist"]') !== event.currentTarget) return;
    const tabs = listTabs(event.currentTarget);
    if (!tabs.length) return;
    let index = tabs.indexOf(event.currentTarget.ownerDocument.activeElement as HTMLButtonElement);
    if (index < 0) index = Math.max(0, tabs.findIndex((tab) => tab.tabIndex === 0));
    const nextIndex = event.key === 'Home' ? 0
      : event.key === 'End' ? tabs.length - 1
      : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    const next = tabs[nextIndex];
    event.preventDefault();
    next.focus();
    onChange(next.dataset.tabsValue!);
  };

  return (
    <div
      ref={listRef}
      role="tablist"
      onKeyDown={handleKeyDown}
      className={cn('inline-flex items-center gap-1 rounded-card border border-gray-alpha-400 bg-background-200 p-1', className)}
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
  const { value: active, onChange, baseId } = useTabs();
  const selected = active === value;
  const id = `${baseId}-${tabValueId(value)}`;
  return (
    <button
      type="button"
      role="tab"
      id={`${id}-trigger`}
      data-tabs-value={value}
      aria-selected={selected}
      aria-controls={selected ? `${id}-panel` : undefined}
      tabIndex={selected ? 0 : -1}
      onClick={() => onChange(value)}
      className={cn(
        'h-8 rounded-control px-3 text-button-12 transition-colors duration-state ease-standard motion-reduce:transition-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 focus-visible:ring-offset-background-100',
        selected ? 'bg-background-100 text-gray-1000 shadow-card' : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
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
}

export function TabsContent({ value, children, className }: TabsContentProps) {
  const { value: active, baseId } = useTabs();
  if (active !== value) return null;
  const id = `${baseId}-${tabValueId(value)}`;
  return <div role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-trigger`} className={className}>{children}</div>;
}
