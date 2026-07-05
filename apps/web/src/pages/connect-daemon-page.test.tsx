import { describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ConnectDaemonPage } from '@/pages/connect-daemon-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { ConnectionConfig } from '@/lib/nexus/connection-storage';
import * as clientContext from '@/lib/client-context';

const mockedNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom');
  return { ...actual, useNavigate: () => mockedNavigate };
});

describe('ConnectDaemonPage', () => {
  it('renders the setup form and fetches a fingerprint on first use', async () => {
    const setConfig = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.json({ fingerprint: 'SHA256:aa:bb:cc', algorithm: 'sha256' }),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null} onConnectionConfigChange={setConfig}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.type(screen.getByTestId('api-key-input'), 'secret-key');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-block')).toHaveTextContent('SHA256:aa:bb:cc');
    });

    await userEvent.click(screen.getByTestId('trust-connect-button'));
    await waitFor(() => {
      expect(setConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          endpointUrl: 'https://remote.example:8420',
          apiKey: 'secret-key',
          active: true,
          pinnedFingerprint: 'SHA256:aa:bb:cc',
        }),
      );
    });
  });

  it('shows a blocking warning when the fingerprint changes', async () => {
    const setConfig = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    const saved: ConnectionConfig = {
      endpointUrl: 'https://remote.example:8420',
      apiKey: 'secret-key',
      pinnedFingerprint: 'SHA256:old:fingerprint',
      active: true,
    };
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(saved);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.json({ fingerprint: 'SHA256:new:fingerprint', algorithm: 'sha256' }),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={saved} onConnectionConfigChange={setConfig}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-mismatch-warning')).toBeInTheDocument();
    });

    expect(setConfig).not.toHaveBeenCalled();
  });

  it('reverts to local without deleting the saved config', async () => {
    const setConfig = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    const saved: ConnectionConfig = {
      endpointUrl: 'https://remote.example:8420',
      apiKey: 'secret-key',
      pinnedFingerprint: 'SHA256:aa:bb:cc',
      active: true,
    };
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(saved);

    renderInApp(
      <clientContext.ClientProvider connectionConfig={saved} onConnectionConfigChange={setConfig}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    await userEvent.click(screen.getByTestId('revert-local-button'));

    await waitFor(() => {
      expect(setConfig).toHaveBeenCalledWith(expect.objectContaining({ active: false }));
    });
  });

  it('shows the loopback-only info note when the daemon has no TLS cert', async () => {
    const setConfig = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.json({ fingerprint: '', algorithm: 'sha256' }),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null} onConnectionConfigChange={setConfig}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('loopback-info-note')).toBeInTheDocument();
    });
  });

  it('masks the API key in the input by default', async () => {
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    const input = screen.getByTestId('api-key-input');
    await userEvent.type(input, 'secret-key');
    expect(input).toHaveAttribute('type', 'password');
  });

  it('shows an error when the fingerprint endpoint is unreachable', async () => {
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.error(),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-error')).toBeInTheDocument();
    });
  });

  it('shows an error when the fingerprint endpoint returns 500', async () => {
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal_error', message: 'Fingerprint lookup failed.' } },
          { status: 500 },
        ),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <ConnectDaemonPage />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-error')).toHaveTextContent('Fingerprint lookup failed.');
    });
  });
});
