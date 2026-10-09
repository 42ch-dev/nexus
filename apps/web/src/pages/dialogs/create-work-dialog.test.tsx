/**
 * Create Work CRUD round-trip test (R-V164-QC1-S1-P1).
 *
 * Exercises the full write path end-to-end against msw: open the dialog, fill
 * the required fields, submit, and assert the daemon receives a well-formed
 * POST `/v1/daemon/works`. Also covers the W-1 error path — a 400 envelope
 * surfaces as a toast (the mutation's onError → useToast) and the dialog stays
 * open so the author can correct.
 */
import { http, HttpResponse } from 'msw';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { WORK_PROFILES } from '@/lib/work-profiles';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import { narrativeWorld, narrativeWorldsList } from '@/test/handlers';
import { CreateWorkDialog } from '@/pages/dialogs/create-work-dialog';

function renderDialog() {
  const onCreated = vi.fn();
  const onOpenChange = vi.fn();
  // renderInApp mounts ToastProvider + Toaster (mirrors main.tsx) so the
  // mutation's onError → useToast → Toaster portal path is exercised live.
  const view = renderInApp(
    <CreateWorkDialog open onOpenChange={onOpenChange} onCreated={onCreated} />,
    {
      client: new BrowserClient(),
    },
  );
  return { onCreated, onOpenChange, container: view.container };
}

/** The dialog's World selector is fed by `GET /v1/daemon/narrative/worlds`. */
const ONE_WORLD = [narrativeWorld()];

/** Wait for the single-world preselect (F-01 W1) to land on the selector. */
async function waitForWorldPreselect() {
  await waitFor(() =>
    expect(screen.getByLabelText(/^World$/i)).toHaveValue('world-1'),
  );
}

