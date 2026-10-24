/**
 * WorksInspirationPage — closed-loop coverage for the Inspiration Pool
 * (v1.222 P1 T2) against the real `BrowserClient` + msw.
 *
 * Pins the four service calls the page drives: list, add (`{title}` only),
 * promote (`{item_id}` only — `set_default` is inert server-side) and archive,
 * plus the promote landing route, the status-driven row actions, the verbatim
 * rendering of an unexpected status, and the page-owned mutation error toast.
 */
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { Route, Routes, useParams } from 'react-router';
import { beforeEach, describe, expect, it } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { WorksInspirationPage } from '@/pages/works-inspiration-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { WorkInspirationItem, WorkInspirationListResponse } from '@42ch/nexus-contracts';

const LIST_ROUTE = '/v1/daemon/works/pool/inspiration';

function makeItem(over: Partial<WorkInspirationItem> = {}): WorkInspirationItem {
  return {
    item_id: 'insp-1',
    rel_path: 'Pool/Ideas/insp-1.md',
    title: 'The drowned orchestra',
    status: 'idea',
    created_at: '2026-10-20T00:00:00Z',
    ...over,
  };
}

function listResponse(items: WorkInspirationItem[]): WorkInspirationListResponse {
  return { items, total: items.length, limit: 20, offset: 0 };
}

function OutlineProbe() {
  const { workId } = useParams<{ workId: string }>();
  return <p data-testid="outline-probe">{workId}</p>;
}

function renderPage() {
  return renderInApp(
    <Routes>
      <Route path="/works/inspiration" element={<WorksInspirationPage />} />
      <Route path="/works/:workId/outline" element={<OutlineProbe />} />
    </Routes>,
    { client: new BrowserClient(), initialRouterEntries: ['/works/inspiration'] },
  );
}

