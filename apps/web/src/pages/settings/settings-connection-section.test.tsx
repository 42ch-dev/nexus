/**
 * Settings Connection section — mount + locked chrome copy.
 */
import { describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SettingsConnectionSection } from '@/pages/settings/settings-connection-section';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import * as clientContext from '@/lib/client-context';

describe('SettingsConnectionSection', () => {
  it('renders section chrome with locked helper and mounts ConnectDaemonForm', () => {
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <SettingsConnectionSection />
      </clientContext.ClientProvider>,
    );

    const section = screen.getByTestId('settings-connection-section');
    expect(section).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Connection' })).toBeInTheDocument();
    expect(
      screen.getByText(
        /Connect this app to a remote Nexus daemon\. Your local daemon stays the default/i,
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId('connect-daemon-form')).toBeInTheDocument();
  });

  it('renders the promoted <TransportErrorBlock> when the fingerprint endpoint is unreachable (V1.129 P1)', async () => {
    vi.spyOn(clientContext, 'useConnectionConfig').mockReturnValue(null);

    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', () =>
        HttpResponse.error(),
      ),
    );

    renderInApp(
      <clientContext.ClientProvider connectionConfig={null}>
        <SettingsConnectionSection />
      </clientContext.ClientProvider>,
    );

    await userEvent.type(screen.getByTestId('daemon-url-input'), 'https://remote.example:8420');
    await userEvent.click(screen.getByTestId('fetch-fingerprint-button'));

    // The section composes ConnectDaemonForm, which now consumes the promoted
    // primitive for transport-classified failures. Verifies the section-level
    // wiring (the primitive is reachable through the Settings → Connection
    // page that a manual tester loads after a stale-URL resume-gate redirect).
    await waitFor(() => {
      const block = screen.getByTestId('transport-error-block');
      expect(block).toHaveAttribute('data-kind', 'network');
    });
  });
});
