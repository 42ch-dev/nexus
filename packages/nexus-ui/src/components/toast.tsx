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
    setToasts((prev) => {
      if (prev.length < MAX_TOASTS) return [...prev, next];
      const evictIndex = prev.findIndex((t) => t.duration !== 0);
      if (evictIndex === -1) {
        // All existing toasts are persistent; keep them and allow the queue
        // to temporarily exceed MAX_TOASTS rather than silently drop either
        // the new toast or a persistent one.
        return [...prev, next];
      }
      const nextQueue = [...prev];
      nextQueue.splice(evictIndex, 1);
      return [...nextQueue, next];
    });
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

const TOAST_EXIT_MS = 140;

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: (id: number) => void }) {
  const { variant, title, description, duration, id, testId } = toast;
  const styles = VARIANT_STYLES[variant];
  const [motion, setMotion] = useState<'enter' | 'shown' | 'exit'>('enter');
  const exitTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitingRef = useRef(false);

  useEffect(() => {
    const frame = requestAnimationFrame(() => setMotion('shown'));
    return () => cancelAnimationFrame(frame);
  }, []);

  const beginExit = useCallback(() => {
    if (exitingRef.current) return;
    exitingRef.current = true;
    setMotion('exit');
    exitTimerRef.current = setTimeout(() => onDismiss(id), TOAST_EXIT_MS);
  }, [id, onDismiss]);

  useEffect(() => {
    if (!duration || duration <= 0) return;
    const timer = setTimeout(() => beginExit(), duration);
    return () => clearTimeout(timer);
  }, [duration, beginExit]);

  useEffect(
    () => () => {
      if (exitTimerRef.current) clearTimeout(exitTimerRef.current);
    },
    [],
  );

  return (
    <div
      data-testid={testId}
      role={variant === 'error' ? 'alert' : 'status'}
      className={cn(
        'pointer-events-auto flex overflow-hidden rounded-popover border border-gray-alpha-400 bg-background-100 shadow-popover',
        'transition-[opacity,transform] motion-reduce:transition-none',
        motion === 'enter' && 'translate-y-2 opacity-0',
        motion === 'shown' && 'translate-y-0 opacity-100 duration-enter ease-standard',
        motion === 'exit' && 'translate-y-2 opacity-0 duration-exit ease-standard',
      )}
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
          onClick={beginExit}
          aria-label="Dismiss notification"
          className="shrink-0 rounded-control p-1 text-gray-700 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 hover:text-gray-1000"
        >
          <X className="h-3.5 w-3.5" aria-hidden />
        </button>
      </div>
    </div>
  );
}
