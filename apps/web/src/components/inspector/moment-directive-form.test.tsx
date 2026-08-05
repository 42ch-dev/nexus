/**
 * MomentDirectiveForm — P1 T4 (DF-76) coverage.
 *
 * Exercises the thin set/clear form against stateful msw handlers for
 * `POST /v1/daemon/moment-directive` + `POST /v1/daemon/inspector/moment`,
 * mirroring the page composition (panel + form via `directiveActions`).
 *
 * The directive-state lives in a module-level variable shared by both
 * handlers, so the invalidation contract is observable end to end: a
 * successful set flips the state to `active` and the refetched inspector
 * packet renders the active status; clear flips back to `none`; a 409
 * surfaces the replace prompt (no silent overwrite).
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent, { type UserEvent } from '@testing-library/user-event';

import { useInspectMoment } from '@/api/queries';
import { AssemblyInspectorPanel } from '@/components/inspector/assembly-inspector-panel';
import { MomentDirectiveForm } from '@/components/inspector/moment-directive-form';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import type { MomentInspectResponse } from '@42ch/nexus-contracts';

type DirectiveBody = {
  action?: string;
  replace?: boolean;
  scope?: { kind: string; id: string };
};

/** Shared directive state — flipped by the directive handler, read by the
 * inspector handler so the refetch reflects the write (AC-I5). */
let directiveStatus: 'none' | 'active' = 'none';
let setRequests: DirectiveBody[] = [];
let clearRequests: DirectiveBody[] = [];

function makePacket(status: 'none' | 'active'): MomentInspectResponse {
  return {
    modules: { placement: [], activation_trace: [] },
    slot_map: [],
    budget: { primary_tokens_est: 0, hop_tokens_est: 0, cap: null, remaining: null },
    moment_directive:
      status === 'active'
        ? {
            scope: 'work',
            scope_id: 'work-a',
            insert_depth: 'tail',
            ttl_kind: 'generations',
            ttl_remaining: 5,
            clear_on_scene_change: false,
            status: 'active',
          }
        : {
            scope: null,
            scope_id: null,
            insert_depth: null,
            ttl_kind: null,
            ttl_remaining: null,
            clear_on_scene_change: false,
            status: 'none',
          },
  };
}

function harnessHandlers() {
  return [
    http.post('/v1/daemon/inspector/moment', () =>
      HttpResponse.json(makePacket(directiveStatus)),
    ),
    http.post('/v1/daemon/moment-directive', async ({ request }) => {
      const body = (await request.json()) as DirectiveBody;
      if (body.action === 'set') {
        setRequests.push(body);
        if (directiveStatus === 'active' && !body.replace) {
          return HttpResponse.json(
            {
              success: false,
              error: {
                code: 'conflict',
                message:
                  'A Moment Directive is already active for this scope. Pass "replace": true to supersede it.',
              },
            },
            { status: 409 },
          );
        }
        directiveStatus = 'active';
        return HttpResponse.json({ directive_id: 'dir_1', status: 'active' });
      }
      clearRequests.push(body);
      directiveStatus = 'none';
      return HttpResponse.json({});
    }),
  ];
}

/** Mirrors the page composition: panel + form via the `directiveActions`
 * extension point, backed by the real `useInspectMoment` query. */
function Harness({ workId = 'work-a', worldId = 'world-a' }: { workId?: string; worldId?: string }) {
  const inspect = useInspectMoment({ world_id: worldId, work_id: workId });
  const packet = inspect.data;
  if (!packet) return null;
  return (
    <AssemblyInspectorPanel
      packet={packet}
      directiveActions={
        <MomentDirectiveForm
          workId={workId}
          worldId={worldId}
          hasActiveDirective={packet.moment_directive.status !== 'none'}
        />
      }
    />
  );
}

function renderHarness(overrides?: { workId?: string; worldId?: string }) {
  return renderInApp(<Harness workId={overrides?.workId} worldId={overrides?.worldId} />, {
    client: new BrowserClient(),
  });
}

async function openForm(user: UserEvent) {
  await user.click(screen.getByText('Set / edit directive'));
  await waitFor(() => expect(screen.getByLabelText('Directive body')).toBeInTheDocument());
}

/** Fill body + TTL (exactly one kind, count >= 1) — the minimal valid set. */
async function fillValidSet(user: UserEvent) {
  await user.type(screen.getByLabelText('Directive body'), 'Keep the prose terse.');
  await user.click(screen.getByLabelText('Generations'));
  fireEvent.change(screen.getByLabelText('TTL count'), { target: { value: '5' } });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
  directiveStatus = 'none';
  setRequests = [];
  clearRequests = [];
});

