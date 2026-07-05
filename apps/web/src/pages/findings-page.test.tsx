/**
 * FindingsPage render + interaction tests — V1.91 P1 batch triage coverage.
 *
 * Exercises multi-select, bulk status transition, bulk target_executor
 * assignment, and client-side CSV export against the real BrowserClient + msw.
 */
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { FindingsPage } from '@/pages/findings-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { FindingDetailResponse, ListFindingsResponse } from '@42ch/nexus-contracts';

const WORK_ID = 'w-1';

function makeFinding(over: Partial<FindingDetailResponse> = {}): FindingDetailResponse {
  return {
    finding_id: 'f1',
    work_id: WORK_ID,
    chapter: 1,
    severity: 'minor',
    status: 'open',
    title: 'Pacing',
    description: 'd',
    target_executor: 'none',
    kind: 'craft',
    rule_suggestion: 'Add more tension.',
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}

const findingsList: ListFindingsResponse = {
  items: [
    makeFinding({ finding_id: 'f1', title: 'Pacing', status: 'open', target_executor: 'none' }),
    makeFinding({ finding_id: 'f2', title: 'Grammar', status: 'open', target_executor: 'none' }),
  ],
  pagination: { limit: 20, has_more: false },
};

function renderFindings() {
  return renderInApp(<FindingsPage />, { client: new BrowserClient() });
}

async function selectWork() {
  const select = screen.getByLabelText(/work/i) as HTMLSelectElement;
  await waitFor(() => expect(select.options.length).toBeGreaterThan(1));
  fireEvent.change(select, { target: { value: WORK_ID } });
  await waitFor(() => expect(screen.getByText('Pacing')).toBeInTheDocument());
}

describe('FindingsPage', () => {
  it('renders the findings table with row checkboxes', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [{ work_id: WORK_ID, title: 'Test Novel', updated_at: '2026-01-01T00:00:00Z' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId/findings', () => HttpResponse.json(findingsList)),
    );

    renderFindings();
    await selectWork();

    expect(screen.getAllByRole('checkbox').length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText('Grammar')).toBeInTheDocument();
  });

  it('shows the bulk action bar and selection count after selecting rows', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [{ work_id: WORK_ID, title: 'Test Novel', updated_at: '2026-01-01T00:00:00Z' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId/findings', () => HttpResponse.json(findingsList)),
    );

    renderFindings();
    await selectWork();

    const rowChecks = screen.getAllByRole('checkbox');
    // Header checkbox + two row checkboxes.
    expect(rowChecks.length).toBeGreaterThanOrEqual(3);
    fireEvent.click(rowChecks[1]);

    const bar = await screen.findByTestId('findings-bulk-bar');
    expect(bar).toHaveTextContent('1 selected');
    expect(screen.getByLabelText(/set status for selected/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/assign target executor for selected/i)).toBeInTheDocument();
  });

  it('selects and clears all rows via the header checkbox', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [{ work_id: WORK_ID, title: 'Test Novel', updated_at: '2026-01-01T00:00:00Z' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId/findings', () => HttpResponse.json(findingsList)),
    );

    renderFindings();
    await selectWork();

    const headerCheck = screen.getByLabelText(/select all visible findings/i);
    fireEvent.click(headerCheck);

    expect(await screen.findByTestId('findings-bulk-bar')).toHaveTextContent('2 selected');

    fireEvent.click(headerCheck);
    await waitFor(() =>
      expect(screen.queryByTestId('findings-bulk-bar')).not.toBeInTheDocument(),
    );
  });

  it('calls the batch endpoint when bulk-setting status', async () => {
    let batchBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [{ work_id: WORK_ID, title: 'Test Novel', updated_at: '2026-01-01T00:00:00Z' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId/findings', () => HttpResponse.json(findingsList)),
      http.patch('/v1/daemon/findings/batch', async ({ request }) => {
        batchBody = await request.json();
        return HttpResponse.json({ updated: 2 });
      }),
    );

    renderFindings();
    await selectWork();

    fireEvent.click(screen.getByLabelText(/select all visible findings/i));
    fireEvent.change(screen.getByLabelText(/set status for selected/i), {
      target: { value: 'triaged' },
    });

    await waitFor(() =>
      expect(batchBody).toEqual({
        finding_ids: ['f1', 'f2'],
        patch: { status: 'triaged' },
      }),
    );
  });

  it('calls the batch endpoint when bulk-assigning target_executor', async () => {
    let batchBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [{ work_id: WORK_ID, title: 'Test Novel', updated_at: '2026-01-01T00:00:00Z' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId/findings', () => HttpResponse.json(findingsList)),
      http.patch('/v1/daemon/findings/batch', async ({ request }) => {
        batchBody = await request.json();
        return HttpResponse.json({ updated: 2 });
      }),
    );

    renderFindings();
    await selectWork();

    fireEvent.click(screen.getByLabelText(/select all visible findings/i));
    fireEvent.change(screen.getByLabelText(/assign target executor for selected/i), {
      target: { value: 'write' },
    });

    await waitFor(() =>
      expect(batchBody).toEqual({
        finding_ids: ['f1', 'f2'],
        patch: { target_executor: 'write' },
      }),
    );
  });

  it('exports the filtered findings as CSV with the documented columns', async () => {
    const createObjectURL = vi.fn(() => 'blob:test');
    const revokeObjectURL = vi.fn();
    const anchorClick = vi.fn();
    URL.createObjectURL = createObjectURL;
    URL.revokeObjectURL = revokeObjectURL;

    // Capture the created anchor to inspect download attributes.
    const originalCreateElement = document.createElement.bind(document);
    let capturedAnchor: HTMLAnchorElement | null = null;
    document.createElement = ((tagName: string, options?: ElementCreationOptions) => {
      const el = originalCreateElement(tagName, options);
      if (tagName === 'a') {
        capturedAnchor = el as HTMLAnchorElement;
        el.addEventListener('click', anchorClick);
      }
      return el;
    }) as typeof document.createElement;

    try {
      useHandlers(
        http.get('/v1/daemon/works', () =>
          HttpResponse.json({
            items: [
              { work_id: WORK_ID, title: 'Test Novel', updated_at: '2026-01-01T00:00:00Z' },
            ],
            pagination: { limit: 20, has_more: false },
          }),
        ),
        http.get('/v1/daemon/works/:workId/findings', () => HttpResponse.json(findingsList)),
      );

      renderFindings();
      await selectWork();

      fireEvent.click(screen.getByRole('button', { name: /export/i }));

      await waitFor(() => expect(anchorClick).toHaveBeenCalledTimes(1));
      expect(capturedAnchor).not.toBeNull();
      expect(capturedAnchor!.download).toMatch(/findings-w-1-\d+\.csv/);

      const [blob] = createObjectURL.mock.calls[0] as [Blob];
      const csv = await new Promise<string>((resolve) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.readAsText(blob);
      });
      const lines = csv.split('\n');
      expect(lines[0]).toBe('id,title,status,kind,severity,target_executor,created_at,rule_suggestion');
      expect(lines.length).toBe(3); // header + 2 rows
      expect(lines[1]).toContain('f1');
      expect(lines[1]).toContain('Pacing');
      expect(lines[2]).toContain('f2');
    } finally {
      document.createElement = originalCreateElement;
    }
  });
});
