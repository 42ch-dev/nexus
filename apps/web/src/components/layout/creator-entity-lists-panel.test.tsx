import { http, HttpResponse } from 'msw';
import { describe, expect, it, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { CreatorEntityListsPanel } from './creator-entity-lists-panel';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

function makeClient() {
  return new BrowserClient();
}

function renderWithWork() {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    worksList([
      {
        work_id: 'work-alpha',
        title: 'Alpha Novel',
        status: 'active',
        intake_status: 'ready',
        primary_preset_id: 'preset-1',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]),
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
    http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
  );
  renderInApp(<CreatorEntityListsPanel />, {
    client: makeClient(),
    activeCreatorId: 'creator-a',
    initialRouterEntries: ['/works'],
  });
}

describe('CreatorEntityListsPanel (V1.132 P3 AC-8)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders Worlds and Works sections as right-side lists', async () => {
    renderWithWork();

    expect(await screen.findByTestId('creator-hub-entity-lists')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-entity-lists-worlds')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-entity-lists-works')).toBeInTheDocument();
    expect(
      await screen.findByTestId('creator-hub-entity-lists-works-row-work-alpha'),
    ).toHaveTextContent('Alpha Novel');
  });

  it('opens submenu on Work row ••• button click', async () => {
    const user = userEvent.setup();
    renderWithWork();

    await waitFor(() =>
      expect(
        screen.getByTestId('creator-hub-entity-lists-works-row-work-alpha'),
      ).toBeInTheDocument(),
    );

    const menuBtn = screen.getByRole('button', { name: /Open menu for Alpha Novel/i });
    await user.click(menuBtn);

    await waitFor(() =>
      expect(screen.getByRole('menu', { name: 'Row actions' })).toBeInTheDocument(),
    );
  });

  it('Rename item triggers inline edit on Work row', async () => {
    const user = userEvent.setup();
    renderWithWork();

    await waitFor(() =>
      expect(
        screen.getByTestId('creator-hub-entity-lists-works-row-work-alpha'),
      ).toBeInTheDocument(),
    );

    const menuBtn = screen.getByRole('button', { name: /Open menu for Alpha Novel/i });
    await user.click(menuBtn);

    await waitFor(() =>
      expect(screen.getByRole('menuitem', { name: /Rename/i })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('menuitem', { name: /Rename/i }));

    await waitFor(() =>
      expect(screen.getByTestId('creator-entity-rename-input')).toBeInTheDocument(),
    );
  });

  it('Rename mutation calls PATCH with correct title on Enter', async () => {
    let patchPayload: unknown;
    useHandlers(
      http.patch('/v1/daemon/works/:workId', async ({ request, params }) => {
        patchPayload = { workId: params.workId, body: await request.json() };
        return HttpResponse.json({});
      }),
    );

    const user = userEvent.setup();
    renderWithWork();

    await waitFor(() =>
      expect(
        screen.getByTestId('creator-hub-entity-lists-works-row-work-alpha'),
      ).toBeInTheDocument(),
    );

    const menuBtn = screen.getByRole('button', { name: /Open menu for Alpha Novel/i });
    await user.click(menuBtn);
    await user.click(await screen.findByRole('menuitem', { name: /Rename/i }));

    const input = await screen.findByTestId('creator-entity-rename-input');
    await user.clear(input);
    await user.type(input, 'Beta Novel');
    await user.keyboard('{Enter}');

    await waitFor(() => {
      expect(patchPayload).toEqual({
        workId: 'work-alpha',
        body: { title: 'Beta Novel' },
      });
    });
  });
});
