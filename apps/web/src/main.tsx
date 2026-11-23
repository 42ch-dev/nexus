import { StrictMode, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { BrowserRouter } from 'react-router';
import { useTranslation } from 'react-i18next';

import { App } from '@/App';
import { ClientProvider } from '@/lib/client-context';
import { AuthRecoveryGate } from '@/components/access/auth-recovery';
import { ThemeProvider } from '@/components/theme-provider';
import { LocaleProvider } from '@/components/locale-provider';
import { ToastProvider, Toaster, useToast } from '@/lib/use-toast';
import { NexusClientError } from '@/lib/nexus';
import './index.css';

function useQueryErrorToast() {
  const { toast } = useToast();
  const { t } = useTranslation('common');
  return (error: unknown) => {
    // toasting it per query is the I-000059 toast storm.
    if (error instanceof NexusClientError && error.status === 401) return;
    const description =
      error instanceof NexusClientError
        ? error.message
        : error instanceof Error
          ? error.message
          : t('error.unexpected');
    toast({ variant: 'error', title: t('error.requestFailed'), description });
  };
}

function AppProviders({ children }: { children: React.ReactNode }) {
  const onError = useQueryErrorToast();

  const queryErrorHandlerRef = useRef(onError);
  useEffect(() => {
    queryErrorHandlerRef.current = onError;
  });

  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: 1,
            refetchOnWindowFocus: false,
            staleTime: 15_000,
          },
        },
        queryCache: new QueryCache({
          onError: (error) => queryErrorHandlerRef.current(error),
        }),
      }),
  );

  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <ClientProvider>
          <AuthRecoveryGate>{children}</AuthRecoveryGate>
        </ClientProvider>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Root element #root not found');

createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider>
      <LocaleProvider>
        <ToastProvider>
          <AppProviders>
            <App />
          </AppProviders>
          <Toaster />
        </ToastProvider>
      </LocaleProvider>
    </ThemeProvider>
  </StrictMode>,
);
