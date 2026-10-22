/**
 * Settings Setup section — browser-available re-run entry
 * (v1219-onboarding T4, access spec §6.3).
 */
import { describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router';

import { SettingsSetupSection } from '@/pages/settings/settings-setup-section';
import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

const settingsRouteTree = (
  <Route path="settings">
    <Route path="setup" element={<SettingsSetupSection />} />
  </Route>
);

describe('SettingsSetupSection', () => {
  it('renders the product copy with no desktop capabilities', () => {
    renderInApp(<Routes>{settingsRouteTree}</Routes>, {
      client: new BrowserClient(),
      initialRouterEntries: ['/settings/setup'],
      setupCompleted: true,
    });

    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
    expect(
      screen.getByText(
        'Re-pick your identity, AI partner, and default workspace; created worlds and works are untouched.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId('settings-rerun-setup')).toBeEnabled();
    expect(screen.getByText('Re-run setup wizard')).toBeInTheDocument();
  });

  it('navigates to the always-reachable wizard route', async () => {
    const user = userEvent.setup();

    renderInApp(
      <>
        <Routes>
          {settingsRouteTree}
          <Route path="setup" element={<div data-testid="setup-wizard">Wizard</div>} />
        </Routes>
        <LocationProbe />
      </>,
      {
        client: new BrowserClient(),
        initialRouterEntries: ['/settings/setup'],
        setupCompleted: true,
      },
    );

    await user.click(screen.getByTestId('settings-rerun-setup'));

    await waitFor(() => expect(screen.getByTestId('setup-wizard')).toBeInTheDocument());
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(screen.queryByTestId('settings-setup-section')).not.toBeInTheDocument();
  });
});
