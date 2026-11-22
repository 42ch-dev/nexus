/**
 * World KB promotion inspector tests — merge-target dropdown filter (V1.73 P0 A6).
 *
 * Regression for V1.73 greploop iter 4 (greptile 4→5): the merge-target
 * dropdown must only offer entities the backend `promote_merge` handler
 * accepts (status `confirmed` or `manual`, per world_kb.rs promote_merge status
 * guard). Earlier code filtered only on `block_type`, so `merged`/`rejected`/
 * `deprecated`/`deleted` entities surfaced as valid targets and produced an
 * unrecoverable 422 when selected.
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
} from '@42ch/nexus-contracts';

import { PromotionInspector } from '../promotion-inspector';
import type { WorldKbNodeData } from '../types';

const mocks = vi.hoisted(() => ({
  promoteResult: { mutate: vi.fn(), isPending: false },
}));

vi.mock('@/lib/canvas/use-world-kb-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-world-kb-data')>();
  return {
    ...actual,
    usePromoteWorldKbCandidate: () => mocks.promoteResult,
  };
});

const node: WorldKbNodeData = {
  worldId: 'w-1',
  keyBlockId: 'kb-pending-1',
  entityKind: 'character',
  name: 'Elena Vale (candidate)',
  lifecycle: 'pending',
  version: 1,
  sourceAnchorCount: 0,
  computable: false,
};

const candidate = {
  candidate_id: 'c-1',
  job_id: 'job-1',
  world_id: 'w-1',
  block_type: 'character',
  canonical_name: 'Elena Vale (candidate)',
  version: 1,
} as unknown as WorldKbCandidateProjection;

function entity(
  id: string,
  name: string,
  status: WorldKbEntityProjection['status'],
  block_type: WorldKbEntityProjection['block_type'] = 'character',
): WorldKbEntityProjection {
  return {
    key_block_id: id,
    world_id: 'w-1',
    block_type,
    canonical_name: name,
    status,
    version: 2,
  };
}

describe('PromotionInspector — merge-target dropdown filter', () => {
  it('offers only confirmed/manual targets and excludes terminal statuses', async () => {
    const user = userEvent.setup();
    render(
      <PromotionInspector
        worldId="w-1"
        node={node}
        candidate={candidate}
        confirmedEntities={[
          entity('kb-confirmed', 'Aria Stormwind', 'confirmed'),
          entity('kb-manual', 'Bram Hollow', 'manual'),
          entity('kb-merged', 'Cassia Reed', 'merged'),
          entity('kb-rejected', 'Dorin Vael', 'rejected'),
          entity('kb-deprecated', 'Elspeth Moor', 'deprecated'),
          entity('kb-deleted', 'Fenwick Ash', 'deleted'),
          entity('kb-pending', 'Greta Wells', 'pending'),
          // Different block_type — must never appear regardless of status.
          entity('kb-scene', 'Stonebridge', 'confirmed', 'scene'),
        ]}
        onConflict={vi.fn()}
      />,
    );

    // Switch to the merge action so the target dropdown mounts.
    await user.click(screen.getByRole('radio', { name: /Merge candidate/i }));

    const select = screen.getByRole('combobox');
    const options = Array.from(select.querySelectorAll('option'));
    const offered = options.map((o) => o.textContent ?? '');

    // Allowed targets present.
    expect(offered).toEqual(
      expect.arrayContaining([
        expect.stringContaining('Aria Stormwind'),
        expect.stringContaining('Bram Hollow'),
      ]),
    );
    // Terminal / non-allowed statuses absent.
    expect(offered.some((t) => t.includes('Cassia Reed'))).toBe(false);
    expect(offered.some((t) => t.includes('Dorin Vael'))).toBe(false);
    expect(offered.some((t) => t.includes('Elspeth Moor'))).toBe(false);
    expect(offered.some((t) => t.includes('Fenwick Ash'))).toBe(false);
    expect(offered.some((t) => t.includes('Greta Wells'))).toBe(false);
    // Wrong block_type absent.
    expect(offered.some((t) => t.includes('Stonebridge'))).toBe(false);
  });

  it('shows the empty state when only non-allowed statuses match the block type', async () => {
    const user = userEvent.setup();
    render(
      <PromotionInspector
        worldId="w-1"
        node={node}
        candidate={candidate}
        // Same block_type, but every status is terminal / non-allowed.
        confirmedEntities={[
          entity('kb-merged', 'Cassia Reed', 'merged'),
          entity('kb-deleted', 'Fenwick Ash', 'deleted'),
        ]}
        onConflict={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('radio', { name: /Merge candidate/i }));

    // No combobox (the list is empty after filtering)…
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
    // …and the "no confirmed entities" hint is shown instead.
    expect(
      screen.getByText(/No confirmed .*entities to merge into/i),
    ).toBeInTheDocument();
  });
});
