/**
 * Work-route nesting under WorkShellLayout — V1.118 P2 T2/T3.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';

import { WorkShellLayout } from '@/components/layout/work-shell-layout';
import { cn } from '@/lib/utils';
import { isWorkShellRoute } from '@/lib/work-shell-routes';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList, workDetail } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient() {
  return new BrowserClient();
}

function WorkRouteTree() {
  const { pathname } = useLocation();
  const workShell = isWorkShellRoute(pathname);
  return (
    <div
      className={cn(
        'mx-auto w-full',
        workShell ? 'max-w-none px-0 py-0' : 'max-w-[1200px] px-4 py-6',
      )}
      data-testid={workShell ? 'main-work-shell' : 'main-standard'}
    >
      <Routes>
        <Route path="works/chapters" element={<div data-testid="works-chapters-list">Chapters list</div>} />
        <Route path="works/:workId" element={<WorkShellLayout />}>
          <Route index element={<Navigate to="outline" replace />} />
          <Route path="outline" element={<div data-testid="outline-route">Outline</div>} />
          <Route path="chapters" element={<div data-testid="chapters-route">Chapters</div>} />
        </Route>
      </Routes>
    </div>
  );
}

function useWorkRouteHandlers() {
  useHandlers(
    worksList([
      {
        work_id: 'work-a',
        title: 'Alpha Novel',
        status: 'active',
        intake_status: 'complete',
        primary_preset_id: 'novel-writing',
        updated_at: '2026-06-24T00:00:00Z',
      },
    ]),
    workDetail('work-a', {
      title: 'Alpha Novel',
      status: 'active',
      work_profile: 'novel',
      primary_preset_id: 'novel-writing',
      updated_at: '2026-06-24T00:00:00Z',
    }),
  );
}

describe('work route nesting (T2)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
    vi.spyOn(window, 'matchMedia').mockImplementation((query: string) => ({
      matches: query.includes('961px'),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
  });

  it('redirects /works/:workId to outline inside WorkShellLayout', async () => {
    useWorkRouteHandlers();

    renderInApp(<WorkRouteTree />, {
      client: makeClient(),
      initialRouterEntries: ['/works/work-a'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-shell-layout')).toBeInTheDocument();
    });
    expect(screen.getByTestId('outline-route')).toBeInTheDocument();
    expect(screen.getByTestId('main-work-shell')).toBeInTheDocument();
  });

  it('wraps chapters routes in the work shell with full-width main', async () => {
    useWorkRouteHandlers();

    renderInApp(<WorkRouteTree />, {
      client: makeClient(),
      initialRouterEntries: ['/works/work-a/chapters'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-shell-layout')).toBeInTheDocument();
    });
    expect(screen.getByTestId('chapters-route')).toBeInTheDocument();
    expect(screen.getByTestId('main-work-shell')).toBeInTheDocument();
  });

  it('keeps standard main width on the reserved /works/chapters sibling route (T3)', async () => {
    useWorkRouteHandlers();

    renderInApp(<WorkRouteTree />, {
      client: makeClient(),
      initialRouterEntries: ['/works/chapters'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('works-chapters-list')).toBeInTheDocument();
    });
    expect(screen.getByTestId('main-standard')).toBeInTheDocument();
    expect(screen.queryByTestId('main-work-shell')).not.toBeInTheDocument();
    expect(screen.queryByTestId('work-shell-layout')).not.toBeInTheDocument();
  });
});
