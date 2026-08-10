import { describe, expect, it, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router';

import { ClientProvider, useNexusClient, useFingerprintGateState, useConnectionConfig, useSetConnectionConfig } from '@/lib/client-context';
import { createConnectionStorage, type ConnectionConfig } from '@/lib/nexus/connection-storage';
import { isDesktopBuild } from '@/lib/nexus/detect';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import userEvent from '@testing-library/user-event';

vi.mock('@/lib/nexus/detect', () => ({
  isDesktopBuild: vi.fn(),
}));

vi.mock('@/lib/nexus/connection-storage', async () => {
  const actual = await vi.importActual<typeof import('@/lib/nexus/connection-storage')>(
    '@/lib/nexus/connection-storage',
  );
  return {
    ...actual,
    createConnectionStorage: vi.fn(() => ({
      load: vi.fn(() => new Promise<ConnectionConfig | null>(() => {})),
      save: vi.fn(async () => {}),
      clear: vi.fn(async () => {}),
    })),
  };
});

function makeFetchImpl(response: { fingerprint: string }, status = 200): typeof fetch {
  return vi.fn(async () =>
    Promise.resolve(
      new Response(JSON.stringify(response), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  ) as unknown as typeof fetch;
}

function makeSequenceFetchImpl(
  sequence: Array<{ fingerprint: string } | Error>,
): typeof fetch {
  let call = 0;
  return vi.fn(async () => {
    const next = sequence[call];
    call += 1;
    if (next instanceof Error) {
      return Promise.reject(next);
    }
    return Promise.resolve(
      new Response(JSON.stringify(next), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
  }) as unknown as typeof fetch;
}

function TestChild() {
  const client = useNexusClient();
  const gate = useFingerprintGateState();
  return (
    <div data-testid="child">
      <span data-testid="client-type">{client.constructor.name}</span>
      <span data-testid="gate-status">{gate?.status ?? 'null'}</span>
    </div>
  );
}

function RouteSpy() {
  const location = useLocation();
  return (
    <>
      <span data-testid="current-path">{location.pathname}</span>
      <span data-testid="current-hash">{location.hash}</span>
    </>
  );
}

function makeQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: 0, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
}

function renderWithGate(
  config: ConnectionConfig | null | undefined,
  fetchImpl?: typeof fetch,
  initialEntries: string[] = ['/'],
) {
  return render(
    <QueryClientProvider client={makeQueryClient()}>
      <MemoryRouter initialEntries={initialEntries}>
        <ClientProvider connectionConfig={config} fetchImpl={fetchImpl}>
          <Routes>
            <Route path="/" element={<TestChild />} />
            <Route
              path="/settings/agent"
              element={<div data-testid="settings-agent-page">Agent</div>}
            />
            <Route
              path="/settings/setup"
              element={<div data-testid="settings-setup-page">Setup</div>}
            />
            <Route
              path="/settings/advanced"
              element={<div data-testid="connect-page">Connect</div>}
            />
            <Route path="/connect" element={<div data-testid="legacy-connect">Legacy</div>} />
            <Route path="/setup" element={<TestChild />} />
          </Routes>
          <RouteSpy />
        </ClientProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('ClientProvider resume-time fingerprint gate', () => {
  beforeEach(() => {
    vi.mocked(isDesktopBuild).mockReturnValue(false);
  });

  it('bypasses the gate for local mode and renders children immediately', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'abc' });
    renderWithGate(null, fetchImpl);

    expect(await screen.findByTestId('child')).toBeInTheDocument();
    expect(screen.getByTestId('gate-status')).toHaveTextContent('verified');
    expect(screen.getByTestId('client-type')).toHaveTextContent('BrowserClient');
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('stays on local transport when a saved config lacks explicit active: true', async () => {
    // Poisoned / partial configs (e.g. unit-test fallback files with only
    // endpointUrl + apiKey) must not redirect the client off loopback.
    vi.mocked(isDesktopBuild).mockReturnValue(true);

    const poison: ConnectionConfig = {
      endpointUrl: 'https://x',
      apiKey: 'k',
      // active intentionally omitted
    };

    function Probe() {
      const client = useNexusClient();
      // TauriClient exposes `port` only for loopback mode; remote overrides
      // leave it undefined.
      const port = 'port' in client ? (client as { port?: number }).port : undefined;
      return (
        <div data-testid="probe">
          <span data-testid="client-type">{client.constructor.name}</span>
          <span data-testid="client-port">{port ?? 'none'}</span>
        </div>
      );
    }

    render(
      <QueryClientProvider client={makeQueryClient()}>
        <MemoryRouter>
          <ClientProvider connectionConfig={poison}>
            <Probe />
          </ClientProvider>
        </MemoryRouter>
      </QueryClientProvider>,
    );

    expect(await screen.findByTestId('client-type')).toHaveTextContent('TauriClient');
    // Loopback default port — not undefined remote-override mode.
    expect(screen.getByTestId('client-port')).toHaveTextContent('8420');
  });

  it('bypasses the gate when the saved config has no pinned fingerprint', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'abc' });
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      active: true,
    };
    renderWithGate(config, fetchImpl);

    expect(await screen.findByTestId('child')).toBeInTheDocument();
    expect(screen.getByTestId('gate-status')).toHaveTextContent('verified');
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('verifies a matching pinned fingerprint before rendering children', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'matching-fingerprint' });
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'matching-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl);

    expect(screen.getByText('Verifying daemon identity…')).toBeInTheDocument();

    expect(await screen.findByTestId('child')).toBeInTheDocument();
    expect(screen.getByTestId('gate-status')).toHaveTextContent('verified');
    expect(screen.getByTestId('client-type')).toHaveTextContent('BrowserClient');

    await waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
    const requestUrl = (fetchImpl as unknown as Mock).mock.calls[0][0] as string;
    expect(requestUrl).toBe('https://remote.example.com/v1/daemon/runtime/cert-fingerprint');
  });

  it('redirects to /settings/advanced on fingerprint mismatch and does not mount children', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'served-fingerprint' });
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl);

    await waitFor(() => {
      expect(screen.getByTestId('current-path')).toHaveTextContent(
        '/settings/advanced',
      );
    });

    expect(screen.queryByTestId('child')).not.toBeInTheDocument();
    expect(screen.getByTestId('connect-page')).toBeInTheDocument();

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const requestUrl = (fetchImpl as unknown as Mock).mock.calls[0][0] as string;
    expect(requestUrl).toBe('https://remote.example.com/v1/daemon/runtime/cert-fingerprint');
  });

  it.each([
    { path: '/settings/agent', siblingTestId: 'settings-agent-page' },
    { path: '/settings/setup', siblingTestId: 'settings-setup-page' },
  ] as const)(
    'redirects fingerprint mismatch from $path to /settings/advanced (sibling is not a bypass)',
    async ({ path, siblingTestId }) => {
      const fetchImpl = makeFetchImpl({ fingerprint: 'served-fingerprint' });
      const config: ConnectionConfig = {
        endpointUrl: 'https://remote.example.com',
        apiKey: 'key-1',
        pinnedFingerprint: 'stored-fingerprint',
        active: true,
      };
      renderWithGate(config, fetchImpl, [path]);

      await waitFor(() => {
        expect(screen.getByTestId('current-path')).toHaveTextContent(
          '/settings/advanced',
        );
      });

      expect(screen.queryByTestId(siblingTestId)).not.toBeInTheDocument();
      expect(screen.getByTestId('connect-page')).toBeInTheDocument();
      expect(fetchImpl).toHaveBeenCalledTimes(1);
    },
  );

  it('allows /settings/advanced#connection on fingerprint mismatch', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'served-fingerprint' });
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl, ['/settings/advanced#connection']);

    await waitFor(() => {
      expect(screen.getByTestId('current-path')).toHaveTextContent(
        '/settings/advanced',
      );
    });

    expect(screen.getByTestId('current-hash')).toHaveTextContent('#connection');
    expect(screen.getByTestId('connect-page')).toBeInTheDocument();
    expect(screen.queryByTestId('child')).not.toBeInTheDocument();
  });

  it('redirects /settings/advanced#setup to #connection on fingerprint mismatch', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'served-fingerprint' });
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl, ['/settings/advanced#setup']);

    await waitFor(() => {
      expect(screen.getByTestId('current-path')).toHaveTextContent(
        '/settings/advanced',
      );
      expect(screen.getByTestId('current-hash')).toHaveTextContent('#connection');
    });

    expect(screen.getByTestId('connect-page')).toBeInTheDocument();
    expect(screen.queryByTestId('settings-setup-page')).not.toBeInTheDocument();
  });

  it('shows a retryable error when fingerprint fetch fails', async () => {
    const error = new Error('Daemon unreachable');
    const fetchImpl = makeSequenceFetchImpl([error]);
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl);

    // V1.129 P1: fetch-failed now renders the promoted <TransportErrorBlock>
    // for kind=network (the classifier's catch-all for undifferentiated fetch
    // throws). The block owns headline + body + CTAs; the diagnostic message
    // rides the `detail` line.
    const block = await screen.findByTestId('transport-error-block');
    expect(block).toHaveAttribute('data-kind', 'network');
    expect(block).toHaveTextContent('Could not connect to the daemon at this address');
    // Diagnostic detail (classifier long-form message) still surfaces.
    expect(screen.getByText(/Cannot reach the daemon at this address/)).toBeInTheDocument();
    // CTA matrix for network: Open Connection Settings primary, Retry secondary.
    expect(screen.getByTestId('transport-error-primary')).toHaveAttribute(
      'data-cta',
      'openConnectionSettings',
    );
    expect(screen.getByTestId('transport-error-secondary')).toHaveAttribute(
      'data-cta',
      'retry',
    );
    expect(screen.queryByTestId('child')).not.toBeInTheDocument();

    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  it('retries after a failed fetch and renders children when the fingerprint matches', async () => {
    const fetchImpl = makeSequenceFetchImpl([
      new Error('Daemon unreachable'),
      { fingerprint: 'stored-fingerprint' },
    ]);
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl);

    expect(await screen.findByTestId('transport-error-block')).toHaveAttribute(
      'data-kind',
      'network',
    );
    expect(fetchImpl).toHaveBeenCalledTimes(1);

    // Retry CTA is the secondary button for kind=network.
    screen.getByTestId('transport-error-secondary').click();

    expect(await screen.findByTestId('child')).toBeInTheDocument();
    expect(screen.getByTestId('gate-status')).toHaveTextContent('verified');
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it('selects TauriClient on first render while config is loading in desktop build', () => {
    vi.mocked(isDesktopBuild).mockReturnValue(true);
    renderWithGate(undefined, undefined, ['/']);

    expect(screen.getByTestId('client-type')).toHaveTextContent('TauriClient');
  });

  it('treats /setup as a bypass route for the fingerprint gate', async () => {
    const fetchImpl = makeSequenceFetchImpl([new Error('Daemon unreachable')]);
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl, ['/setup']);

    // Children render immediately on /setup, even while verifying, and no
    // loading or error shell is shown.
    expect(await screen.findByTestId('child')).toBeInTheDocument();
    expect(screen.queryByText('Verifying daemon identity…')).not.toBeInTheDocument();
    expect(screen.queryByText('Could not verify daemon identity')).not.toBeInTheDocument();

    // After the fetch fails, children remain mounted and the gate still does
    // not block the route.
    await waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId('child')).toBeInTheDocument();
    expect(screen.queryByText('Could not verify daemon identity')).not.toBeInTheDocument();
  });
});