describe('MomentDirectiveForm — set (T4)', () => {
  it('submits a Work-scoped set and the inspector refreshes to active', async () => {
    useHandlers(...harnessHandlers());
    const user = userEvent.setup();
    renderHarness();

    expect(await screen.findByTestId('directive-form')).toBeInTheDocument();
    expect(screen.getByTestId('directive-none')).toHaveTextContent('No active directive');

    await openForm(user);
    await fillValidSet(user);
    await user.click(screen.getByTestId('directive-form-set'));

    await waitFor(() => expect(setRequests).toHaveLength(1));
    expect(setRequests[0]).toEqual({
      action: 'set',
      scope: { kind: 'work', id: 'work-a' },
      body: 'Keep the prose terse.',
      insert_depth: 'tail',
      ttl_kind: 'generations',
      ttl_remaining: 5,
      clear_on_scene_change: false,
    });
    expect(setRequests[0]).not.toHaveProperty('replace');

    // Invalidation → the inspector packet refetches and shows the active
    // directive (set → active), and the Clear action becomes available.
    expect(await screen.findByTestId('directive-status-active')).toHaveTextContent('Active');
    expect(screen.queryByTestId('directive-none')).toBeNull();
    await waitFor(() => expect(screen.getByTestId('directive-form-clear')).toBeEnabled());
  });

  it('scopes to the bound World when the World override is selected', async () => {
    useHandlers(...harnessHandlers());
    const user = userEvent.setup();
    renderHarness();
    await screen.findByTestId('directive-form');

    await openForm(user);
    await user.click(screen.getByLabelText('World'));
    await fillValidSet(user);
    await user.click(screen.getByTestId('directive-form-set'));

    await waitFor(() => expect(setRequests).toHaveLength(1));
    expect(setRequests[0].scope).toEqual({ kind: 'world', id: 'world-a' });
  });
});

describe('MomentDirectiveForm — clear (T4)', () => {
  it('clears the active directive and the inspector refreshes to none', async () => {
    directiveStatus = 'active';
    useHandlers(...harnessHandlers());
    const user = userEvent.setup();
    renderHarness();

    expect(await screen.findByTestId('directive-status-active')).toBeInTheDocument();

    await user.click(screen.getByTestId('directive-form-clear'));

    await waitFor(() => expect(clearRequests).toHaveLength(1));
    expect(clearRequests[0]).toEqual({ action: 'clear', scope: { kind: 'work', id: 'work-a' } });

    // Invalidation → the packet refetches to none; Clear deactivates.
    expect(await screen.findByTestId('directive-none')).toHaveTextContent('No active directive');
    await waitFor(() => expect(screen.getByTestId('directive-form-clear')).toBeDisabled());
  });

  it('keeps Clear disabled while no directive is active', async () => {
    useHandlers(...harnessHandlers());
    renderHarness();
    await screen.findByTestId('directive-form');
    expect(screen.getByTestId('directive-form-clear')).toBeDisabled();
  });
});

describe('MomentDirectiveForm — 409 replace prompt (T4)', () => {
  it('surfaces the conflict and retries with replace enabled — no silent overwrite', async () => {
    directiveStatus = 'active';
    useHandlers(...harnessHandlers());
    const user = userEvent.setup();
    renderHarness();
    await screen.findByTestId('directive-form');

    await openForm(user);
    await fillValidSet(user);
    await user.click(screen.getByTestId('directive-form-set'));

    // 409 → the conflict prompt appears instead of overwriting silently.
    const conflict = await screen.findByTestId('directive-form-conflict');
    expect(conflict).toHaveTextContent('A directive is already active for this scope');
    expect(setRequests).toHaveLength(1);
    expect(setRequests[0]).not.toHaveProperty('replace');

    // "Enable replace and retry" re-submits with `replace: true` and succeeds.
    await user.click(screen.getByTestId('directive-form-enable-replace'));
    await waitFor(() => expect(setRequests).toHaveLength(2));
    expect(setRequests[1].replace).toBe(true);
    expect(screen.queryByTestId('directive-form-conflict')).toBeNull();
    expect(await screen.findByTestId('directive-status-active')).toHaveTextContent('Active');
  });
});

describe('MomentDirectiveForm — client-side validation mirrors the CLI (T4)', () => {
  it('blocks an empty body and a missing TTL kind without firing a request', async () => {
    useHandlers(...harnessHandlers());
    const user = userEvent.setup();
    renderHarness();
    await screen.findByTestId('directive-form');

    await openForm(user);
    await user.click(screen.getByTestId('directive-form-set'));

    const errors = await screen.findByTestId('directive-form-errors');
    expect(errors).toHaveTextContent('Directive body is required.');
    expect(errors).toHaveTextContent('Choose exactly one TTL kind.');
    expect(setRequests).toHaveLength(0);
  });

  it('rejects a TTL count below 1 and non-integer counts', async () => {
    useHandlers(...harnessHandlers());
    const user = userEvent.setup();
    renderHarness();
    await screen.findByTestId('directive-form');

    await openForm(user);
    await user.type(screen.getByLabelText('Directive body'), 'Keep the prose terse.');
    await user.click(screen.getByLabelText('Chapters'));

    fireEvent.change(screen.getByLabelText('TTL count'), { target: { value: '0' } });
    await user.click(screen.getByTestId('directive-form-set'));
    expect(screen.getByTestId('directive-form-errors')).toHaveTextContent(
      'TTL count must be a whole number of at least 1.',
    );

    fireEvent.change(screen.getByLabelText('TTL count'), { target: { value: '2.5' } });
    await user.click(screen.getByTestId('directive-form-set'));
    expect(screen.getByTestId('directive-form-errors')).toHaveTextContent(
      'TTL count must be a whole number of at least 1.',
    );
    expect(setRequests).toHaveLength(0);
  });
});
