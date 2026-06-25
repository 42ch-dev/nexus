/**
 * ChaptersPage render tests — structure table, inline edit, status progression,
 * and protected-chapter confirmation.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { Route, Routes } from 'react-router-dom';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { ChaptersPage } from '@/pages/chapters-page';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

const client = () => new BrowserClient();

function renderChapters(workId = 'w-123') {
  return renderInApp(
    <Routes>
      <Route path="works/:workId/chapters" element={<ChaptersPage />} />
    </Routes>,
    { client: client(), initialRouterEntries: [`/works/${workId}/chapters`] },
  );
}

function worksHandler() {
  return http.get('/v1/local/works', () =>
    HttpResponse.json({
      works: [{ work_id: 'w-123', title: 'Galaxy Novel', status: 'active', updated_at: '2026-06-25T00:00:00Z' }],
      pagination: { limit: 20, has_more: false },
    }),
  );
}

describe('ChaptersPage', () => {
  it('renders the chapter structure table', async () => {
    useHandlers(
      worksHandler(),
      http.get('/v1/local/works/:workId/chapters', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              chapter: 1,
              volume: 1,
              title: null,
              slug: 'ch01',
              planned_word_count: 4000,
              actual_word_count: 1200,
              status: 'not_started',
              created_at: '2026-06-25T00:00:00Z',
              updated_at: '2026-06-25T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderChapters();
    expect(await screen.findByText('Chapter Structure')).toBeInTheDocument();
    expect(await screen.findByText('ch01')).toBeInTheDocument();
    expect(screen.getByText('4,000')).toBeInTheDocument();
    expect(screen.getByText('1,200')).toBeInTheDocument();
  });

  it('shows the empty state when the Work has no chapters', async () => {
    useHandlers(
      worksHandler(),
      http.get('/v1/local/works/:workId/chapters', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderChapters();
    expect(await screen.findByText('No chapters yet')).toBeInTheDocument();
  });

  it('shows the error state when the daemon fails', async () => {
    useHandlers(
      worksHandler(),
      http.get('/v1/local/works/:workId/chapters', () =>        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderChapters();
    expect(await screen.findByText('Could not load this view')).toBeInTheDocument();
    expect(screen.getByText(/Could not load chapters for this Work/i)).toBeInTheDocument();
  });

  it('advances status from not_started to outlined', async () => {
    let patched = false;
    useHandlers(
      worksHandler(),
      http.get('/v1/local/works/:workId/chapters', () =>        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              chapter: 1,
              volume: 1,
              slug: 'ch01',
              planned_word_count: 4000,
              status: 'not_started',
              created_at: '2026-06-25T00:00:00Z',
              updated_at: '2026-06-25T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.patch('/v1/local/works/:workId/chapters/:n', async ({ request }) => {
        const body = (await request.json()) as { status?: string };
        patched = body.status === 'outlined';
        return HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          slug: 'ch01',
          planned_word_count: 4000,
          status: 'outlined',
          can_edit_outline: true,
          can_edit_structure: true,
          body_read_only: true,
          protection: { level: 'none', reason: '' },
          created_at: '2026-06-25T00:00:00Z',
          updated_at: '2026-06-25T00:00:00Z',
        });
      }),
    );

    renderChapters();
    const button = await screen.findByRole('button', { name: /Mark outlined/i });
    await userEvent.click(button);
    await waitFor(() => expect(patched).toBe(true));
  });

  it('inline-edits slug and planned word count', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      worksHandler(),
      http.get('/v1/local/works/:workId/chapters', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              chapter: 1,
              volume: 1,
              slug: 'ch01',
              planned_word_count: 4000,
              status: 'not_started',
              created_at: '2026-06-25T00:00:00Z',
              updated_at: '2026-06-25T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.patch('/v1/local/works/:workId/chapters/:n', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          slug: (receivedBody as { slug?: string }).slug ?? 'ch01',
          planned_word_count: (receivedBody as { planned_word_count?: number }).planned_word_count ?? 4000,
          status: 'not_started',
          can_edit_outline: true,
          can_edit_structure: true,
          body_read_only: true,
          protection: { level: 'none', reason: '' },
          created_at: '2026-06-25T00:00:00Z',
          updated_at: '2026-06-25T00:00:00Z',
        });
      }),
    );

    renderChapters();
    await screen.findByText('ch01');
    await userEvent.click(screen.getByRole('button', { name: /Edit structure/i }));

    const slugInput = screen.getByLabelText('Slug');
    await userEvent.clear(slugInput);
    await userEvent.type(slugInput, 'opening-scene');

    const wcInput = screen.getByLabelText('Planned word count');
    await userEvent.clear(wcInput);
    await userEvent.type(wcInput, '4500');

    await userEvent.click(screen.getByRole('button', { name: /Save edits/i }));
    await waitFor(() =>
      expect(receivedBody).toMatchObject({ slug: 'opening-scene', planned_word_count: 4500 }),
    );
  });

  it('asks for confirmation before editing a finalized chapter', async () => {
    useHandlers(
      worksHandler(),
      http.get('/v1/local/works/:workId/chapters', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              chapter: 1,
              volume: 1,
              slug: 'ch01',
              planned_word_count: 4000,
              status: 'finalized',
              created_at: '2026-06-25T00:00:00Z',
              updated_at: '2026-06-25T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.patch('/v1/local/works/:workId/chapters/:n', async ({ request }) => {
        const body = await request.json();
        return HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          slug: (body as { slug?: string }).slug ?? 'ch01',
          planned_word_count: (body as { planned_word_count?: number }).planned_word_count ?? 4000,
          status: 'finalized',
          can_edit_outline: true,
          can_edit_structure: true,
          body_read_only: true,
          protection: { level: 'confirm_structure_edit', reason: 'Chapter is finalized.' },
          created_at: '2026-06-25T00:00:00Z',
          updated_at: '2026-06-25T00:00:00Z',
        });
      }),
    );

    renderChapters();
    await screen.findByText('Finalized');
    await userEvent.click(screen.getByRole('button', { name: /Edit structure/i }));
    await userEvent.click(screen.getByRole('button', { name: /Save edits/i }));

    expect(await screen.findByText('Confirm structural edit')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Confirm Edit/i })).toBeInTheDocument();
  });
});
