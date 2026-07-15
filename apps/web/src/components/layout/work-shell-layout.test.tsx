/**
 * WorkShellLayout — structural smoke tests (V1.118 P2 T1).
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

import { WorkShellLayout } from './work-shell-layout';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList, workDetail } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient() {
  return new BrowserClient();
}

function useShellHandlers() {
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

function renderShell() {
  return renderInApp(
    <Routes>
      <Route path="/works/:workId" element={<WorkShellLayout />}>
        <Route path="outline" element={<div data-testid="shell-outlet">Outline</div>} />
      </Route>
    </Routes>,
    { client: makeClient(), initialRouterEntries: ['/works/work-a/outline'] },
  );
}

describe('WorkShellLayout', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
    vi.spyOn(window, 'matchMedia').mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
  });

  it('establishes flex row shell with main outlet slot', async () => {
    useShellHandlers();
    renderShell();

    const shell = screen.getByTestId('work-shell-layout');
    expect(shell).toHaveClass('flex');
    expect(shell).toHaveClass('lg:flex-row');

    await waitFor(() => {
      expect(screen.getByTestId('shell-outlet')).toBeInTheDocument();
    });
    expect(screen.getByTestId('work-shell-main')).toBeInTheDocument();
  });

  it('renders desktop rail aside with 280px width class', async () => {
    useShellHandlers();
    renderShell();

    await waitFor(() => {
      expect(screen.getByTestId('work-shell-rail-desktop')).toBeInTheDocument();
    });

    const rail = screen.getByTestId('work-shell-rail-desktop');
    expect(rail).toHaveClass('w-[280px]');
    expect(rail).toHaveClass('lg:flex');
  });

  it('opens the mobile end-sheet drawer from the header control', async () => {
    const user = userEvent.setup();
    useShellHandlers();
    renderShell();

    await user.click(screen.getByTestId('work-shell-open-rail'));

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByRole('heading', { name: 'Works' })).toBeInTheDocument();
    expect(within(dialog).getByTestId('work-rail')).toBeInTheDocument();
  });
});
