import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { BrowserRouter } from 'react-router-dom';

import { App } from '@/App';
import { ClientProvider } from '@/lib/client-context';
import { ThemeProvider } from '@/components/theme-provider';
import './index.css';

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
});

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Root element #root not found');

createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider>
      <ClientProvider>
        <QueryClientProvider client={queryClient}>
          <BrowserRouter>
            <App />
          </BrowserRouter>
        </QueryClientProvider>
      </ClientProvider>
    </ThemeProvider>
  </StrictMode>,
);
