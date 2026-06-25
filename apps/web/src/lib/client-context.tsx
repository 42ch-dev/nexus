import { createContext, useContext, useMemo, type ReactNode } from 'react';

import { BrowserClient, type NexusClient } from '@/lib/nexus';

/**
 * Provides the active {@link NexusClient} to the app. The browser build uses
 * {@link BrowserClient} (same-origin `/v1/local/*`). The V1.65 desktop shell
 * will swap in `TauriClient` here — no screen code changes (web-ui.md §5).
 */
const ClientContext = createContext<NexusClient | null>(null);

export function ClientProvider({
  client,
  children,
}: {
  client?: NexusClient;
  children: ReactNode;
}) {
  const value = useMemo(() => client ?? new BrowserClient(), [client]);
  return <ClientContext.Provider value={value}>{children}</ClientContext.Provider>;
}

export function useNexusClient(): NexusClient {
  const client = useContext(ClientContext);
  if (!client) throw new Error('useNexusClient must be used within a ClientProvider');
  return client;
}
