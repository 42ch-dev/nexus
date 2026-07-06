import { describe, expect, it, vi, type Mock } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';

import { ClientProvider, useNexusClient, useFingerprintGateState } from '@/lib/client-context';
import type { ConnectionConfig } from '@/lib/nexus/connection-storage';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

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
  return <span data-testid="current-path">{location.pathname}</span>;
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
  config: ConnectionConfig | null,
  fetchImpl?: typeof fetch,
  initialEntries: string[] = ['/'],
) {
  return render(
    <QueryClientProvider client={makeQueryClient()}>
      <MemoryRouter initialEntries={initialEntries}>
        <ClientProvider connectionConfig={config} fetchImpl={fetchImpl}>
          <Routes>
            <Route path="/" element={<TestChild />} />
            <Route path="/connect" element={<div data-testid="connect-page">Connect</div>} />
          </Routes>
          <RouteSpy />
        </ClientProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('ClientProvider resume-time fingerprint gate', () => {
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

  it('redirects to /connect on fingerprint mismatch and does not mount children', async () => {
    const fetchImpl = makeFetchImpl({ fingerprint: 'served-fingerprint' });
    const config: ConnectionConfig = {
      endpointUrl: 'https://remote.example.com',
      apiKey: 'key-1',
      pinnedFingerprint: 'stored-fingerprint',
      active: true,
    };
    renderWithGate(config, fetchImpl);

    await waitFor(() => {
      expect(screen.getByTestId('current-path')).toHaveTextContent('/connect');
    });

    expect(screen.queryByTestId('child')).not.toBeInTheDocument();
    expect(screen.getByTestId('connect-page')).toBeInTheDocument();

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const requestUrl = (fetchImpl as unknown as Mock).mock.calls[0][0] as string;
    expect(requestUrl).toBe('https://remote.example.com/v1/daemon/runtime/cert-fingerprint');
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
});