describe('CreateWorkDialog CRUD round-trip', () => {
  it('submits a well-formed POST /v1/daemon/works including the required world_id (F-01 W1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      narrativeWorldsList(ONE_WORLD),
      http.post('/v1/daemon/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-new', status: 'intake' });
      }),
    );

    const { onCreated, onOpenChange } = renderDialog();
    await waitForWorldPreselect();

    await user.type(screen.getByLabelText(/Title/i), 'My New Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Finish the first arc');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A heist in a floating city');
    // The Work-profile selector is NOT touched — V1.66 semantics require the
    // field to be omitted so the daemon stores NULL (qc1 W1).
    await user.click(screen.getByRole('button', { name: /^Create$/i }));

    await waitFor(() => expect(onCreated).toHaveBeenCalledWith('w-new'));
    expect(postedBody).toEqual({
      title: 'My New Work',
      long_term_goal: 'Finish the first arc',
      initial_idea: 'A heist in a floating city',
      world_id: 'world-1',
    });
    expect(postedBody).not.toHaveProperty('work_profile');
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('sends the selected work_profile when the author changes it (V1.67 G1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      narrativeWorldsList(ONE_WORLD),
      http.post('/v1/daemon/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-essay', status: 'intake' });
      }),
    );

    renderDialog();
    await waitForWorldPreselect();

    await user.type(screen.getByLabelText(/Title/i), 'Essay Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Publish a collection');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A meditation on cities');
    await user.selectOptions(screen.getByLabelText(/Work profile/i), 'essay');
    await user.click(screen.getByRole('button', { name: /^Create$/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    expect(postedBody).toMatchObject({ work_profile: 'essay' });
  });

  it('keeps the dialog open and shows a toast when the daemon returns a 400 envelope (W-1)', async () => {
    const user = userEvent.setup();
    useHandlers(
      narrativeWorldsList(ONE_WORLD),
      http.post('/v1/daemon/works', () =>
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
    await waitForWorldPreselect();

    await user.type(screen.getByLabelText(/Title/i), 'A Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'A goal');
    await user.type(screen.getByLabelText(/Initial idea/i), 'An idea');
    await user.click(screen.getByRole('button', { name: /^Create$/i }));

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
      narrativeWorldsList(ONE_WORLD),
      http.post('/v1/daemon/works', () => HttpResponse.json({ work_id: 'x', status: 'intake' })),
    );

    renderDialog();
    const submit = screen.getByRole('button', { name: /^Create$/i });
    expect(submit).toBeDisabled();

    // Partial fill is still not enough.
    await user.type(screen.getByLabelText(/Title/i), 'Only a title');
    expect(submit).toBeDisabled();
  });

  it('blocks create when no World is available (F-01 W1)', async () => {
    const user = userEvent.setup();
    useHandlers(
      narrativeWorldsList([]),
      http.post('/v1/daemon/works', () => HttpResponse.json({ work_id: 'x', status: 'intake' })),
    );

    renderDialog();

    await user.type(screen.getByLabelText(/Title/i), 'Orphan Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'A goal');
    await user.type(screen.getByLabelText(/Initial idea/i), 'An idea');

    // No Worlds → the selector shows the empty hint and create stays blocked:
    // the daemon would 400 `world_id_required` anyway.
    const worldSelect = screen.getByLabelText(/^World$/i);
    expect(worldSelect).toHaveValue('');
    expect(screen.getByRole('option', { name: /No Worlds available/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Create$/i })).toBeDisabled();
  });

  it('lets the author pick a World when several exist and sends its world_id (F-01 W1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      narrativeWorldsList([
        narrativeWorld(),
        narrativeWorld({ world_id: 'world-2', title: 'Briar Archive', slug: 'briar-archive' }),
      ]),
      http.post('/v1/daemon/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-multi', status: 'intake' });
      }),
    );

    renderDialog();

    // Two worlds → no preselect; the author must choose explicitly.
    const worldSelect = screen.getByLabelText(/^World$/i);
    await waitFor(() =>
      expect(screen.getByRole('option', { name: 'Briar Archive' })).toBeInTheDocument(),
    );
    expect(worldSelect).toHaveValue('');
    await user.selectOptions(worldSelect, 'world-2');

    await user.type(screen.getByLabelText(/Title/i), 'Multi Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'A goal');
    await user.type(screen.getByLabelText(/Initial idea/i), 'An idea');
    await user.click(screen.getByRole('button', { name: /^Create$/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    expect(postedBody).toMatchObject({ world_id: 'world-2' });
  });

  it('sends work_profile when the author explicitly selects the default novel (W1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      narrativeWorldsList(ONE_WORLD),
      http.post('/v1/daemon/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-novel', status: 'intake' });
      }),
    );

    renderDialog();
    await waitForWorldPreselect();

    await user.type(screen.getByLabelText(/Title/i), 'Novel Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Finish the draft');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A quiet coastal town');
    // Explicitly re-select the default — this counts as "touched" and MUST
    // send work_profile (qc1 W1 positive case).
    await user.selectOptions(screen.getByLabelText(/Work profile/i), 'novel');
    await user.click(screen.getByRole('button', { name: /^Create$/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    expect(postedBody).toMatchObject({ work_profile: 'novel' });
  });

  it('emits the canonical underscore wire value for Game Bible (C1)', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      narrativeWorldsList(ONE_WORLD),
      http.post('/v1/daemon/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-gb', status: 'intake' });
      }),
    );

    renderDialog();
    await waitForWorldPreselect();

    await user.type(screen.getByLabelText(/Title/i), 'Game Bible Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Ship the lore bible');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A dying solar system');
    await user.selectOptions(screen.getByLabelText(/Work profile/i), 'game_bible');
    await user.click(screen.getByRole('button', { name: /^Create$/i }));

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
  // bootstrap at apps/nexus42/src/commands/creator/bootstrap.rs:140-143
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

