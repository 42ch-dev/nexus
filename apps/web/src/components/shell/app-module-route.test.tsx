/**
 * AppModuleRoute (plan v1219 T2 — lazy-chunk assertion):
 *
 * Tests inject a registry whose `load` factories are spies and assert:
 *  - a locked app's factory is NEVER invoked and its chunk stays unrequested —
 *    the route redirects to the first unlocked app with the § Copy reason as toast;
 *  - an unlocked app's factory resolves and its element renders.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { screen } from '@testing-library/react';
import { Route, Routes } from 'react-router';
import { Globe } from 'lucide-react';

import { AppModuleRoute } from '@/components/shell/app-shell';
import { i18n } from '@/lib/i18n/config';
import {
  isAppLocked,
  RAIL_LOCK_REASON,
  type AppDef,
  type AppId,
} from '@/lib/app-registry';
import type * as AppRegistryModule from '@/lib/app-registry';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

const loadSpy: Mock<AppDef['load']> = vi.fn(() =>
  Promise.resolve({ default: () => <div data-testid="work-module">work module</div> }),
);

const registryApps: readonly AppDef[] = [
  { id: 'world', label: 'rail.world', icon: Globe, route: '/worlds', load: loadSpy },
  {
    id: 'work',
    label: 'rail.work',
    icon: Globe,
    route: '/works',
    load: loadSpy,
    lockedUntil: 'brief-saved',
    lockReason: RAIL_LOCK_REASON.work,
  },
];

// Inject the spy registry; keep the real lock-resolution logic.
vi.mock('@/lib/app-registry', async (importOriginal) => {
  const actual = await importOriginal<typeof AppRegistryModule>();
  return {
    ...actual,
    appsForEntrance: (_entrance: unknown) => registryApps,
  };
});

function renderRoute(appId: AppId) {
  return renderInApp(
    <Routes>
      <Route path="/works" element={<AppModuleRoute appId={appId} />} />
      <Route path="/worlds" element={<div data-testid="world-page">world</div>} />
    </Routes>,
    { activeCreatorId: 'creator-a', initialRouterEntries: ['/works'] },
  );
}

describe('AppModuleRoute — progressive activation lazy boundary', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    loadSpy.mockClear();
    await i18n.changeLanguage('zh-CN');
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );
  });

  it('never invokes the factory for a locked app; redirects with the reason toast', async () => {
    renderRoute('work');
    // Bounced to the first unlocked app (世界) without requesting the chunk.
    expect(await screen.findByTestId('world-page')).toBeInTheDocument();
    expect(loadSpy).not.toHaveBeenCalled();
    expect(await screen.findByText('完成 Brief 后解锁「作品」')).toBeInTheDocument();
  });

  it('resolves the factory and renders the element once unlocked', async () => {
    window.localStorage.setItem(
      'nexus-activation:creator-a',
      JSON.stringify({ work: true }),
    );
    renderRoute('work');
    expect(await screen.findByTestId('work-module')).toBeInTheDocument();
    expect(loadSpy).toHaveBeenCalledTimes(1);
  });

  it('registry contract sanity: only brief-saved apps consult the store', () => {
    const work = registryApps.find((a) => a.id === 'work')!;
    expect(isAppLocked(work, () => false)).toBe(true);
    expect(isAppLocked(work, () => true)).toBe(false);
  });
});
