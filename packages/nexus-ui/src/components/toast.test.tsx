import { act, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
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
    const getApi = renderToaster();
    const { toast } = getApi();
    act(() => toast({ variant: 'info', title: 'Dismiss me' }));
    expect(screen.getByText('Dismiss me')).toBeInTheDocument();
    act(() => {
      screen.getByRole('button', { name: 'Dismiss notification' }).click();
    });
    expect(screen.queryByText('Dismiss me')).not.toBeInTheDocument();
  });
});
