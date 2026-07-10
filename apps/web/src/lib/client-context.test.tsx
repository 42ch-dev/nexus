import { describe, expect, it, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';

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

    expect(await screen.findByText('Could not verify daemon identity')).toBeInTheDocument();
    expect(screen.getByText(/Cannot reach the daemon at this address/)).toBeInTheDocument();
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

    expect(await screen.findByText('Could not verify daemon identity')).toBeInTheDocument();
    expect(fetchImpl).toHaveBeenCalledTimes(1);

    screen.getByText('Try again').click();

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
