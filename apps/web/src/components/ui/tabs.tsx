import { createContext, useContext, useMemo, useState, type ReactNode } from 'react';

import { cn } from '@/lib/utils';

interface TabsContextValue {
  value: string;
  onChange: (value: string) => void;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabs() {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error('Tabs components must be used inside <Tabs />');
  return ctx;
}

interface TabsProps {
  value: string;
  onValueChange?: (value: string) => void;
  defaultValue?: string;
  children: ReactNode;
  className?: string;
}

export function Tabs({ value, onValueChange, defaultValue, children, className }: TabsProps) {
  const [internal, setInternal] = useState(defaultValue ?? '');
  const controlled = value !== undefined;
  const active = controlled ? value : internal;
  const onChange = (v: string) => {
    if (!controlled) setInternal(v);
    onValueChange?.(v);
  };
  const ctx = useMemo(() => ({ value: active, onChange }), [active, onChange]);
  return (
    <TabsContext.Provider value={ctx}>
      <div className={cn('flex flex-col gap-4', className)}>{children}</div>
    </TabsContext.Provider>
  );
}

export function TabsList({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      role="tablist"
      className={cn('inline-flex items-center gap-1 rounded-card border border-gray-alpha-400 bg-background-200 p-1', className)}
    >
      {children}
    </div>
  );
}

interface TabsTriggerProps {
  value: string;
  children: ReactNode;
  className?: string;
}

export function TabsTrigger({ value, children, className }: TabsTriggerProps) {
  const { value: active, onChange } = useTabs();
  const selected = active === value;
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected}
      onClick={() => onChange(value)}
      className={cn(
        'h-8 rounded-control px-3 text-button-12 transition-colors duration-state ease-standard',
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

interface TabsContentProps {
  value: string;
  children: ReactNode;
  className?: string;
}

export function TabsContent({ value, children, className }: TabsContentProps) {
  const { value: active } = useTabs();
  if (active !== value) return null;
  return (
    <div role="tabpanel" className={className}>
      {children}
    </div>
  );
}
