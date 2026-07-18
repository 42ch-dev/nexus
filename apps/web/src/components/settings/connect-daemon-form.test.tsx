import { describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ConnectDaemonForm } from '@/components/settings/connect-daemon-form';
import { renderInApp, noopClient } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { ConnectionConfig } from '@/lib/nexus/connection-storage';
import * as clientContext from '@/lib/client-context';

const mockedNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom');
  return { ...actual, useNavigate: () => mockedNavigate };
});

describe('ConnectDaemonForm', () => {
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
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.type(screen.getByTestId('api-key-input'), 'secret-key');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-block')).toHaveTextContent('SHA256:aa:bb:cc');
    });
    // V1.121 P2: fingerprint block size rides the copy-13 token (no arbitrary
    // text-[13px]).
    const fingerprintBlock = screen.getByTestId('fingerprint-block');
    expect(fingerprintBlock).toHaveClass('text-copy-13');
    expect(fingerprintBlock.className).not.toContain('text-[13px]');

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
    // Post-activate stays on Connection — no navigate away.
    expect(mockedNavigate).not.toHaveBeenCalled();
  });

  it('shows a reassurance hint when reconnecting to a pinned-matching endpoint', async () => {
    const setConfig = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    const saved: ConnectionConfig = {
      endpointUrl: 'https://remote.example:8420',
      apiKey: 'secret-key',
      pinnedFingerprint: 'SHA256:aa:bb:cc',
      active: true,
    };
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(saved);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.json({ fingerprint: 'SHA256:aa:bb:cc', algorithm: 'sha256' }),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider
        client={noopClient}
        connectionConfig={saved}
        onConnectionConfigChange={setConfig}
      >
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-match-hint')).toBeInTheDocument();
    });
    expect(screen.getByTestId('trust-connect-button')).toHaveTextContent(
      'Reconnect With These Settings',
    );
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
      <clientContext.ClientProvider
        client={noopClient}
        connectionConfig={saved}
        onConnectionConfigChange={setConfig}
      >
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-mismatch-warning')).toBeInTheDocument();
    });

    expect(setConfig).not.toHaveBeenCalled();
  });

  it('reverts to local without deleting the saved config and does not navigate away', async () => {
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
      <clientContext.ClientProvider
        client={noopClient}
        connectionConfig={saved}
        onConnectionConfigChange={setConfig}
      >
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.click(screen.getByTestId('revert-local-button'));

    await waitFor(() => {
      expect(setConfig).toHaveBeenCalledWith(expect.objectContaining({ active: false }));
    });
    expect(mockedNavigate).not.toHaveBeenCalled();
  });

  it('shows an error toast when activate setConfig rejects', async () => {
    const setConfig = vi.fn().mockRejectedValue(new Error('storage write failed'));
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.json({ fingerprint: 'SHA256:aa:bb:cc', algorithm: 'sha256' }),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null} onConnectionConfigChange={setConfig}>
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.type(screen.getByTestId('api-key-input'), 'secret-key');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('trust-connect-button')).toBeInTheDocument();
    });

    await userEvent.click(screen.getByTestId('trust-connect-button'));

    await waitFor(() => {
      expect(screen.getByText('Could not connect to daemon')).toBeInTheDocument();
    });
    expect(screen.getByText('storage write failed')).toBeInTheDocument();
    expect(screen.queryByText('Connected to daemon')).not.toBeInTheDocument();
    expect(mockedNavigate).not.toHaveBeenCalled();
  });

  it('shows an error toast when revert setConfig rejects', async () => {
    const setConfig = vi.fn().mockRejectedValue(new Error('storage write failed'));
    vi.spyOn(clientContext, 'useSetConnectionConfig').mockReturnValue(setConfig);
    const saved: ConnectionConfig = {
      endpointUrl: 'https://remote.example:8420',
      apiKey: 'secret-key',
      pinnedFingerprint: 'SHA256:aa:bb:cc',
      active: true,
    };
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(saved);

    renderInApp(
      <clientContext.ClientProvider
        client={noopClient}
        connectionConfig={saved}
        onConnectionConfigChange={setConfig}
      >
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.click(screen.getByTestId('revert-local-button'));

    await waitFor(() => {
      expect(screen.getByText('Could not switch to local daemon')).toBeInTheDocument();
    });
    expect(screen.getByText('storage write failed')).toBeInTheDocument();
    expect(screen.queryByText('Using local daemon')).not.toBeInTheDocument();
    expect(mockedNavigate).not.toHaveBeenCalled();
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
        <ConnectDaemonForm />
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
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    const input = screen.getByTestId('api-key-input');
    await userEvent.type(input, 'secret-key');
    expect(input).toHaveAttribute('type', 'password');
  });

  it('toggles API key visibility with a Verb-only label and object-bearing aria-label (AC-P4-4)', async () => {
    const user = userEvent.setup();
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    const input = screen.getByTestId('api-key-input') as HTMLInputElement;
    await user.type(input, 'secret-key');
    expect(input).toHaveAttribute('type', 'password');

    // Hidden state: visible Verb-only "Show"; accessible name keeps the object.
    const showButton = screen.getByRole('button', { name: 'Show key' });
    expect(showButton).toHaveTextContent('Show');
    expect(showButton).toHaveAttribute('aria-pressed', 'false');

    await user.click(showButton);

    // Revealed state: visible Verb-only "Hide"; accessible name keeps the object.
    const hideButton = await screen.findByRole('button', { name: 'Hide key' });
    expect(hideButton).toHaveTextContent('Hide');
    expect(hideButton).toHaveAttribute('aria-pressed', 'true');
    expect(input).toHaveAttribute('type', 'text');
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
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-error')).toBeInTheDocument();
      expect(screen.getByTestId('fingerprint-error')).toHaveTextContent('Trust On First Use');
      expect(screen.getByTestId('fingerprint-error')).toHaveTextContent('desktop app');
    });
    // V1.121 P2: error surface consumes the P1 error-surface tokens with the
    // ErrorState text recipe (red-1000 title / red-900 helper).
    const errorRegion = screen.getByTestId('fingerprint-error');
    expect(errorRegion).toHaveClass('bg-error-surface');
    expect(errorRegion).toHaveClass('border-error-surface-border');
    expect(errorRegion.querySelector('.text-red-1000')).not.toBeNull();
    expect(errorRegion.querySelector('.text-red-900')).not.toBeNull();
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
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    await waitFor(() => {
      expect(screen.getByTestId('fingerprint-error')).toHaveTextContent('Fingerprint lookup failed.');
    });
  });

  it('renders locked form card description and field helpers', () => {
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <ConnectDaemonForm />
      </clientContext.ClientProvider>,
    );

    expect(screen.getByText('Connect to Daemon')).toBeInTheDocument();
    expect(
      screen.getByText(
        /Enter the remote daemon URL and API key\. Local mode remains available/i,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/The full HTTPS address of the daemon, including port/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/The API key from the daemon machine/i)).toBeInTheDocument();
    expect(screen.getByText('nexus42 daemon api-key')).toBeInTheDocument();
    // V1.121 P2: inline code helpers ride the copy-13 token (no arbitrary
    // text-[13px]).
    expect(screen.getByText('nexus42 daemon api-key')).toHaveClass('text-copy-13');
  });
});
