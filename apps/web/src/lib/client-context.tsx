import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';

import { BrowserClient, type NexusClient } from '@/lib/nexus';
import { TauriClient } from '@/lib/nexus/tauri-client';
import { TauriDesktopCapabilities, type DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { isDesktopBuild } from '@/lib/nexus/detect';
import {
  createConnectionStorage,
  type ConnectionConfig,
} from '@/lib/nexus/connection-storage';

/**
 * Provides the active {@link NexusClient} (and, in desktop mode, a
 * {@link DesktopCapabilities} object) to the app. Since V1.92 P1 the provider
 * is stateful: it loads the saved {@link ConnectionConfig} from platform
 * storage and reconstructs the client when the config changes. Local
 * same-origin mode remains the default when no remote config is active.
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
const ConnectionConfigContext = createContext<ConnectionConfig | null>(null);
const SetConnectionConfigContext = createContext<
  ((config: ConnectionConfig | null) => Promise<void>) | null
>(null);

export interface ClientProviderProps {
  /** Override the NexusClient (tests). If omitted, the factory selects. */
  client?: NexusClient;
  /** Override desktop capabilities (tests). `null` hides desktop affordances. */
  desktop?: DesktopCapabilities | null;
  /** Override the connection config (tests). */
  connectionConfig?: ConnectionConfig | null;
  /** Override the config setter (tests). */
  onConnectionConfigChange?: (config: ConnectionConfig | null) => Promise<void>;
  children: ReactNode;
}

interface ResolvedClients {
  client: NexusClient;
  desktop: DesktopCapabilities | null;
}

function buildClient(config: ConnectionConfig | null, desktop: boolean): NexusClient {
  if (!config || config.active === false) {
    return desktop ? new TauriClient() : new BrowserClient();
  }
  return desktop
    ? new TauriClient({ baseUrl: config.endpointUrl, apiKey: config.apiKey })
    : new BrowserClient({ baseUrl: config.endpointUrl, apiKey: config.apiKey });
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

export function ClientProvider({
  client,
  desktop,
  connectionConfig: injectedConfig,
  onConnectionConfigChange: injectedSetter,
  children,
}: ClientProviderProps) {
  const [storedConfig, setStoredConfig] = useState<ConnectionConfig | null>(null);
  const [loaded, setLoaded] = useState(false);
  const storage = useMemo(() => createConnectionStorage(), []);
  const isDesktop = useMemo(() => isDesktopBuild(), []);

  useEffect(() => {
    if (injectedConfig !== undefined) {
      setStoredConfig(injectedConfig);
      setLoaded(true);
      return;
    }
    let cancelled = false;
    storage
      .load()
      .then((cfg) => {
        if (cancelled) return;
        setStoredConfig(cfg);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
    return () => {
      cancelled = true;
    };
  }, [injectedConfig, storage]);

  const config = injectedConfig !== undefined ? injectedConfig : storedConfig;

  const setConfig = useMemo(
    () =>
      injectedSetter ??
      (async (next: ConnectionConfig | null) => {
        setStoredConfig(next);
        if (next === null) {
          await storage.clear();
        } else {
          await storage.save(next);
        }
      }),
    [injectedSetter, storage],
  );

  const value = useMemo<ResolvedClients>(() => {
    if (client) return { client, desktop: desktop ?? null };
    if (!loaded) return { client: new BrowserClient(), desktop: null };
    return {
      client: buildClient(config, isDesktop),
      desktop: isDesktop ? new TauriDesktopCapabilities() : null,
    };
  }, [client, desktop, config, loaded, isDesktop]);

  return (
    <ClientContext.Provider value={value.client}>
      <DesktopContext.Provider value={value.desktop}>
        <ConnectionConfigContext.Provider value={config}>
          <SetConnectionConfigContext.Provider value={setConfig}>
            {children}
          </SetConnectionConfigContext.Provider>
        </ConnectionConfigContext.Provider>
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

/** The currently active (or saved-but-inactive) connection config, if any. */
export function useConnectionConfig(): ConnectionConfig | null {
  return useContext(ConnectionConfigContext);
}

/** Setter for the active connection config. Passing `null` clears remote mode. */
export function useSetConnectionConfig(): (config: ConnectionConfig | null) => Promise<void> {
  const setter = useContext(SetConnectionConfigContext);
  if (!setter) throw new Error('useSetConnectionConfig must be used within a ClientProvider');
  return setter;
}
