/**
 * Test wrapper that mounts the React providers screens depend on.
 *
 * Mirrors src/main.tsx's provider stack (minus ThemeProvider localStorage side
 * effects, which screens do not exercise). Use `renderInApp` in component tests
 * so hooks relying on `QueryClientProvider` + `ClientProvider` resolve.
 */
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, type RenderOptions } from '@testing-library/react';
import type { ReactElement, ReactNode } from 'react';
import { MemoryRouter } from 'react-router';

import { ClientProvider } from '@/lib/client-context';
import { ActiveCreatorProvider } from '@/lib/active-creator-context';
import { SetupCompletedProvider } from '@/lib/setup-completed-context';
import { EntranceProvider } from '@/lib/entrance-context';
import { CreatorEntitySelectionProvider } from '@/components/layout/creator-entity-selection-context';
import { LocaleProvider } from '@/components/locale-provider';
import type { NexusClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { ToastProvider, Toaster } from '@/lib/use-toast';

/**
 * Build a fresh QueryClient per test. Defaults are overridden so retries do not
 * re-fire handlers unexpectedly and errors surface immediately (no retry delay).
 */
export function makeQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: 0, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
}

/** A no-op client for tests that do not exercise transport; methods reject. */
export const noopClient = {
  health: () => Promise.reject(new Error('noopClient: no transport wired')),
} as unknown as NexusClient;

interface RenderInAppOptions extends Omit<RenderOptions, 'wrapper'> {
  client?: NexusClient;
  /**
   * Desktop capabilities to inject (V1.66 desktop-mode tests). Omit (or `null`)
   * to simulate the browser build — `useDesktopCapabilities()` returns `null`
   * and native affordances hide.
   */
  desktop?: DesktopCapabilities | null;
  queryClient?: QueryClient;
  initialRouterEntries?: string[];
  /** Initial active creator id for the footer profile switcher context. */
  activeCreatorId?: string | null;
  /** Initial setup-completed value for the setup gate context. */
  setupCompleted?: boolean;
}

export function renderInApp(
  ui: ReactElement,
  {
    client,
    desktop,
    queryClient,
    initialRouterEntries = ['/'],
    activeCreatorId,
    setupCompleted,
    ...rest
  }: RenderInAppOptions = {},
) {
  const qc = queryClient ?? makeQueryClient();
  const activeClient = client ?? noopClient;

  function Wrapper({ children }: { children: ReactNode }): ReactElement {
    return (
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={initialRouterEntries}>
          <ClientProvider client={activeClient} desktop={desktop ?? null} connectionConfig={null}>
            <ActiveCreatorProvider initialCreatorId={activeCreatorId}>
              <SetupCompletedProvider initialCompleted={setupCompleted}>
                <EntranceProvider>
                  <CreatorEntitySelectionProvider>
                    <LocaleProvider>
                      <ToastProvider>
                        {children}
                        <Toaster />
                      </ToastProvider>
                    </LocaleProvider>
                  </CreatorEntitySelectionProvider>
                </EntranceProvider>
              </SetupCompletedProvider>
            </ActiveCreatorProvider>
          </ClientProvider>
        </MemoryRouter>
      </QueryClientProvider>
    );
  }

  return render(ui, { wrapper: Wrapper, ...rest });
}

export { render };
