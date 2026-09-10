import { useState } from 'react';

import {
  Button,
  ToastProvider,
  Toaster,
  useToast,
  type ToastVariant,
} from '@42ch/nexus-ui';

const VARIANTS: ToastVariant[] = ['success', 'error', 'warning', 'info'];

const TOAST_PRESETS: Record<
  ToastVariant,
  { title: string; description?: string }
> = {
  success: {
    title: 'Profile saved',
    description: 'Your workspace profile is ready.',
  },
  error: {
    title: 'Could not save profile',
    description: 'Check your connection and try again.',
  },
  warning: {
    title: 'Workspace path changed',
    description: 'Reload the app so the daemon uses the new path.',
  },
  info: {
    title: 'Update available',
  },
};

/**
 * Studio Components fixture for the live Toast variant matrix.
 *
 * Uses promoted `@42ch/nexus-ui` Toast primitives (`ToastProvider`, `useToast`,
 * `Toaster`). Controls queue toasts through the public API — no auto-mounted
 * persistent notifications that obscure unrelated gallery specimens.
 */
function ToastControls() {
  const { toast, dismiss, toasts } = useToast();
  const [lastId, setLastId] = useState<number | null>(null);

  return (
    <div className="flex flex-col gap-4">
      <p className="text-copy-13 text-gray-700">
        Show a variant, then dismiss via the toast close control or{' '}
        <strong>Dismiss last</strong>. Persistent toasts use{' '}
        <code className="text-copy-13-mono">duration: 0</code> and stay until dismissed.
      </p>
      <div className="flex flex-wrap gap-2">
        {VARIANTS.map((variant) => (
          <Button
            key={variant}
            variant="secondary"
            size="small"
            data-testid={`toast-show-${variant}`}
            onClick={() => {
              const id = toast({
                variant,
                ...TOAST_PRESETS[variant],
                testId: `toast-variant-${variant}`,
              });
              setLastId(id);
            }}
          >
            Show {variant}
          </Button>
        ))}
        <Button
          variant="tertiary"
          size="small"
          data-testid="toast-show-persistent"
          onClick={() => {
            const id = toast({
              variant: 'info',
              title: 'Persistent notice',
              description: 'Stays until dismissed (duration: 0).',
              duration: 0,
              testId: 'toast-persistent',
            });
            setLastId(id);
          }}
        >
          Show persistent
        </Button>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="tertiary"
          size="small"
          disabled={lastId === null}
          data-testid="toast-dismiss-last"
          onClick={() => {
            if (lastId !== null) {
              dismiss(lastId);
              setLastId(null);
            }
          }}
        >
          Dismiss last
        </Button>
        <span className="text-copy-13 text-gray-700" data-testid="toast-queue-count">
          Queue: {toasts.length} {toasts.length === 1 ? 'toast' : 'toasts'}
        </span>
      </div>
    </div>
  );
}

export function ToastFixtures() {
  return (
    <div data-testid="toast-matrix">
      <ToastProvider>
        <ToastControls />
        <Toaster />
      </ToastProvider>
    </div>
  );
}
