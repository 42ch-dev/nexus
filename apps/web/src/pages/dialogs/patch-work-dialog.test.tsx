/**
 * Patch Work dialog i18n + submit tests.
 *
 * Exercises the dialog form end-to-end against msw: open the dialog,
 * change a single field, submit, and assert the daemon receives a PATCH
 * `/v1/daemon/works/:workId` with only the delta field.
 */
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import { PatchWorkDialog } from '@/pages/dialogs/patch-work-dialog';
import type { WorkDetailResponse } from '@42ch/nexus-contracts';

const workFixture: WorkDetailResponse = {
  work_id: 'w-123',
  status: 'draft',
  title: 'Patchable Work',
  long_term_goal: 'Finish the book',
  initial_idea: 'A story',
  inspiration_log: [],
  primary_preset_id: 'preset-abc',
  schedule_ids: [],
  created_at: '2026-06-25T00:00:00Z',
  updated_at: '2026-06-25T00:00:00Z',
  current_stage: 'drafting',
  stage_status: 'active',
  intake_status: 'completed',
  current_chapter: 0,
  auto_chain_enabled: false,
  auto_chain_interrupted: false,
  auto_review_master_on_timeout: false,
};

function renderDialog(work = workFixture) {
  const onOpenChange = vi.fn();
  renderInApp(<PatchWorkDialog work={work} open onOpenChange={onOpenChange} />, {
    client: new BrowserClient(),
  });
  return { onOpenChange };
}

describe('PatchWorkDialog', () => {
  it('renders translated labels and pre-fills current values', () => {
    renderDialog();

    expect(screen.getByRole('heading', { name: 'Update Work' })).toBeInTheDocument();
    expect(screen.getByLabelText(/^Status$/i)).toHaveValue('draft');
    expect(screen.getByLabelText(/^Intake status$/i)).toHaveValue('completed');
    expect(screen.getByLabelText(/^Current stage$/i)).toHaveValue('drafting');
    expect(screen.getByLabelText(/^Stage status$/i)).toHaveValue('active');
    expect(
      screen.getByText('Statuses are free-text in the runtime (e.g. intake, active, completed, paused).'),
    ).toBeInTheDocument();
  });

  it('submits only the changed field as a PATCH /v1/daemon/works/:workId', async () => {
    const user = userEvent.setup();
    let patchedBody: unknown = null;
    useHandlers(
      http.patch('/v1/daemon/works/:workId', async ({ request }) => {
        patchedBody = await request.json();
        return HttpResponse.json({ ...workFixture, status: 'active' });
      }),
    );

    const { onOpenChange } = renderDialog();

    await user.clear(screen.getByLabelText(/^Status$/i));
    await user.type(screen.getByLabelText(/^Status$/i), 'active');
    await user.click(screen.getByRole('button', { name: /^Update$/i }));

    await waitFor(() => expect(patchedBody).not.toBeNull());
    expect(patchedBody).toEqual({ status: 'active' });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it('closes without calling PATCH when no fields changed', async () => {
    const user = userEvent.setup();
    let requestCount = 0;
    useHandlers(
      http.patch('/v1/daemon/works/:workId', () => {
        requestCount += 1;
        return HttpResponse.json(workFixture);
      }),
    );

    const { onOpenChange } = renderDialog();

    await user.click(screen.getByRole('button', { name: /^Update$/i }));

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(requestCount).toBe(0);
  });

  it('closes when Cancel is clicked', async () => {
    const user = userEvent.setup();
    const { onOpenChange } = renderDialog();

    await user.click(screen.getByRole('button', { name: /^Cancel$/i }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
