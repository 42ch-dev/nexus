/**
 * 操作台 Hub page (plan v1220 T1) — route skeleton + developer rail wiring.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';

import { HubPage } from '@/components/developer/hub-page';
import { appsForEntrance } from '@/lib/app-registry';
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

  it('is the developer rail hub entry: /developer, all five apps unlocked', async () => {
    const apps = appsForEntrance('developer');
    const hub = apps.find((app) => app.id === 'hub');

    expect(apps).toHaveLength(5);
    expect(hub?.route).toBe('/developer');
    expect(apps.every((app) => app.lockReason === undefined)).toBe(true);

    const module = await hub!.load();
    expect(module.default).toBe(HubPage);
  });
});
