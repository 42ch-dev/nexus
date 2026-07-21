import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { useNavigate, useLocation } from 'react-router-dom';

import { BrowserClient, type NexusClient } from '@/lib/nexus';
import { TauriClient } from '@/lib/nexus/tauri-client';
import { TauriDesktopCapabilities, type DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { isDesktopBuild } from '@/lib/nexus/detect';
import {
  createConnectionStorage,
  type ConnectionConfig,
} from '@/lib/nexus/connection-storage';
import {
  useResumeFingerprintGate,
  type ResumeFingerprintGateState,
} from '@/lib/nexus/use-resume-fingerprint-gate';
import { Button } from '@/components/ui/button';
import { LoadingState, ErrorState } from '@/components/ui/states';
import { TransportErrorBlock } from '@42ch/nexus-ui';

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
const FingerprintGateContext = createContext<ResumeFingerprintGateState | null>(null);

export interface ClientProviderProps {
  /** Override the NexusClient (tests). If omitted, the factory selects. */
  client?: NexusClient;
  /** Override desktop capabilities (tests). `null` hides desktop affordances. */
  desktop?: DesktopCapabilities | null;
  /** Override the connection config (tests). */
  connectionConfig?: ConnectionConfig | null;
  /** Override the config setter (tests). */
  onConnectionConfigChange?: (config: ConnectionConfig | null) => Promise<void>;
  /** Override fetch for the fingerprint gate (tests). */
  fetchImpl?: typeof fetch;
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

/**
 * Resume-time TOFU gate shell (daemon-runtime.md §16.2 Phases 2–3).
 *
 * Blocks the app from mounting any screen that may issue authenticated daemon
 * requests until a pinned remote fingerprint is re-verified. Local mode and
 * configs without a pinned fingerprint bypass the gate. Mismatch is resolved
 * by redirecting to `/settings/connection` so the user can re-pin.
 */
function FingerprintGate({
  fetchImpl,
  children,
}: {
  fetchImpl?: typeof fetch;
  children: ReactNode;
}) {
  const config = useConnectionConfig();
  const { state, verify } = useResumeFingerprintGate(config, { fetchImpl });
  const navigate = useNavigate();
  const location = useLocation();

  // Connection re-pin lives under Settings (V1.106). Allow the recovery path
  // (and legacy `/connect` / `/settings/connection` while they redirect) to
  // mount on fingerprint mismatch so the author is not hard-locked out of
  // re-pinning. The Advanced page also hosts the Setup section, so only the
  // Connection hash (or the landing view) is allowed; `#setup` is redirected
  // to `#connection` while identity is unresolved.
  const isConnectRoute =
    location.pathname === '/connect' ||
    location.pathname === '/settings/connection' ||
    location.pathname === '/setup' ||
    (location.pathname === '/settings/advanced' &&
      (location.hash === '#connection' || location.hash === ''));

  useEffect(() => {
    if (state.status === 'mismatch' && !isConnectRoute) {
      navigate('/settings/advanced#connection', { replace: true });
    }
  }, [state.status, navigate, isConnectRoute]);

  if (!isConnectRoute && state.status === 'verifying') {
    return (
      <div className="flex min-h-screen items-center justify-center p-6">
        <LoadingState label="Verifying daemon identity…" />
      </div>
    );
  }

  if (!isConnectRoute && state.status === 'fetch-failed') {
    return (
      <div className="flex min-h-screen items-center justify-center p-6">
        <div className="w-full max-w-md space-y-4">
          {state.kind ? (
            <TransportErrorBlock
              kind={state.kind}
              // The gate owns retry (re-run verification) and the deep-link
              // to Connection settings (re-pin / re-enter endpoint).
              onRetry={() => void verify()}
              onOpenSettings={() => navigate('/settings/advanced#connection')}
              // `state.message` carries the classifier's diagnostic message
              // (e.g., the long-form "Cannot reach the daemon at this
              // address…" string). Surfaced as the block's detail line so
              // the primitive owns headline+body+CTAs (single voice) and the
              // diagnostic stays accessible below.
              detail={state.message}
            />
          ) : (
            <>
              <ErrorState
                title="Could not verify daemon identity"
                description={state.message}
                onRetry={() => void verify()}
                retryLabel="Try again"
              />
              <div className="mt-4 flex justify-center">
                <Button variant="secondary" onClick={() => navigate('/settings/advanced#connection')}>
                  Reconnect
                </Button>
              </div>
            </>
          )}
        </div>
      </div>
    );
  }

  return (
    <FingerprintGateContext.Provider value={state}>
      {children}
    </FingerprintGateContext.Provider>
  );
}

export function ClientProvider({
  client,
  desktop,
  connectionConfig: injectedConfig,
  onConnectionConfigChange: injectedSetter,
  fetchImpl,
  children,
}: ClientProviderProps) {
  const [storedConfig, setStoredConfig] = useState<ConnectionConfig | null>(null);
  const [loaded, setLoaded] = useState(false);
  const storage = useMemo(() => createConnectionStorage(), []);
  const isDesktop = useMemo(() => isDesktopBuild(), []);
  // Keep a live ref so setConfig can roll back optimistic updates without a stale closure.
  const storedConfigRef = useRef<ConnectionConfig | null>(null);

  useEffect(() => {
    if (injectedConfig !== undefined) {
      setStoredConfig(injectedConfig);
      storedConfigRef.current = injectedConfig;
      setLoaded(true);
      return;
    }
    let cancelled = false;
    storage
      .load()
      .then((cfg) => {
        if (cancelled) return;
        setStoredConfig(cfg);
        storedConfigRef.current = cfg;
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
        const previous = storedConfigRef.current;
        setStoredConfig(next);
        storedConfigRef.current = next;
        try {
          if (next === null) {
            await storage.clear();
          } else {
            await storage.save(next);
          }
        } catch (err) {
          setStoredConfig(previous);
          storedConfigRef.current = previous;
          throw err;
        }
      }),
    [injectedSetter, storage],
  );

  const value = useMemo<ResolvedClients>(() => {
    if (client) return { client, desktop: desktop ?? null };
    if (!loaded) {
      if (isDesktop) {
        return { client: new TauriClient(), desktop: new TauriDesktopCapabilities() };
      }
      return { client: new BrowserClient(), desktop: null };
    }
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
            {client ? (
              children
            ) : (
              <FingerprintGate fetchImpl={fetchImpl}>{children}</FingerprintGate>
            )}
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

/**
 * Exposes the resume-time fingerprint gate state for screens that need to
 * reason about verification status (e.g. tests, diagnostics).
 */
export function useFingerprintGateState(): ResumeFingerprintGateState | null {
  return useContext(FingerprintGateContext);
}
