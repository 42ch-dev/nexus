/**
 * 操作台 Hub page (plan v1220 T1) — route skeleton + developer rail wiring.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';
import { Route, Routes } from 'react-router';

import { HubPage } from '@/components/developer/hub-page';
import { AppModuleRoute } from '@/components/shell/app-shell';
import { i18n } from '@/lib/i18n/config';
import { renderInApp } from '@/test/test-providers';

describe('HubPage', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('en');
  });

  it('renders the page shell with the four status sections', () => {
    renderInApp(<HubPage />);

    expect(screen.getByTestId('hub-page')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1, name: 'Hub' })).toBeInTheDocument();
    for (const id of ['service', 'agent', 'harness', 'modules']) {
      expect(screen.getByTestId(`hub-card-${id}`)).toBeInTheDocument();
    }
  });

  it('renders through the /developer route', async () => {
    window.localStorage.setItem('nexus-entrance', 'developer');
    renderInApp(
      <Routes>
        <Route path="/developer" element={<AppModuleRoute appId="hub" />} />
      </Routes>,
      { initialRouterEntries: ['/developer'] },
    );

    expect(await screen.findByTestId('hub-page')).toBeInTheDocument();
  });
});
