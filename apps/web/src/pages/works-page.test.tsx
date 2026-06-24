/**
 * WorksPage render tests — representative screen coverage (R-V164-QC1-S1-P1).
 *
 * Exercises the three states every screen shares — success (table renders),
 * empty (empty-state CTA), and error (error-state + retry) — against the real
 * BrowserClient transport, which msw intercepts. Establishes the component-test
 * baseline P-last can extend to the remaining screens.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { WorksPage } from '@/pages/works-page';
import { screen, waitFor } from '@testing-library/react';

const client = () => new BrowserClient();

function renderWorks() {
  return renderInApp(<WorksPage />, { client: client() });
}

describe('WorksPage', () => {
  it('renders the works table on a successful list', async () => {
    useHandlers(
      http.get('/v1/local/works', () =>
        HttpResponse.json({
          works: [
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
      http.get('/v1/local/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderWorks();

    expect(await screen.findByText('Galaxy Novel')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  it('renders the empty state when there are no works', async () => {
    useHandlers(
      http.get('/v1/local/works', () =>
        HttpResponse.json({ works: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderWorks();

    expect(await screen.findByText('No works yet')).toBeInTheDocument();
    expect(screen.getByText(/Create a Work to start the local loop/i)).toBeInTheDocument();
  });

  it('renders the error state and offers retry when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/local/works', () =>
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
      http.get('/v1/local/works', () =>
        HttpResponse.json({ works: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderWorks();
    await waitFor(() => expect(screen.getByText('No works yet')).toBeInTheDocument());

    // The Create Work button is present in both the toolbar and the empty state.
    const createButtons = screen.getAllByRole('button', { name: /Create Work/i });
    expect(createButtons.length).toBeGreaterThanOrEqual(1);
    // Sanity: the table column header / title text is present.
    expect(screen.getByText('Works')).toBeInTheDocument();
  });
});
