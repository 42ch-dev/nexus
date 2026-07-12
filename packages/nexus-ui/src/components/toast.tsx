/**
 * Toast system — DESIGN.md §Component Primitives/Toast.
 *
 * Minimal context-driven toast queue (no external dependency): show +
 * auto-dismiss, manual dismiss, and a `<Toaster />` portal that renders the
 * queue. Variants map to the DESIGN.md semantic accents on the leading bar +
 * icon.
 *
 * Promoted to `@42ch/nexus-ui` for V1.106 Studio Surfaces fixtures. Pure
 * presentational: no daemon state, no routing, no app providers.
 */
import { AlertCircle, AlertTriangle, CheckCircle, Info, X } from 'lucide-react';
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
import { createPortal } from 'react-dom';

import { cn } from '../lib/cn';

export type ToastVariant = 'success' | 'error' | 'warning' | 'info';

export interface Toast {
  id: number;
  variant: ToastVariant;
  title: string;
  description?: string;
  /** Auto-dismiss delay (ms). 0 keeps the toast until dismissed. */
  duration?: number;
  /** Optional test id for the rendered toast item (used by fixtures). */
  testId?: string;
}

interface ToastContextValue {
  toasts: Toast[];
  toast: (toast: Omit<Toast, 'id'>) => number;
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

const DEFAULT_DURATION = 6_000;
const MAX_TOASTS = 5;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const toast = useCallback((input: Omit<Toast, 'id'>): number => {
    const id = nextId.current++;
    const next: Toast = { id, duration: DEFAULT_DURATION, ...input };
    setToasts((prev) => [...prev.slice(-(MAX_TOASTS - 1)), next]);
    return id;
  }, []);

  const value = useMemo<ToastContextValue>(() => ({ toasts, toast, dismiss }), [
    toasts,
    toast,
    dismiss,
  ]);

  return <ToastContext.Provider value={value}>{children}</ToastContext.Provider>;
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used within a ToastProvider');
  return ctx;
}

const VARIANT_STYLES: Record<ToastVariant, { bar: string; icon: ReactNode }> = {
  success: {
    bar: 'bg-green-700',
    icon: <CheckCircle className="h-4 w-4 text-green-700" aria-hidden />,
  },
  error: {
    bar: 'bg-red-700',
    icon: <AlertCircle className="h-4 w-4 text-red-700" aria-hidden />,
  },
  warning: {
    bar: 'bg-amber-700',
    icon: <AlertTriangle className="h-4 w-4 text-amber-700" aria-hidden />,
  },
  info: {
    bar: 'bg-blue-700',
    icon: <Info className="h-4 w-4 text-blue-700" aria-hidden />,
  },
};

/**
 * Toast viewport — portal in the bottom-right. Each toast auto-dismisses after
 * its duration (default 6s) unless `duration: 0`. Renders nothing on the server
 * / non-DOM environments.
 */
export function Toaster() {
  const { toasts, dismiss } = useToast();
  const handleDismiss = useCallback((id: number) => dismiss(id), [dismiss]);
  if (typeof document === 'undefined') return null;

  return createPortal(
    <div
      aria-live="polite"
      aria-atomic="false"
      className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-full max-w-[360px] flex-col gap-2"
    >
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} onDismiss={handleDismiss} />
      ))}
    </div>,
    document.body,
  );
}

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: (id: number) => void }) {
  const { variant, title, description, duration, id, testId } = toast;
  const styles = VARIANT_STYLES[variant];

  useEffect(() => {
    if (!duration || duration <= 0) return;
    const timer = setTimeout(() => onDismiss(id), duration);
    return () => clearTimeout(timer);
  }, [duration, id, onDismiss]);

  return (
    <div
      data-testid={testId}
      role={variant === 'error' ? 'alert' : 'status'}
      className="pointer-events-auto flex overflow-hidden rounded-popover border border-gray-alpha-400 bg-background-100 shadow-popover"
    >
      <span aria-hidden className={cn('w-1 shrink-0', styles.bar)} />
      <div className="flex flex-1 items-start gap-2 p-4">
        <span className="mt-0.5 shrink-0">{styles.icon}</span>
        <div className="min-w-0 flex-1">
          <p className="text-label-14 font-medium text-gray-1000">{title}</p>
          {description && <p className="mt-1 text-copy-13 text-gray-900">{description}</p>}
        </div>
        <button
          type="button"
          onClick={() => onDismiss(id)}
          aria-label="Dismiss notification"
          className="shrink-0 rounded-control p-1 text-gray-700 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 hover:text-gray-1000"
        >
          <X className="h-3.5 w-3.5" aria-hidden />
        </button>
      </div>
    </div>
  );
}
