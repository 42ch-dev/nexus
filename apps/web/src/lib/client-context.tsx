import { createContext, useContext, useMemo, type ReactNode } from 'react';

import { BrowserClient, type NexusClient } from '@/lib/nexus';
import { TauriClient } from '@/lib/nexus/tauri-client';
import { TauriDesktopCapabilities, type DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { isDesktopBuild } from '@/lib/nexus/detect';

/**
 * Provides the active {@link NexusClient} (and, in desktop mode, a
 * {@link DesktopCapabilities} object) to the app.
 *
 * Capability detection runs **once** here, at the factory (compass §5 #7 LOCKED)
 * — not scattered across screens. Browser build selects {@link BrowserClient}
 * with `desktop = null`; the desktop webview selects {@link TauriClient}
 * (thin-over-`BrowserClient`, same HTTP transport) plus a
 * {@link TauriDesktopCapabilities} for the native actions.
 *
 * Tests may inject an explicit `client` (and `desktop`) to bypass detection.
 */
const ClientContext = createContext<NexusClient | null>(null);
const DesktopContext = createContext<DesktopCapabilities | null>(null);

export interface ClientProviderProps {
  /** Override the NexusClient (tests). If omitted, the factory selects. */
  client?: NexusClient;
  /** Override desktop capabilities (tests). `null` hides desktop affordances. */
  desktop?: DesktopCapabilities | null;
  children: ReactNode;
}

interface ResolvedClients {
  client: NexusClient;
  desktop: DesktopCapabilities | null;
}

/**
 * Select clients once, at module/factory scope. Extracted so the provider is a
 * thin wrapper and detection is trivially testable without React.
 */
export function selectClients(): ResolvedClients {
  if (!isDesktopBuild()) {
    return { client: new BrowserClient(), desktop: null };
  }
  const tauri = new TauriClient();
  const desktop = new TauriDesktopCapabilities();
  return { client: tauri, desktop };
}

export function ClientProvider({ client, desktop, children }: ClientProviderProps) {
  const value = useMemo<ResolvedClients>(() => {
    if (client) return { client, desktop: desktop ?? null };
    return selectClients();
  }, [client, desktop]);
  return (
    <ClientContext.Provider value={value.client}>
      <DesktopContext.Provider value={value.desktop}>
        {children}
      </DesktopContext.Provider>
    </ClientContext.Provider>
  );
}

export function useNexusClient(): NexusClient {
  const client = useContext(ClientContext);
  if (!client) throw new Error('useNexusClient must be used within a ClientProvider');
  return client;
}

/**
 * Desktop-only capabilities, or `null` in the browser build. Screens branch on
 * the `null` return to hide native affordances (e.g. Open With / Reveal in
 * Finder); Copy Path stays unconditional because it is plain clipboard write.
 */
export function useDesktopCapabilities(): DesktopCapabilities | null {
  return useContext(DesktopContext);
}
