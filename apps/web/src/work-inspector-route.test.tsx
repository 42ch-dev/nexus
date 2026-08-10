/**
 * Assembly Inspector route — P1 T3 (DF-76) IA placement coverage.
 *
 * Pins the creator-area entry point: `/works/:workId/inspector` mounts inside
 * `WorkShellLayout` as a Control-Room-style sibling of outline/timeline/
 * chapters (no new `CanvasSurfaceKind`), renders the panel from the P0
 * `POST /v1/daemon/inspector/moment` route via `useInspectMoment`, and drives
 * the request from the Work's bound world. Also covers the no-bound-world
 * empty state (the page cannot assemble a moment without a world).
 *
 * Mount strategy mirrors `app-work-timeline-route.test.tsx`: a real
 * `MemoryRouter` with the production `WorkShellLayout` + lazy route boundary
 * stubbed, and a mocked `NexusClient` resolving `getWork` (via MSW) +
 * `inspectMoment` (per-test override).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Navigate, Route, Routes } from 'react-router';
import { Suspense } from 'react';

import { WorkShellLayout } from '@/components/layout/work-shell-layout';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList, workDetail } from '@/test/handlers';
import type { NexusClient } from '@/lib/nexus';
import type { MomentInspectResponse } from '@42ch/nexus-contracts';

import { WorkInspectorPage } from '@/pages/work-inspector-page';
import { NotFoundPage } from '@/pages/not-found-page';

function makePacket(): MomentInspectResponse {
  return {
    modules: {
      placement: [{ entry_id: 'king-entry', canonical_name: "King's court", reason: 'matched key [king]' }],
      activation_trace: [
        {
          entry_id: 'king-entry',
          canonical_name: "King's court",
          reason: 'primary-any (literal): matched key [king]',
          accepted: true,
        },
        {
          entry_id: 'bandit-entry',
          canonical_name: 'Bandit gang',
          reason: 'no matching keys',
          accepted: false,
        },
      ],
    },
    slot_map: [{ entry_id: 'king-entry', slot: 'world.before' }],
    budget: { primary_tokens_est: 120, hop_tokens_est: 0, cap: 512, remaining: 392 },
    moment_directive: {
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

function makeClient(packet: MomentInspectResponse): NexusClient {
  // BrowserClient base so MSW intercepts the real `getWork` path; override
  // `inspectMoment` so the inspector query resolves deterministically.
  const base = new BrowserClient();
  return Object.assign(base, {
    inspectMoment: vi.fn().mockResolvedValue(packet),
  }) as unknown as NexusClient;
}

/** Minimal route tree mirroring `App.tsx` for `/works/:workId/*`. */
function WorkRouteTree() {
  return (
    <Routes>
      <Route path="works/:workId" element={<WorkShellLayout />}>
        <Route index element={<Navigate to="outline" replace />} />
        <Route path="outline" element={<div data-testid="outline-route">Outline</div>} />
        <Route
          path="inspector"
          element={
            <Suspense fallback={<div data-testid="inspector-route-loading">Loading…</div>}>
              <WorkInspectorPage />
            </Suspense>
          }
        />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}

function useWorkRouteHandlers(worldId?: string) {
  useHandlers(
    worksList([
      {
        work_id: 'work-a',
        title: 'Alpha Novel',
        status: 'active',
        intake_status: 'complete',
        primary_preset_id: 'novel-writing',
        updated_at: '2026-06-24T00:00:00Z',
      },
    ]),
    workDetail('work-a', {
      title: 'Alpha Novel',
      status: 'active',
      work_profile: 'novel',
      primary_preset_id: 'novel-writing',
      updated_at: '2026-06-24T00:00:00Z',
      ...(worldId ? { world_id: worldId } : {}),
    }),
  );
}

describe('Assembly Inspector route (V1.151 P1 T3)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the panel at /works/:workId/inspector from the bound world', async () => {
    useWorkRouteHandlers('world-a');
    const packet = makePacket();
    const client = makeClient(packet);

    renderInApp(<WorkRouteTree />, {
      client,
      initialRouterEntries: ['/works/work-a/inspector'],
    });

    await waitFor(
      () => {
        expect(screen.getByTestId('assembly-inspector-panel')).toBeInTheDocument();
      },
      { timeout: 5000 },
    );

    // The shell layout is present (creator-area nesting verified).
    expect(screen.getByTestId('work-shell-layout')).toBeInTheDocument();
    // Fired + missed trace rows render from the route response.
    expect(screen.getByTestId('trace-entry-king-entry')).toHaveTextContent(/Fired/);
    expect(screen.getByTestId('trace-entry-bandit-entry')).toHaveTextContent(/Missed/);
  });

  it('calls inspectMoment with the work + bound world (default generation stage omitted)', async () => {
    useWorkRouteHandlers('world-a');
    const client = makeClient(makePacket());

    renderInApp(<WorkRouteTree />, {
      client,
      initialRouterEntries: ['/works/work-a/inspector'],
    });

    await waitFor(
      () => {
        expect(screen.getByTestId('assembly-inspector-panel')).toBeInTheDocument();
      },
      { timeout: 5000 },
    );

    expect(client.inspectMoment).toHaveBeenCalledWith({ world_id: 'world-a', work_id: 'work-a' });
  });

  it('shows the no-bound-world empty state when the work has no world', async () => {
    useWorkRouteHandlers(undefined);
    const client = makeClient(makePacket());

    renderInApp(<WorkRouteTree />, {
      client,
      initialRouterEntries: ['/works/work-a/inspector'],
    });

    await waitFor(
      () => {
        expect(screen.getByText('No bound world')).toBeInTheDocument();
      },
      { timeout: 5000 },
    );

    // No inspection fires without a world to assemble over.
    expect(client.inspectMoment).not.toHaveBeenCalled();
  });
});