describe('WorksInspirationPage', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('en');
  });

  it('renders the pool with status-driven actions and a back link', async () => {
    useHandlers(
      http.get(LIST_ROUTE, () =>
        HttpResponse.json(
          listResponse([
            makeItem({ item_id: 'insp-idea', title: 'The drowned orchestra' }),
            makeItem({
              item_id: 'insp-promoted',
              title: 'A cartographer of vanished roads',
              status: 'promoted',
              promoted_work_id: 'work-42',
            }),
            makeItem({ item_id: 'insp-archived', title: 'Two rival archivists', status: 'archived' }),
            makeItem({ item_id: 'insp-odd', title: 'Unrecognised status', status: 'quarantined' }),
          ]),
        ),
      ),
    );

    renderPage();

    const rows = await screen.findAllByTestId('inspiration-pool-row');
    expect(rows.map((row) => row.getAttribute('data-status'))).toEqual([
      'idea',
      'promoted',
      'archived',
      'quarantined',
    ]);

    const [idea, promoted, archived, unexpected] = rows;
    expect(within(idea).getByTestId('inspiration-pool-row-promote')).toBeEnabled();
    expect(within(idea).getByTestId('inspiration-pool-row-archive')).toBeEnabled();

    expect(within(promoted).getByRole('link', { name: 'Open Work' })).toHaveAttribute(
      'href',
      '/works/work-42/outline',
    );
    expect(within(promoted).queryByTestId('inspiration-pool-row-promote')).toBeNull();

    expect(within(archived).queryAllByRole('button')).toHaveLength(0);
    expect(within(archived).queryAllByRole('link')).toHaveLength(0);

    // An unexpected status renders verbatim and offers no transition.
    expect(within(unexpected).getByText('quarantined')).toBeInTheDocument();
    expect(within(unexpected).queryAllByRole('button')).toHaveLength(0);
    expect(within(unexpected).queryAllByRole('link')).toHaveLength(0);

    expect(screen.getByTestId('inspiration-back-link')).toHaveAttribute('href', '/works');
    expect(screen.getByTestId('inspiration-pool-count')).toHaveTextContent('4 / 4');
    expect(screen.getAllByTestId('inspiration-pool-row-added')[0]).toHaveTextContent(/^Added:/);
  });

  it('adds a trimmed title and refreshes the pool', async () => {
    const addBodies: unknown[] = [];
    let items = [makeItem({ item_id: 'insp-1', title: 'First idea' })];
    useHandlers(
      http.get(LIST_ROUTE, () => HttpResponse.json(listResponse(items))),
      http.post(LIST_ROUTE, async ({ request }) => {
        addBodies.push(await request.json());
        items = [...items, makeItem({ item_id: 'insp-2', title: 'Second idea' })];
        return HttpResponse.json(
          { item_id: 'insp-2', rel_path: 'Pool/Ideas/insp-2.md' },
          { status: 201 },
        );
      }),
    );

    renderPage();

    const input = await screen.findByLabelText('Title');
    fireEvent.change(input, { target: { value: '  Second idea  ' } });
    fireEvent.click(screen.getByTestId('inspiration-pool-add-submit'));

    await waitFor(() => expect(addBodies).toEqual([{ title: 'Second idea' }]));
    expect(await screen.findByText('Second idea')).toBeInTheDocument();
    expect(input).toHaveValue('');
  });

  it('promotes with item_id only and lands on the created Work outline', async () => {
    const promoteBodies: unknown[] = [];
    useHandlers(
      http.get(LIST_ROUTE, () => HttpResponse.json(listResponse([makeItem()]))),
      http.post(`${LIST_ROUTE}/promote`, async ({ request }) => {
        promoteBodies.push(await request.json());
        return HttpResponse.json({ work_id: 'work-created', pool_entry_id: 'pool-1' });
      }),
    );

    renderPage();

    const [row] = await screen.findAllByTestId('inspiration-pool-row');
    fireEvent.click(within(row).getByTestId('inspiration-pool-row-promote'));

    // `set_default` must never be sent — it is inert server-side.
    await waitFor(() => expect(promoteBodies).toEqual([{ item_id: 'insp-1' }]));
    expect(await screen.findByTestId('outline-probe')).toHaveTextContent('work-created');
  });

  it('archives by item_id and refreshes the pool', async () => {
    const archiveBodies: unknown[] = [];
    let items = [makeItem({ item_id: 'insp-1', title: 'First idea', status: 'promoted' })];
    useHandlers(
      http.get(LIST_ROUTE, () => HttpResponse.json(listResponse(items))),
      http.post(`${LIST_ROUTE}/archive`, async ({ request }) => {
        archiveBodies.push(await request.json());
        items = [{ ...items[0], status: 'archived' }];
        return HttpResponse.json(items[0]);
      }),
    );

    renderPage();

    const [row] = await screen.findAllByTestId('inspiration-pool-row');
    fireEvent.click(within(row).getByTestId('inspiration-pool-row-archive'));

    await waitFor(() => expect(archiveBodies).toEqual([{ item_id: 'insp-1' }]));
    await waitFor(() => expect(row).toHaveAttribute('data-status', 'archived'));
    expect(within(row).queryAllByRole('button')).toHaveLength(0);
  });

  it('toasts a mutation failure on the page', async () => {
    useHandlers(
      http.get(LIST_ROUTE, () => HttpResponse.json(listResponse([makeItem()]))),
      http.post(`${LIST_ROUTE}/archive`, () =>
        HttpResponse.json({ error: 'archive failed' }, { status: 500 }),
      ),
    );

    renderPage();

    const [row] = await screen.findAllByTestId('inspiration-pool-row');
    fireEvent.click(within(row).getByTestId('inspiration-pool-row-archive'));

    expect(await screen.findByText('Request failed')).toBeInTheDocument();
    // The row stays on screen mid-failure; no navigation, no silent drop.
    expect(row).toHaveAttribute('data-status', 'idea');
  });

  it('titles the surface "Inspiration Pool" / "灵感池" (compass D8)', async () => {
    useHandlers(http.get(LIST_ROUTE, () => HttpResponse.json(listResponse([]))));

    const en = renderPage();
    expect(
      await screen.findByRole('heading', { name: 'Inspiration Pool' }),
    ).toBeInTheDocument();
    en.unmount();

    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    renderPage();
    expect(await screen.findByRole('heading', { name: '灵感池' })).toBeInTheDocument();
  });
});
