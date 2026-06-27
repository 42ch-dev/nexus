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
import { CreateWorkDialog, WORK_PROFILES } from '@/pages/dialogs/create-work-dialog';

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
  it('submits a well-formed POST /v1/local/works and omits work_profile when untouched (W1)', async () => {
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
    // The Work-profile selector is NOT touched — V1.66 semantics require the
    // field to be omitted so the daemon stores NULL (qc1 W1).
    await user.click(screen.getByRole('button', { name: /Create Work/i }));

    await waitFor(() => expect(onCreated).toHaveBeenCalledWith('w-new'));
    expect(postedBody).toEqual({
      title: 'My New Work',
      long_term_goal: 'Finish the first arc',
      initial_idea: 'A heist in a floating city',
    });
    expect(postedBody).not.toHaveProperty('work_profile');
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

  it('sends work_profile when the author explicitly selects the default novel (W1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.post('/v1/local/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-novel', status: 'intake' });
      }),
    );

    renderDialog();

    await user.type(screen.getByLabelText(/Title/i), 'Novel Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Finish the draft');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A quiet coastal town');
    // Explicitly re-select the default — this counts as "touched" and MUST
    // send work_profile (qc1 W1 positive case).
    await user.selectOptions(screen.getByLabelText(/Work profile/i), 'novel');
    await user.click(screen.getByRole('button', { name: /Create Work/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    expect(postedBody).toMatchObject({ work_profile: 'novel' });
  });

  it('emits the canonical underscore wire value for Game Bible (C1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.post('/v1/local/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-gb', status: 'intake' });
      }),
    );

    renderDialog();

    await user.type(screen.getByLabelText(/Title/i), 'Game Bible Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Ship the lore bible');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A dying solar system');
    await user.selectOptions(screen.getByLabelText(/Work profile/i), 'game_bible');
    await user.click(screen.getByRole('button', { name: /Create Work/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    // C1: the wire value MUST be the underscore canonical form `game_bible`,
    // not the hyphenated `game-bible`. The daemon HTTP API stores the value
    // verbatim and the DB CHECK / Rust helpers only recognize `game_bible`.
    expect(postedBody).toMatchObject({ work_profile: 'game_bible' });
    expect(postedBody).not.toMatchObject({ work_profile: 'game-bible' });
  });
});

describe('CreateWorkDialog work_profile wire contract (C1)', () => {
  // Backend canonical accepted set — the authoritative source is the DB CHECK
  // constraint at
  //   crates/nexus-local-db/migrations/202606230001_work_profile_script.sql:27
  // (latest cumulative: novel / essay / game_bible / script). Confirmed by the
  // Rust helpers in crates/nexus-local-db/src/works.rs:28-60 and the daemon
  // handlers at crates/nexus-daemon-runtime/src/api/handlers/works.rs:576,623,
  // 678,733. The daemon HTTP API stores req.work_profile verbatim (no
  // normalization), so the UI MUST emit a member of this set. (The CLI
  // bootstrap at crates/nexus42/src/commands/creator/bootstrap.rs:140-143
  // accepts both game-bible/game_bible and normalizes — that path is NOT
  // used by the Web UI.)
  const BACKEND_ACCEPTED_WORK_PROFILES = new Set(['novel', 'essay', 'game_bible', 'script']);

  it('exposes exactly the four backend-supported profiles', () => {
    expect(WORK_PROFILES).toHaveLength(4);
    for (const option of WORK_PROFILES) {
      expect(
        BACKEND_ACCEPTED_WORK_PROFILES.has(option.value),
        `UI value "${option.value}" must be a backend-accepted work_profile`,
      ).toBe(true);
    }
  });

  it('uses the underscore canonical form for Game Bible (not the hyphenated drift)', () => {
    const gameBible = WORK_PROFILES.find((p) => p.label === 'Game Bible');
    expect(gameBible).toBeDefined();
    expect(gameBible?.value).toBe('game_bible');
  });
});
