import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi, afterEach } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { ToastProvider, Toaster, useToast } from './toast';

describe('ToastProvider', () => {
  it('renders children without a Toaster', () => {
    render(
      <ToastProvider>
        <div data-testid="child">child</div>
      </ToastProvider>,
    );
    expect(screen.getByTestId('child')).toBeInTheDocument();
  });

  it('throws when useToast is called outside provider', () => {
    function Orphan() {
      useToast();
      return null;
    }
    expect(() => render(<Orphan />)).toThrow(/useToast must be used within a ToastProvider/);
  });
});

describe('Toaster + useToast', () => {
  function ToastControls({ onReady }: { onReady: (api: ReturnType<typeof useToast>) => void }) {
    const api = useToast();
    onReady(api);
    return null;
  }

  function renderToaster() {
    let api: ReturnType<typeof useToast> | null = null;
    render(
      <ToastProvider>
        <ToastControls onReady={(a) => (api = a)} />
        <Toaster />
      </ToastProvider>,
    );
    return () => api!;
  }

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders a toast with title and description', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => toast({ variant: 'info', title: 'Sync finished', description: '12 rows' }));
    expect(screen.getByText('Sync finished')).toBeInTheDocument();
    expect(screen.getByText('12 rows')).toBeInTheDocument();
  });

  it('renders all four variant accents', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => {
      toast({ variant: 'success', title: 'Saved' });
      toast({ variant: 'error', title: 'Failed' });
      toast({ variant: 'warning', title: 'Caution' });
      toast({ variant: 'info', title: 'Note' });
    });
    expect(screen.getByText('Saved')).toBeInTheDocument();
    expect(screen.getByText('Failed')).toBeInTheDocument();
    expect(screen.getByText('Caution')).toBeInTheDocument();
    expect(screen.getByText('Note')).toBeInTheDocument();
  });

  it('applies data-testid when provided', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => toast({ variant: 'success', title: 'Saved', testId: 'toast-variant-success' }));
    expect(screen.getByTestId('toast-variant-success')).toHaveTextContent('Saved');
  });

  it('renders the popover-level v0.4 surface (elevation-3 via shadow-popover alias)', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => toast({ variant: 'info', title: 'Surfaced', testId: 'toast-surface' }));
    const el = screen.getByTestId('toast-surface');
    // DESIGN.md components.toast + §Elevation alias chain:
    // shadow-popover resolves onto --shadow-elevation-3 (popover-level surface).
    expect(el).toHaveClass('shadow-popover');
    expect(el).toHaveClass('rounded-popover');
    expect(el).toHaveClass('border-gray-alpha-400');
    expect(el).toHaveClass('bg-background-100');
  });

  it('gives error toasts role=alert and others role=status', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => {
      toast({ variant: 'error', title: 'Err', testId: 'toast-error' });
      toast({ variant: 'info', title: 'Info', testId: 'toast-info' });
    });
    expect(screen.getByTestId('toast-error')).toHaveAttribute('role', 'alert');
    expect(screen.getByTestId('toast-info')).toHaveAttribute('role', 'status');
  });

  it('dismisses a toast when the dismiss button is clicked', () => {
    vi.useFakeTimers();
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => toast({ variant: 'info', title: 'Dismiss me' }));
    expect(screen.getByText('Dismiss me')).toBeInTheDocument();
    act(() => {
      screen.getByRole('button', { name: 'Dismiss notification' }).click();
    });
    act(() => {
      vi.advanceTimersByTime(140);
    });
    expect(screen.queryByText('Dismiss me')).not.toBeInTheDocument();
  });

  it('applies DESIGN.md enter/exit motion tokens on the toast surface', () => {
    const rafSpy = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      cb(0);
      return 1;
    });
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => toast({ variant: 'info', title: 'Motion', testId: 'toast-motion', duration: 0 }));
    rafSpy.mockRestore();
    const el = screen.getByTestId('toast-motion');
    expect(el).toHaveClass('duration-enter');
    expect(el).toHaveClass('ease-standard');
    expect(el).toHaveClass('translate-y-0');
    expect(el).toHaveClass('opacity-100');
  });

  it('caps the queue at MAX_TOASTS and keeps the newest items', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => {
      toast({ variant: 'info', title: 'One', testId: 'toast-1' });
      toast({ variant: 'info', title: 'Two', testId: 'toast-2' });
      toast({ variant: 'info', title: 'Three', testId: 'toast-3' });
      toast({ variant: 'info', title: 'Four', testId: 'toast-4' });
      toast({ variant: 'info', title: 'Five', testId: 'toast-5' });
      toast({ variant: 'info', title: 'Six', testId: 'toast-6' });
    });
    expect(screen.queryByTestId('toast-1')).not.toBeInTheDocument();
    expect(screen.getByTestId('toast-2')).toBeInTheDocument();
    expect(screen.getByTestId('toast-3')).toBeInTheDocument();
    expect(screen.getByTestId('toast-4')).toBeInTheDocument();
    expect(screen.getByTestId('toast-5')).toBeInTheDocument();
    expect(screen.getByTestId('toast-6')).toBeInTheDocument();
  });

  it('does not evict persistent toasts when trimming the queue', () => {
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => {
      toast({ variant: 'info', title: 'One', duration: 0, testId: 'toast-1' });
      toast({ variant: 'info', title: 'Two', duration: 0, testId: 'toast-2' });
      toast({ variant: 'info', title: 'Three', duration: 0, testId: 'toast-3' });
      toast({ variant: 'info', title: 'Four', duration: 0, testId: 'toast-4' });
      toast({ variant: 'info', title: 'Five', duration: 0, testId: 'toast-5' });
      toast({ variant: 'info', title: 'Six', testId: 'toast-6' });
    });
    expect(screen.getByTestId('toast-1')).toBeInTheDocument();
    expect(screen.getByTestId('toast-2')).toBeInTheDocument();
    expect(screen.getByTestId('toast-3')).toBeInTheDocument();
    expect(screen.getByTestId('toast-4')).toBeInTheDocument();
    expect(screen.getByTestId('toast-5')).toBeInTheDocument();
    expect(screen.getByTestId('toast-6')).toBeInTheDocument();
  });

  it('does not reset the auto-dismiss timer when a second toast is added', () => {
    vi.useFakeTimers();
    const getApi = renderToaster();
    const { toast } = getApi();

    act(() =>
      toast({ variant: 'info', title: 'First', duration: 3000, testId: 'toast-a' }),
    );
    act(() => vi.advanceTimersByTime(2000));

    act(() =>
      toast({ variant: 'info', title: 'Second', duration: 3000, testId: 'toast-b' }),
    );
    act(() => vi.advanceTimersByTime(1500));

    expect(screen.queryByTestId('toast-a')).not.toBeInTheDocument();
    expect(screen.getByTestId('toast-b')).toBeInTheDocument();
  });
});
