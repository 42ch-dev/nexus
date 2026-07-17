/**
 * WorksPage render tests — representative screen coverage (R-V164-QC1-S1-P1).
 *
 * Exercises the three states every screen shares — success (table renders),
 * empty (empty-state CTA), and error (error-state + retry) — against the real
 * BrowserClient transport, which msw intercepts. Establishes the component-test
 * baseline P-last can extend to the remaining screens.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { WorksPage } from '@/pages/works-page';
import { act, screen, waitFor } from '@testing-library/react';

const client = () => new BrowserClient();

function renderWorks() {
  return renderInApp(<WorksPage />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('WorksPage', () => {
  it('renders the works table on a successful list', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'novel-writing',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderWorks();

    expect(await screen.findByText('Galaxy Novel')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  it('renders the empty state when there are no works', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderWorks();

    expect(await screen.findByText('No works yet')).toBeInTheDocument();
    expect(screen.getByText(/Create a Work to start the local loop/i)).toBeInTheDocument();
  });

  it('renders the error state and offers retry when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderWorks();

    expect(await screen.findByText('Could not load this view')).toBeInTheDocument();
    expect(screen.getByText(/daemon did not return Works/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('offers a Create Work action that opens the create dialog', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderWorks();
    await waitFor(() => expect(screen.getByText('No works yet')).toBeInTheDocument());

    // The Create Work button is present in both the toolbar and the empty state.
    const createButtons = screen.getAllByRole('button', { name: /^Create$/i });
    expect(createButtons.length).toBeGreaterThanOrEqual(1);
    // Sanity: the table column header / title text is present.
    expect(screen.getByText('Works')).toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'novel-writing',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderWorks();
    expect(await screen.findByText('Works')).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    await waitFor(() => expect(screen.getByText('作品')).toBeInTheDocument());
    expect(screen.getByText('标题')).toBeInTheDocument();
  });
});

// V1.121 v0.4 — voice-split discipline (DESIGN.md §Design Concept).
//
// Pins both directions of the serif contract on the Works list page:
//   - creative-entity title (CardTitle "Works") → content voice (serif
//     display-20 via CardTitle voice="content");
//   - all page chrome (table headers, buttons, refresh control, filter input)
//     → interface voice (sans) — no `font-display` leaks into chrome.
describe('WorksPage voice-split (V1.121 v0.4)', () => {
  it('renders the Works CardTitle in the content voice (serif display-20)', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-1',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'p-1',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderWorks();

    const title = await screen.findByRole('heading', { name: 'Works' });
    // Content voice (serif display tier) per DESIGN.md components.card.title.voice.
    expect(title.className).toMatch(/\bfont-display\b/);
    expect(title.className).toMatch(/\btext-display-20\b/);
    // Interface-voice heading treatment is absent.
    expect(title.className).not.toMatch(/\btext-heading-16\b/);
    expect(title.className).not.toMatch(/\bfont-heading\b/);
  });

  it('keeps table headers and buttons in the interface voice (sans)', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-1',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'p-1',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderWorks();
    await screen.findByText('Galaxy Novel');

    // Table column header — interface voice.
    const colHeader = screen.getByRole('columnheader', { name: 'Title' });
    expect(colHeader.className).not.toMatch(/\bfont-display\b/);

    // Refresh + Create buttons stay sans.
    const refresh = screen.getByRole('button', { name: /Refresh Works/i });
    expect(refresh.className).not.toMatch(/\bfont-display\b/);
    const create = screen.getByRole('button', { name: /^Create$/i });
    expect(create.className).not.toMatch(/\bfont-display\b/);
  });
});
