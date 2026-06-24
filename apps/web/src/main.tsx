import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { BrowserRouter } from 'react-router-dom';

import { App } from '@/App';
import { ClientProvider } from '@/lib/client-context';
import { ThemeProvider } from '@/components/theme-provider';
import { ToastProvider, Toaster, useToast } from '@/lib/use-toast';
import { NexusClientError } from '@/lib/nexus';
import './index.css';

/**
 * Query-error toast bridge: any query that fails (and is not handled locally)
 * surfaces a single toast parsed from the shared ErrorResponse. Mutations own
 * their own error callbacks (they invalidate caches + name the changed object),
 * so this default only catches read-path failures.
 */
function useQueryErrorToast() {
  const { toast } = useToast();
  return (error: unknown) => {
    const description =
      error instanceof NexusClientError
        ? error.message
        : error instanceof Error
          ? error.message
          : 'Unexpected error.';
    toast({ variant: 'error', title: 'Request failed', description });
  };
}

function AppProviders({ children }: { children: React.ReactNode }) {
  const onError = useQueryErrorToast();
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        // Local loopback daemon; keep refetch conservative and avoid noisy
        // retries on a daemon that may legitimately be down during setup.
        retry: 1,
        refetchOnWindowFocus: false,
        staleTime: 15_000,
      },
    },
    queryCache: new QueryCache({ onError }),
  });

  return (
    <QueryClientProvider client={queryClient}>
      <ClientProvider>
        <BrowserRouter>{children}</BrowserRouter>
      </ClientProvider>
    </QueryClientProvider>
  );
}

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Root element #root not found');

createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider>
      <ToastProvider>
        <AppProviders>
          <App />
        </AppProviders>
        <Toaster />
      </ToastProvider>
    </ThemeProvider>
  </StrictMode>,
);
