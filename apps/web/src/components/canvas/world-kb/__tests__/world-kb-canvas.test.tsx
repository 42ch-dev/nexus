/**
 * World KB canvas orchestrator — pure-helper + conflict-reapply tests (V1.73).
 *
 * `patchFromForm` rebuilds the patch payload for the conflict-modal "Reapply"
 * path. It must produce the SAME shape the primary inspector submit path
 * produces, otherwise the reapply silently misbehaves (V1.73 greploop issue 4).
 *
 * The orchestration test below covers V1.73 greploop iter 3: the promote
 * conflict `onReapply` must mirror the entity path and update
 * `promoteConflict.currentVersion` from a second 409 so a follow-up reapply
 * does not re-send a stale `expected_version`.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';

import { patchFromForm, WorldKbCanvas, type EntityField } from '../world-kb-canvas';
import type { EntityEditForm } from '../entity-inspector';
import { NexusClientError } from '@/lib/nexus/errors';
import type { WorldKbCandidateProjection } from '@42ch/nexus-contracts';

function form(overrides: Partial<EntityEditForm> = {}): EntityEditForm {
  return {
    title: 'Aria',
    bodyText: '',
    aliasesText: '',
    block_type: 'character',
    ...overrides,
  };
}

describe('patchFromForm (reapply payload shape)', () => {
  it('preserves a non-empty trimmed title', () => {
    const patch = patchFromForm(form({ title: '  Aria Stormwind  ' }), [
      'title',
    ] as EntityField[]);
    expect(patch.title).toBe('Aria Stormwind');
  });

  // Regression for V1.73 greploop issue 4: coercing the trimmed empty string to
  // `undefined` dropped `title` from the JSON payload, so a reapply after the
  // user cleared the title sent an empty patch → 400 InvalidInput. The primary
  // submit path sends the empty string, surfacing a meaningful 422.
  it('keeps an explicitly-cleared title as the empty string (not undefined)', () => {
    const patch = patchFromForm(form({ title: '   ' }), ['title'] as EntityField[]);

    // `title` must be present as an empty string...
    expect(patch.title).toBe('');
    expect('title' in patch).toBe(true);
    // ...and survive JSON serialization so it reaches the server.
    expect(JSON.parse(JSON.stringify(patch)).title).toBe('');
  });

  it('omits title entirely when it is not in the dirty set', () => {
    const patch = patchFromForm(form({ title: 'unchanged' }), [
      'body',
    ] as EntityField[]);
    expect(patch.title).toBeUndefined();
    expect('title' in patch).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Promote conflict reapply onError — V1.73 greploop iter 3 (greptile 4→5).
//
// The promote `onReapply` handler must carry an `onError` that, on a second 409
// racing the reapply, stores the new `current_version` so the next Reapply uses
// the real version instead of re-sending the stale one. The data hooks are
// stubbed so the mutate opts are captured for manual onError invocation; list
// view is forced so React Flow never mounts in jsdom (no ResizeObserver).
// ---------------------------------------------------------------------------

const mocks = vi.hoisted(() => {
  const promoteMutate = vi.fn();
  const candidate = {
    world_id: 'w-1',
    candidate_id: 'c-1',
    job_id: 'job-1',
    block_type: 'character',
    canonical_name: 'Elena Vale',
    version: 1,
    source_anchor_count: 0,
    created_at: '2026-06-29T00:00:00Z',
  } as unknown as WorldKbCandidateProjection;
  // Return STABLE references: the canvas derives `candidateItems` from the
  // hook result each render and feeds it into a useMemo + setNodes effect — a
  // fresh `data` object per render would retrigger that effect forever.
  const graphData = { entities: [] as unknown[], source_anchors: [] as unknown[] };
  return {
    promoteMutate,
    graphResult: {
      data: graphData,
      isLoading: false,
      isError: false,
      isFetching: false,
      refetch: vi.fn(),
      dataUpdatedAt: 0,
    },
    candidatesResult: {
      data: { items: [candidate] },
      isLoading: false,
      isError: false,
      isFetching: false,
      refetch: vi.fn(),
      dataUpdatedAt: 0,
    },
    patchResult: { mutate: vi.fn(), isPending: false },
    // The inspector and the canvas each call usePromoteWorldKbCandidate; both
    // resolve to the same mutate stub so call order is deterministic.
    promoteResult: { mutate: promoteMutate, isPending: false },
  };
});

vi.mock('@/lib/canvas/use-world-kb-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-world-kb-data')>();
  return {
    ...actual,
    // `isWorldKbConflictError` (and other pure guards) stay real via ...actual.
    useWorldKbGraph: () => mocks.graphResult,
    useWorldKbCandidates: () => mocks.candidatesResult,
    usePatchWorldKbEntity: () => mocks.patchResult,
    usePromoteWorldKbCandidate: () => mocks.promoteResult,
  };
});

// Force the non-spatial list view so the canvas never mounts React Flow
// (jsdom has no ResizeObserver); the alt-view row click is the selection path.
vi.mock('../use-view-preference', () => ({
  useReducedMotionPreference: () => true,
}));

/** Build a real NexusClientError 409 carrying a `current_version`. */
function promoteConflictErr(currentVersion: number): NexusClientError {
  return new NexusClientError(409, 'world_kb_conflict', 'stale version', {
    current_version: currentVersion,
    conflicting_path: 'version',
    recovery_hint: '',
  });
}

/** Invoke the latest captured mutate call's onError callback inside act(). */
async function rejectLastPromoteAsConflict(currentVersion: number) {
  const lastCall = mocks.promoteMutate.mock.calls.at(-1);
  if (!lastCall) throw new Error('no promoteCandidate.mutate call captured');
  const opts = lastCall[1] as { onError?: (e: unknown) => void };
  await act(async () => {
    opts.onError?.(promoteConflictErr(currentVersion));
  });
}

describe('promote conflict reapply onError (V1.73 greploop iter 3)', () => {
  it('updates currentVersion on a second 409 so the next reapply is not stale', async () => {
    const user = userEvent.setup();
    render(<WorldKbCanvas worldId="w-1" />);

    // 1. Select the pending candidate via the list view (no selection yet, so
    //    the name only appears in the alt-view row).
    await user.click(await screen.findByText('Elena Vale'));

    // 2. Inspector submit → first 409 (v5) hands off to the canvas, opening the
    //    conflict modal at version 5.
    await user.click(screen.getByRole('button', { name: /Adopt candidate/i }));
    await rejectLastPromoteAsConflict(5);
    expect(
      await screen.findByText('5', { selector: 'span.font-mono' }),
    ).toBeInTheDocument();

    // 3. Reapply → a concurrent write races and returns a SECOND 409 (v9). The
    //    fix's onError must store v9 as the new currentVersion.
    await user.click(screen.getByRole('button', { name: /Reapply my decision/i }));
    await rejectLastPromoteAsConflict(9);

    // 4. Reapply again. The request MUST carry expected_version = 9 (the new
    //    version). Without the onError fix the state stays stale at 5 and this
    //    assertion fails — the conflict would never resolve without dismissing.
    await user.click(screen.getByRole('button', { name: /Reapply my decision/i }));
    const lastRequest = (
      mocks.promoteMutate.mock.calls.at(-1) as [unknown, unknown]
    )[0] as { expected_version: number };
    expect(lastRequest.expected_version).toBe(9);
  });
});