// AC-P1-3 / AC-P1-5 (V1.120 P1 T2) — Select single chevron + disabled-primary
// dark token contrast. The Select primitive (native `<select>` from
// @42ch/nexus-ui) suppresses the UA dropdown arrow via `appearance-none` and
// renders a single in-boundary chevron overlay; apps/web's Tailwind content
// config must scan the package source so `appearance-none` is emitted (else the
// native arrow re-appears alongside the overlay → duplicate chevron).
describe('CreateWorkDialog Select chevron + disabled Create button', () => {
  it('renders a single chevron inside the Work-profile Select control boundary (AC-P1-3)', () => {
    useHandlers(narrativeWorldsList(ONE_WORLD));
    renderDialog();

    const profileSelect = screen.getByLabelText(/Work profile/i);
    expect(profileSelect).toHaveProperty('tagName', 'SELECT');
    // `appearance-none` suppresses the native UA dropdown arrow; `pe-8`
    // reserves the right inset for the overlay. Both are package-exclusive
    // utilities, so they rely on apps/web scanning packages/nexus-ui/src.
    expect(profileSelect).toHaveClass('appearance-none');
    expect(profileSelect).toHaveClass('pe-8');

    // The dialog now renders two Select controls (World + Work profile), so
    // two overlay chevrons exist in the portal — but exactly ONE per control
    // boundary: no duplicate native arrow + overlay inside the wrapper.
    expect(screen.queryAllByTestId('select-chevron')).toHaveLength(2);

    // The chevron sits inside the same `.relative` control wrapper as the
    // `<select>` (i.e. within the control boundary, not bleeding outside).
    const wrapper = profileSelect.parentElement;
    expect(wrapper).not.toBeNull();
    expect(wrapper).toHaveClass('relative');
    expect(within(wrapper!).getAllByTestId('select-chevron')).toHaveLength(1);
  });

  it('disabled Create button applies the dark-token disabled-primary classes (AC-P1-5)', () => {
    useHandlers(narrativeWorldsList([]));
    renderDialog();

    // Empty form → primary Create submit is disabled.
    const createBtn = screen.getByRole('button', { name: /^Create$/i });
    expect(createBtn).toBeDisabled();
    expect(createBtn).toHaveClass('disabled:bg-gray-100');
    expect(createBtn).toHaveClass('disabled:text-gray-700');
    // The disabled treatment is theme-aware via CSS vars (no separate dark
    // rule needed): in dark, --color-gray-100=#1f1f1f fill +
    // --color-gray-700=#a3a3a3 text = 6.53:1 text/bg (≥ WCAG AA). See
    // task-2-report.md for the contrast citation.
  });
});

// PR #372 (P2) — when the World list request fails before any data is cached,
// the dialog must show a read-error state with a Retry wired to
// `narrativeWorlds.refetch()` instead of the misleading "No Worlds available"
// empty option, and keep Create disabled until worlds load.
describe('CreateWorkDialog World read error (PR #372)', () => {
  it('shows a read-error state with Retry on a failed worlds request, and recovers on retry', async () => {
    const user = userEvent.setup();
    let worldsRecovered = false;
    let worldsRequests = 0;
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () => {
        worldsRequests += 1;
        // All reads fail until the test observes the error UI and flips the
        // latch — the retry (refetch) then succeeds. The test QueryClient
        // sets `retry: false`, so the initial failure surfaces immediately
        // without re-firing this handler.
        return worldsRecovered
          ? HttpResponse.json({ worlds: ONE_WORLD })
          : HttpResponse.json(
              { success: false, error: { code: 'internal', message: 'boom' } },
              { status: 500 },
            );
      }),
    );

    renderDialog();

    // Read-error state (role="alert" from ErrorState), NOT the empty option.
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(/couldn't load worlds/i)).toBeInTheDocument();
    expect(screen.queryByText(/no worlds available/i)).not.toBeInTheDocument();
    // The World selector is replaced by the error state.
    expect(screen.queryByLabelText(/^World$/i)).not.toBeInTheDocument();

    // Fill every required text field so submit-readiness is attributable to
    // the World state alone — Create must STILL stay blocked under the read
    // error (the button is not disabled merely because the fields are blank).
    await user.type(screen.getByLabelText(/Title/i), 'My New Work');
    await user.type(screen.getByLabelText(/Long-term goal/i), 'Finish the first arc');
    await user.type(screen.getByLabelText(/Initial idea/i), 'A heist in a floating city');
    expect(screen.getByRole('button', { name: /^Create$/i })).toBeDisabled();

    // Retry refetches the list → recovery: the selector comes back with the
    // single-world preselect, and Create becomes enabled with every other
    // required field already filled.
    worldsRecovered = true;
    const requestsBeforeRetry = worldsRequests;
    await user.click(within(alert).getByRole('button', { name: /try again/i }));
    await waitForWorldPreselect();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(worldsRequests).toBeGreaterThanOrEqual(requestsBeforeRetry + 1);
    expect(screen.getByRole('button', { name: /^Create$/i })).toBeEnabled();
  });
});