describe('ClientProvider setConfig optimistic rollback', () => {
  beforeEach(() => {
    vi.mocked(isDesktopBuild).mockReturnValue(false);
  });

  it('rolls back stored config when storage.save rejects', async () => {
    const initial: ConnectionConfig = {
      endpointUrl: 'https://old.example:8420',
      apiKey: 'old-key',
      active: true,
    };
    const next: ConnectionConfig = {
      endpointUrl: 'https://new.example:8420',
      apiKey: 'new-key',
      active: true,
    };
    const save = vi.fn().mockRejectedValue(new Error('disk full'));
    vi.mocked(createConnectionStorage).mockReturnValue({
      load: vi.fn().mockResolvedValue(initial),
      save,
      clear: vi.fn(async () => {}),
    });

    function Probe() {
      const config = useConnectionConfig();
      const setConfig = useSetConnectionConfig();
      return (
        <div>
          <span data-testid="endpoint">{config?.endpointUrl ?? 'none'}</span>
          <button
            type="button"
            data-testid="apply-next"
            onClick={() => {
              void setConfig(next).catch(() => {
                /* expected — form surfaces toast */
              });
            }}
          >
            Apply
          </button>
        </div>
      );
    }

    render(
      <QueryClientProvider client={makeQueryClient()}>
        <MemoryRouter>
          <ClientProvider>
            <Probe />
          </ClientProvider>
        </MemoryRouter>
      </QueryClientProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId('endpoint')).toHaveTextContent('https://old.example:8420');
    });

    await userEvent.click(screen.getByTestId('apply-next'));

    await waitFor(() => {
      expect(save).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(screen.getByTestId('endpoint')).toHaveTextContent('https://old.example:8420');
    });
  });
});
