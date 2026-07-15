/**
 * WorkRail — list + metadata preview smoke tests (V1.118 P2 T1).
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

import { WorkRail } from './work-rail';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList, workDetail } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient() {
  return new BrowserClient();
}

const workA = {
  work_id: 'work-a',
  title: 'Alpha Novel',
  status: 'active',
  intake_status: 'complete',
  primary_preset_id: 'novel-writing',
  updated_at: '2026-06-24T00:00:00Z',
};

const workB = {
  work_id: 'work-b',
  title: 'Beta Essay',
  status: 'draft',
  intake_status: 'pending',
  primary_preset_id: 'essay-writing',
  updated_at: '2026-06-20T00:00:00Z',
};

function useRailHandlers() {
  useHandlers(
    worksList([workA, workB]),
    workDetail('work-a', {
      title: 'Alpha Novel',
      status: 'active',
      work_profile: 'novel',
      primary_preset_id: 'novel-writing',
      updated_at: '2026-06-24T00:00:00Z',
    }),
  );
}

function renderRail(initialEntry = '/works/work-a/outline') {
  return renderInApp(
    <Routes>
      <Route path="/works/:workId/outline" element={<WorkRail />} />
    </Routes>,
    { client: makeClient(), initialRouterEntries: [initialEntry] },
  );
}

describe('WorkRail', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders the works list with the current work highlighted on outline', async () => {
    useRailHandlers();
    renderRail();

    await waitFor(() => {
      expect(screen.getByTestId('work-rail-item-work-a')).toBeInTheDocument();
    });

    expect(screen.getByTestId('work-rail-item-work-b')).toBeInTheDocument();
    expect(screen.getByTestId('work-rail-item-work-a')).toHaveAttribute('aria-current', 'page');
    expect(screen.getByTestId('work-rail-item-work-b')).not.toHaveAttribute('aria-current');
  });

  it('does not set aria-current on the current work while on chapters', async () => {
    useRailHandlers();

    renderInApp(
      <Routes>
        <Route path="/works/:workId/chapters" element={<WorkRail />} />
      </Routes>,
      { client: makeClient(), initialRouterEntries: ['/works/work-a/chapters'] },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-rail-item-work-a')).toBeInTheDocument();
    });

    expect(screen.getByTestId('work-rail-item-work-a')).not.toHaveAttribute('aria-current');
  });

  it('shows metadata preview for the route-scoped work', async () => {
    useRailHandlers();
    renderRail();

    await waitFor(() => {
      expect(screen.getByTestId('work-rail-preview')).toBeInTheDocument();
    });

    const preview = screen.getByTestId('work-rail-preview');
    await waitFor(() => {
      expect(within(preview).getByRole('heading', { name: 'Alpha Novel' })).toBeInTheDocument();
    });
    expect(within(preview).getByText('Active')).toBeInTheDocument();
    expect(within(preview).getByText('novel-writing')).toBeInTheDocument();
  });

  it('navigates to outline when selecting another work', async () => {
    const user = userEvent.setup();
    useRailHandlers();

    renderInApp(
      <Routes>
        <Route path="/works/:workId/outline" element={<WorkRail />} />
        <Route path="/works/:workId/chapters" element={<div data-testid="chapters-route">Chapters</div>} />
      </Routes>,
      { client: makeClient(), initialRouterEntries: ['/works/work-a/outline'] },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-rail-item-work-b')).toBeInTheDocument();
    });

    await user.click(screen.getByTestId('work-rail-item-work-b'));

    await waitFor(() => {
      expect(screen.getByTestId('work-rail-item-work-b')).toHaveAttribute('aria-current', 'page');
    });
  });

  it('invokes onWorkSelect after navigation', async () => {
    const user = userEvent.setup();
    const onWorkSelect = vi.fn();
    useRailHandlers();

    renderInApp(
      <Routes>
        <Route
          path="/works/:workId/outline"
          element={<WorkRail onWorkSelect={onWorkSelect} />}
        />
      </Routes>,
      { client: makeClient(), initialRouterEntries: ['/works/work-a/outline'] },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-rail-item-work-b')).toBeInTheDocument();
    });

    await user.click(screen.getByTestId('work-rail-item-work-b'));
    expect(onWorkSelect).toHaveBeenCalledTimes(1);
  });
});
