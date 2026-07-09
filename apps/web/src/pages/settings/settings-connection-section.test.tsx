/**
 * Settings Connection section — mount + locked chrome copy.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';

import { SettingsConnectionSection } from '@/pages/settings/settings-connection-section';
import { renderInApp } from '@/test/test-providers';
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
});
