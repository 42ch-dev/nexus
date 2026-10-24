/**
 * Inspiration pool query layer — key shape, page size and the exact
 * invalidation set (v1.222 P1 T1).
 *
 * The stale-Works-list risk on promote is the reason these cases exist: promote
 * is a server-side Work create, so it must invalidate `works.lists()` in
 * addition to `inspiration.lists()`. Add/archive must not touch `works`.
 */
import { fireEvent, screen, waitFor } from '@testing-library/react';
import type { QueryClient } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_PAGE_SIZE,
  useAddWorkInspiration,
  useArchiveWorkInspiration,
  usePromoteWorkInspiration,
  useWorkInspiration,
} from '@/api/queries';
import type { NexusClient } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { makeQueryClient, renderInApp } from '@/test/test-providers';

function makeClient() {
  const list = vi.fn(async (_query?: { status?: string; limit?: number; offset?: number }) => ({
    items: [],
    total: 3,
    limit: DEFAULT_PAGE_SIZE,
    offset: 0,
  }));
  const add = vi.fn(async (_request: { title: string }) => ({
    item_id: 'i1',
    rel_path: 'Pool/Ideas/i1.md',
  }));
  const promote = vi.fn(async (_request: { item_id: string }) => ({
    work_id: 'w1',
    pool_entry_id: 'p1',
  }));
  const archive = vi.fn(async (_request: { item_id: string }) => ({
    item_id: 'i2',
    rel_path: 'Pool/Ideas/i2.md',
    title: 'Two',
    status: 'archived',
    created_at: '2026-10-01T00:00:00Z',
  }));
  const client = {
    listWorkInspiration: list,
    addWorkInspiration: add,
    promoteWorkInspiration: promote,
    archiveWorkInspiration: archive,
  } as unknown as NexusClient;
  return { client, list, add, promote, archive };
}

/** Mounts the list hook plus one trigger per mutation. */
function Harness() {
  const list = useWorkInspiration();
  const add = useAddWorkInspiration();
  const promote = usePromoteWorkInspiration();
  const archive = useArchiveWorkInspiration();
  return (
    <div>
      <span data-testid="total">{list.data?.total ?? 'none'}</span>
      <button type="button" onClick={() => add.mutate('Idea one')}>
        add
      </button>
      <button type="button" onClick={() => promote.mutate('i1')}>
        promote
      </button>
      <button type="button" onClick={() => archive.mutate('i2')}>
        archive
      </button>
    </div>
  );
}

/**
 * Seeds one entry at each invalidatable key so `isInvalidated` is observable.
 * These seeds have no observer, so the test client's `gcTime: 0` would collect
 * them before the mutation settles.
 */
function seedInvalidatable(qc: QueryClient): void {
  qc.setDefaultOptions({ queries: { retry: false, gcTime: Infinity, staleTime: 0 } });
  qc.setQueryData(queryKeys.inspiration.lists(), { seed: 'inspiration' });
  qc.setQueryData(queryKeys.works.lists(), { seed: 'works' });
}

function invalidated(qc: QueryClient, key: readonly unknown[]): boolean {
  return qc.getQueryState(key)?.isInvalidated === true;
}

describe('inspiration pool query layer', () => {
  it('reads one DEFAULT_PAGE_SIZE page and exposes the server total', async () => {
    const { client, list } = makeClient();
    const qc = makeQueryClient();
    renderInApp(<Harness />, { client, queryClient: qc });

    await waitFor(() => expect(screen.getByTestId('total').textContent).toBe('3'));
    expect(list).toHaveBeenCalledWith({ limit: DEFAULT_PAGE_SIZE });
    expect(
      qc.getQueryData(queryKeys.inspiration.list({ limit: DEFAULT_PAGE_SIZE })),
    ).toBeDefined();
  });

  it('add invalidates the inspiration lists only', async () => {
    const { client } = makeClient();
    const qc = makeQueryClient();
    seedInvalidatable(qc);
    renderInApp(<Harness />, { client, queryClient: qc });

    fireEvent.click(screen.getByRole('button', { name: 'add' }));

    await waitFor(() => expect(invalidated(qc, queryKeys.inspiration.lists())).toBe(true));
    expect(invalidated(qc, queryKeys.works.lists())).toBe(false);
  });

  it('archive invalidates the inspiration lists only', async () => {
    const { client } = makeClient();
    const qc = makeQueryClient();
    seedInvalidatable(qc);
    renderInApp(<Harness />, { client, queryClient: qc });

    fireEvent.click(screen.getByRole('button', { name: 'archive' }));

    await waitFor(() => expect(invalidated(qc, queryKeys.inspiration.lists())).toBe(true));
    expect(invalidated(qc, queryKeys.works.lists())).toBe(false);
  });

  it('promote invalidates inspiration lists AND works lists', async () => {
    const { client } = makeClient();
    const qc = makeQueryClient();
    seedInvalidatable(qc);
    renderInApp(<Harness />, { client, queryClient: qc });

    fireEvent.click(screen.getByRole('button', { name: 'promote' }));

    await waitFor(() => expect(invalidated(qc, queryKeys.inspiration.lists())).toBe(true));
    expect(invalidated(qc, queryKeys.works.lists())).toBe(true);
  });

  it('promote sends item_id only — never the inert set_default', async () => {
    const { client, promote } = makeClient();
    renderInApp(<Harness />, { client, queryClient: makeQueryClient() });

    fireEvent.click(screen.getByRole('button', { name: 'promote' }));

    await waitFor(() => expect(promote).toHaveBeenCalledTimes(1));
    expect(promote.mock.calls[0][0]).toEqual({ item_id: 'i1' });
  });
});
