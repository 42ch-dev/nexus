/**
 * Toast / notification hook coverage (R-V164-QC1-S1-P1 T3).
 *
 * The notification surface is the second half of the W-1 fix: the daemon's
 * `ErrorResponse` envelope is unwrapped by `NexusClientError.fromBody` (covered
 * in errors.test.ts) and the resulting `.message` is pushed into a toast by the
 * query/mutation error bridges (queries.ts `useErrorToast` + main.tsx
 * `useQueryErrorToast`). These tests pin the notification hook itself and the
 * error→toast architectural path: a `NexusClientError` parsed from a canonical
 * F-E1 envelope surfaces its stable `message` as the toast description.
 */
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';

import { NexusClientError } from '@/lib/nexus';
import { ToastProvider, Toaster, useToast } from '@/lib/use-toast';

/** Drive the toast API from inside the provider tree. */
function ToastControls({
  onReady,
}: {
  onReady: (api: ReturnType<typeof useToast>) => void;
}) {
  const api = useToast();
  return <>{(onReady(api), null)}</>;
}

function renderToaster(children?: ReactNode) {
  return render(
    <ToastProvider>
      {children}
      <Toaster />
    </ToastProvider>,
  );
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('ToastProvider mount', () => {
  it('renders children', () => {
    render(
      <ToastProvider>
        <div>toast-host-child</div>
      </ToastProvider>,
    );
    expect(screen.getByText('toast-host-child')).toBeInTheDocument();
  });
});

describe('useToast outside provider', () => {
  it('throws when used without a ToastProvider', () => {
    function Orphan() {
      useToast();
      return null;
    }
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Orphan />)).toThrow(/useToast must be used within a ToastProvider/);
    spy.mockRestore();
  });
});

describe('toast queue + Toaster rendering', () => {
  it('renders a queued toast with its title and description', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);

    act(() => toast({ variant: 'info', title: 'Sync finished', description: '12 rows' }));
    expect(screen.getByText('Sync finished')).toBeInTheDocument();
    expect(screen.getByText('12 rows')).toBeInTheDocument();
  });

  it('supports multiple queued toasts at once', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);

    act(() => {
      toast({ variant: 'info', title: 'First' });
      toast({ variant: 'info', title: 'Second' });
    });
    expect(screen.getByText('First')).toBeInTheDocument();
    expect(screen.getByText('Second')).toBeInTheDocument();
  });

  it('dismiss() removes only the targeted toast', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    let dismiss!: ReturnType<typeof useToast>['dismiss'];
    let firstId = -1;
    renderToaster(
      <ToastControls
        onReady={(api) => {
          toast = api.toast;
          dismiss = api.dismiss;
        }}
      />,
    );

    act(() => {
      firstId = toast({ variant: 'info', title: 'Keep' });
      toast({ variant: 'info', title: 'Drop' });
    });

    act(() => dismiss(firstId));
    expect(screen.queryByText('Keep')).not.toBeInTheDocument();
    expect(screen.getByText('Drop')).toBeInTheDocument();
  });

  it('the Dismiss button removes the toast', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);
    act(() => toast({ variant: 'info', title: 'Click me away' }));

    const button = screen.getByRole('button', { name: 'Dismiss notification' });
    act(() => button.click());
    expect(screen.queryByText('Click me away')).not.toBeInTheDocument();
  });
});

describe('toast auto-dismiss', () => {
  it('auto-dismisses after the duration elapses', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);

    act(() => toast({ variant: 'info', title: 'Ephemeral', duration: 5_000 }));
    expect(screen.getByText('Ephemeral')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(screen.queryByText('Ephemeral')).not.toBeInTheDocument();
  });

  it('duration: 0 keeps the toast until manually dismissed', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    let dismiss!: ReturnType<typeof useToast>['dismiss'];
    let id = -1;
    renderToaster(
      <ToastControls
        onReady={(api) => {
          toast = api.toast;
          dismiss = api.dismiss;
        }}
      />,
    );

    act(() => {
      id = toast({ variant: 'error', title: 'Sticky', duration: 0 });
    });
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.getByText('Sticky')).toBeInTheDocument();

    act(() => dismiss(id));
    expect(screen.queryByText('Sticky')).not.toBeInTheDocument();
  });
});

describe('toast a11y semantics', () => {
  it('renders an error toast with role=alert', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);
    act(() => toast({ variant: 'error', title: 'Boom' }));
    expect(screen.getByRole('alert')).toBeInTheDocument();
  });

  it('renders a non-error toast with role=status', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);
    act(() => toast({ variant: 'success', title: 'Done' }));
    expect(screen.getByRole('status')).toBeInTheDocument();
  });
});

describe('error-envelope → toast path (W-1 end-to-end)', () => {
  it('surfaces the parsed ErrorResponse message as the toast description', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);

    // The exact shape the daemon emits and BrowserClient unwraps (F-E1).
    const envelope = {
      success: false,
      error: { code: 'validation_failed', message: 'Title is required.' },
    };
    const error = NexusClientError.fromBody(400, envelope);

    // Mirror the queries.ts useErrorToast bridge: NexusClientError.message
    // becomes the description so the actionable text reaches the user.
    act(() =>
      toast({
        variant: 'error',
        title: 'Could not create Work',
        description: error instanceof NexusClientError ? error.message : 'Unexpected error.',
      }),
    );

    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getByText('Could not create Work')).toBeInTheDocument();
    expect(screen.getByText('Title is required.')).toBeInTheDocument();
  });

  it('keeps the transport_unreachable message actionable in a toast', () => {
    let toast!: ReturnType<typeof useToast>['toast'];
    renderToaster(<ToastControls onReady={(api) => (toast = api.toast)} />);

    const error = NexusClientError.fromBody(0, {
      success: false,
      error: { code: 'transport_unreachable', message: 'Cannot reach the local daemon.' },
    });
    act(() =>
      toast({
        variant: 'error',
        title: 'Request failed',
        description: error.message,
      }),
    );

    expect(screen.getByText('Cannot reach the local daemon.')).toBeInTheDocument();
  });
});
