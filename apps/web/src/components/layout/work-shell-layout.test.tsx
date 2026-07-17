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

function mockMatchMedia(matches: boolean) {
  vi.spyOn(window, 'matchMedia').mockImplementation((query: string) => ({
    matches: query.includes('961px') ? matches : false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

function renderShell(initialEntry = '/works/work-a/outline', desktop = false) {
  mockMatchMedia(desktop);
  return renderInApp(
    <Routes>
      <Route path="/works/:workId" element={<WorkShellLayout />}>
        <Route path="outline" element={<div data-testid="shell-outlet">Outline</div>} />
      </Route>
    </Routes>,
    { client: makeClient(), initialRouterEntries: [initialEntry] },
  );
}

describe('WorkShellLayout', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('establishes flex row shell with main outlet slot', async () => {
    useShellHandlers();
    renderShell('/works/work-a/outline', true);

    const shell = screen.getByTestId('work-shell-layout');
    expect(shell).toHaveClass('flex');
    expect(shell).toHaveClass('lg:flex-row');

    await waitFor(() => {
      expect(screen.getByTestId('shell-outlet')).toBeInTheDocument();
    });
    expect(screen.getByTestId('work-shell-main')).toBeInTheDocument();
  });

  it('renders desktop rail aside with the sheet-width token class', async () => {
    useShellHandlers();
    renderShell('/works/work-a/outline', true);

    await waitFor(() => {
      expect(screen.getByTestId('work-shell-rail-desktop')).toBeInTheDocument();
    });

    const rail = screen.getByTestId('work-shell-rail-desktop');
    // DESIGN.md components.sheet.width (min(100vw, 280px)) — the work-shell
    // right rail shares the sheet width token (V1.121 P2 T1).
    expect(rail).toHaveClass('w-sheet');
    expect(rail).toHaveClass('lg:flex');
  });

  it('exposes dialog disclosure state on the mobile open-rail control', async () => {
    const user = userEvent.setup();
    useShellHandlers();
    renderShell('/works/work-a/outline', false);

    const trigger = screen.getByTestId('work-shell-open-rail');
    expect(trigger).toHaveAttribute('aria-label', 'Show Works rail');
    expect(trigger).toHaveAttribute('aria-haspopup', 'dialog');
    expect(trigger).toHaveAttribute('aria-expanded', 'false');

    await user.click(trigger);
    await waitFor(() => {
      expect(trigger).toHaveAttribute('aria-expanded', 'true');
    });

    await user.keyboard('{Escape}');
    await waitFor(() => {
      expect(trigger).toHaveAttribute('aria-expanded', 'false');
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens the mobile end-sheet drawer from the header control', async () => {
    const user = userEvent.setup();
    useShellHandlers();
    renderShell('/works/work-a/outline', false);

    await user.click(screen.getByRole('button', { name: 'Show Works rail' }));

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByRole('heading', { name: 'Works' })).toBeInTheDocument();
    expect(within(dialog).getByTestId('work-rail')).toBeInTheDocument();
  });
});

describe('WorkShellLayout locale parity (V1.118 P2 T4)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
  });

  it('renders work-shell chrome copy in zh-CN', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    useShellHandlers();
    renderShell('/works/work-a/outline', true);

    await waitFor(() => {
      expect(screen.getByTestId('work-shell-rail-desktop')).toBeInTheDocument();
    });

    expect(screen.getByRole('complementary', { name: '作品侧栏' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '作品' })).toBeInTheDocument();
  });
});
