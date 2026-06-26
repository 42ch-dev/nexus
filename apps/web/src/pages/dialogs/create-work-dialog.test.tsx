/**
 * Create Work CRUD round-trip test (R-V164-QC1-S1-P1).
 *
 * Exercises the full write path end-to-end against msw: open the dialog, fill
 * the required fields, submit, and assert the daemon receives a well-formed
 * POST `/v1/local/works`. Also covers the W-1 error path — a 400 envelope
 * surfaces as a toast (the mutation's onError → useToast) and the dialog stays
 * open so the author can correct.
 */
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import { CreateWorkDialog } from '@/pages/dialogs/create-work-dialog';

function renderDialog() {
  const onCreated = vi.fn();
  const onOpenChange = vi.fn();
  // renderInApp mounts ToastProvider + Toaster (mirrors main.tsx) so the
  // mutation's onError → useToast → Toaster portal path is exercised live.
  renderInApp(<CreateWorkDialog open onOpenChange={onOpenChange} onCreated={onCreated} />, {
    client: new BrowserClient(),
  });
  return { onCreated, onOpenChange };
}

describe('CreateWorkDialog CRUD round-trip', () => {
  it('submits a well-formed POST /v1/local/works and calls onCreated', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.post('/v1/local/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-new', status: 'intake' });
      }),
    );

    const { onCreated, onOpenChange } = renderDialog();

    await user.type(screen.getByLabelText(/Title/i), 'My New Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Finish the first arc');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A heist in a floating city');
    await user.click(screen.getByRole('button', { name: /Create Work/i }));

    await waitFor(() => expect(onCreated).toHaveBeenCalledWith('w-new'));
    expect(postedBody).toEqual({
      title: 'My New Work',
      long_term_goal: 'Finish the first arc',
      initial_idea: 'A heist in a floating city',
      work_profile: 'novel',
    });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('sends the selected work_profile when the author changes it (V1.67 G1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.post('/v1/local/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-essay', status: 'intake' });
      }),
    );

    renderDialog();

    await user.type(screen.getByLabelText(/Title/i), 'Essay Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Publish a collection');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A meditation on cities');
    await user.selectOptions(screen.getByLabelText(/Work profile/i), 'essay');
    await user.click(screen.getByRole('button', { name: /Create Work/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    expect(postedBody).toMatchObject({ work_profile: 'essay' });
  });

  it('keeps the dialog open and shows a toast when the daemon returns a 400 envelope (W-1)', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/local/works', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'validation_failed', message: 'Initial idea is too short.' },
          },
          { status: 400 },
        ),
      ),
    );

    const { onCreated, onOpenChange } = renderDialog();

    await user.type(screen.getByLabelText(/Title/i), 'A Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'A goal');
    await user.type(screen.getByLabelText(/Initial idea/i), 'An idea');
    await user.click(screen.getByRole('button', { name: /Create Work/i }));

    // The error toast surfaces the parsed envelope message (W-1 fix, live).
    expect(await screen.findByText('Could not create Work')).toBeInTheDocument();
    expect(screen.getByText('Initial idea is too short.')).toBeInTheDocument();
    // The dialog stays open so the author can correct and retry.
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it('blocks submission until all required fields are filled', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/local/works', () => HttpResponse.json({ work_id: 'x', status: 'intake' })),
    );

    renderDialog();
    const submit = screen.getByRole('button', { name: /Create Work/i });
    expect(submit).toBeDisabled();

    // Partial fill is still not enough.
    await user.type(screen.getByLabelText(/Title/i), 'Only a title');
    expect(submit).toBeDisabled();
  });
});
