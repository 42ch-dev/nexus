/**
 * era-create-dialog — V1.159 P1 Task 3 component tests (activated V1.160
 * P1 T2 — F-001 / R-V1159P1-001 closed: the V1.160 P1 T1 backend
 * create-on-absent path makes `patch-entity` create the era when the minted
 * entity id is absent, so the create dialog path is LIVE).
 *
 * Coverage:
 *   1. creates_era_without_parent  — patch-entity called with the correct
 *      create payload (minted id, expected_version 0, era patch); no
 *      relationship mutation fires.
 *   2. creates_era_with_parent     — patch-entity + patch-relationship both
 *      fire; the relationship carries `custom`/`parent_era`,
 *      source=parent, target=new era, symmetric=false.
 *   3. validates_required_name     — empty name shows the inline error and
 *      blocks every mutation.
 *   4. handles_validation_error_422 — daemon 422 renders
 *      `validation_summary.errors[]` in the dialog.
 *   5. handles_conflict_409        — daemon 409 renders the retry hint.
 *   6. partial_failure_relationship — entity create commits but the parent
 *      relationship fails: dialog closes + `onSuccess` fires (one submit =
 *      one era — no retry trap that would mint a duplicate era), with a
 *      warning toast for the missing parent link.
 *
 * Plus two DoD-supporting cases:
 *   7. creates_era_with_custom_type — the freeform "custom" era-type input
 *      flows into `body.attributes.era_type`.
 *   8. (canvas wiring) "新建 era" entry visible on the Brief layer + the
 *      T2-M2 alt-view precedence fix (time-bands win; toggle hidden).
 *      The entry-visibility case asserts the live gate
 *      (`showCreateEra={activeLayer === 'brief'}`); the T2-M2 precedence
 *      case stays live (it does not depend on the entry).
 *
 * The mutation hooks are stubbed (mirrors the relationship-inspector test
 * pattern); the World KB graph hook stays real and is fed through a mocked
 * `NexusClient` for the canvas-level wiring tests.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import { NexusClientError } from '@/lib/nexus/errors';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import { EraCreateDialog, type EraCreateDialogProps } from '../era-create-dialog';
import { TimelineCanvas } from '../timeline-canvas';

const patchEntityMutateAsync = vi.fn();
const patchRelationshipMutateAsync = vi.fn();

vi.mock('@/lib/canvas/use-world-kb-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-world-kb-data')>();
  return {
    ...actual,
    usePatchWorldKbEntity: () => ({
      mutateAsync: patchEntityMutateAsync,
      isPending: false,
    }),
    usePatchWorldKbRelationship: () => ({
      mutateAsync: patchRelationshipMutateAsync,
      isPending: false,
    }),
  };
});

// ─── Fixtures ──────────────────────────────────────────────────────────────

const existingEras = [
  { entity_id: 'kb-era-1', canonical_name: 'The First Age' },
  { entity_id: 'kb-era-2', canonical_name: 'The Second Age' },
];

function patchEntitySuccess() {
  patchEntityMutateAsync.mockResolvedValue({
    entity: {
      key_block_id: 'kb-new-1',
      world_id: 'w-1',
      block_type: 'era',
      canonical_name: 'The Age of Embers',
      status: 'confirmed',
      version: 1,
    } as WorldKbEntityProjection,
    version: 1,
    validation_summary: { errors: [], warnings: [] },
  });
}

function renderDialog(overrides: Partial<EraCreateDialogProps> = {}) {
  const props: EraCreateDialogProps = {
    open: true,
    onOpenChange: vi.fn(),
    onSuccess: vi.fn(),
    worldId: 'w-1',
    existingEras,
    ...overrides,
  };
  renderInApp(<EraCreateDialog {...props} />);
  return props;
}

async function fillName(name: string) {
  await userEvent.type(
    screen.getByLabelText(/Era name/i),
    name,
  );
}

async function submit() {
  await userEvent.click(screen.getByTestId('era-create-submit'));
}

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return {
    world_id: 'world-7',
    block_type: 'era',
    status: 'confirmed',
    version: 1,
    key_block_id,
    canonical_name,
    body:
      body ??
      ({
        attributes: {
          era_id: key_block_id,
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: `${canonical_name} summary`,
        },
      } as WorldKbEntityProjection['body']),
    ...rest,
  };
}

function makeMockClient(graph: WorldKbGraphResponse): NexusClient {
  return {
    getWorldKbGraph: vi.fn().mockResolvedValue(graph),
    worldKbPatchEntity: vi.fn(),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchTimelineEvent: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

const eraGraph: WorldKbGraphResponse = {
  entities: [
    eraEntity({ key_block_id: 'kb-era-1', canonical_name: 'The First Age' }),
  ],
  source_anchors: [],
  relationships: [],
};

afterEach(() => {
  vi.clearAllMocks();
});

// ─── Required dialog cases (live since V1.160 P1 T2 — F-001 closed) ───────

describe('EraCreateDialog', () => {
  it('creates_era_without_parent — patch-entity with minted id + era patch; no relationship', async () => {
    patchEntitySuccess();
    const props = renderDialog();

    await fillName('The Age of Embers');
    await submit();

    expect(patchEntityMutateAsync).toHaveBeenCalledTimes(1);
    const request = patchEntityMutateAsync.mock.calls[0][0];
    expect(request.entity_id).toMatch(/^kb_[0-9a-f]{32}$/);
    expect(request.expected_version).toBe(0);
    expect(request.patch).toEqual({
      title: 'The Age of Embers',
      body: { attributes: { world_summary: '' } },
      block_type: 'era',
    });

    // No parent selected → no relationship mutation.
    expect(patchRelationshipMutateAsync).not.toHaveBeenCalled();

    // Success closes the dialog and hands the new era id to the parent.
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    await waitFor(() => {
      expect(props.onSuccess).toHaveBeenCalledWith('kb-new-1');
    });
  });

  it('creates_era_with_parent — patch-entity + patch-relationship both fire', async () => {
    patchEntitySuccess();
    patchRelationshipMutateAsync.mockResolvedValue({
      relationship_id: 'rel-new-1',
      version: 1,
      validation_summary: { errors: [], warnings: [] },
    });
    const props = renderDialog();

    await fillName('The Age of Embers');

    // Pick a parent from the searchable combobox.
    const parentInput = screen.getByRole('combobox', { name: /Parent era/i });
    await userEvent.click(parentInput);
    await userEvent.type(parentInput, 'First');
    const listbox = screen.getByRole('listbox');
    fireEvent.mouseDown(
      within(listbox).getByText('The First Age'),
    );

    await submit();

    expect(patchEntityMutateAsync).toHaveBeenCalledTimes(1);
    const entityRequest = patchEntityMutateAsync.mock.calls[0][0];
    expect(entityRequest.patch.block_type).toBe('era');

    expect(patchRelationshipMutateAsync).toHaveBeenCalledTimes(1);
    expect(patchRelationshipMutateAsync.mock.calls[0][0]).toEqual({
      action: 'add',
      relationship: {
        source_entity_id: 'kb-era-1',
        target_entity_id: 'kb-new-1',
        relation_type: 'custom',
        custom_label: 'parent_era',
        symmetric: false,
      },
    });

    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    await waitFor(() => {
      expect(props.onSuccess).toHaveBeenCalledWith('kb-new-1');
    });
  });

  it('validates_required_name — empty name shows error; no mutation called', async () => {
    renderDialog();

    await submit();

    expect(
      screen.getByTestId('era-create-dialog-error'),
    ).toHaveTextContent(/required/i);
    expect(patchEntityMutateAsync).not.toHaveBeenCalled();
    expect(patchRelationshipMutateAsync).not.toHaveBeenCalled();
  });

  it('handles_validation_error_422 — daemon errors render in the dialog', async () => {
    patchEntityMutateAsync.mockRejectedValue(
      new NexusClientError(
        422,
        'world_kb_validation_failed',
        'invalid',
        {
          validation_summary: {
            errors: ['canonical name is too long'],
            warnings: [],
          },
        },
      ),
    );
    renderDialog();

    await fillName('The Age of Embers');
    await submit();

    await waitFor(() => {
      expect(
        screen.getByTestId('era-create-dialog-error'),
      ).toHaveTextContent('canonical name is too long');
    });
    // The dialog stays open on failure.
    expect(screen.getByLabelText(/Era name/i)).toBeInTheDocument();
  });

  it('handles_conflict_409 — conflict response shows the retry hint', async () => {
    patchEntityMutateAsync.mockRejectedValue(
      new NexusClientError(409, 'world_kb_conflict', 'stale version', {
        current_version: 1,
        entity_id: 'kb_new_1',
        conflicting_path: 'version',
      }),
    );
    renderDialog();

    await fillName('The Age of Embers');
    await submit();

    await waitFor(() => {
      expect(
        screen.getByTestId('era-create-dialog-error'),
      ).toHaveTextContent(/already exists|concurrently/i);
    });
    expect(screen.getByLabelText(/Era name/i)).toBeInTheDocument();
  });

  it('partial_failure_relationship — entity commits but parent link fails: dialog closes, onSuccess fires, warning toast', async () => {
    patchEntitySuccess();
    patchRelationshipMutateAsync.mockRejectedValue(
      new NexusClientError(500, 'internal_error', 'relationship exploded'),
    );
    const props = renderDialog();

    await fillName('The Age of Embers');

    // Pick a parent from the searchable combobox.
    const parentInput = screen.getByRole('combobox', { name: /Parent era/i });
    await userEvent.click(parentInput);
    await userEvent.type(parentInput, 'First');
    fireEvent.mouseDown(
      within(screen.getByRole('listbox')).getByText('The First Age'),
    );

    await submit();

    expect(patchRelationshipMutateAsync).toHaveBeenCalledTimes(1);

    // One submit = one era: the entity committed, so the dialog closes and
    // onSuccess fires even though the relationship step failed (a retry
    // would mint a fresh entity id → duplicate era).
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    await waitFor(() => {
      expect(props.onSuccess).toHaveBeenCalledWith('kb-new-1');
    });

    // No inline retry error — the era exists; the missing parent link is
    // surfaced as a warning toast instead.
    expect(screen.queryByTestId('era-create-dialog-error')).toBeNull();
    await waitFor(() => {
      expect(
        screen.getByText(/Era created, but the parent relationship failed/i),
      ).toBeInTheDocument();
    });
  });

  it('creates_era_with_custom_type — freeform era_type rides body.attributes', async () => {
    patchEntitySuccess();
    renderDialog();

    await fillName('The Age of Embers');
    await userEvent.selectOptions(
      screen.getByLabelText(/Era type/i),
      '__custom__',
    );
    await userEvent.type(
      screen.getByPlaceholderText(/custom era type/i),
      'golden-age',
    );
    await submit();

    const request = patchEntityMutateAsync.mock.calls[0][0];
    expect(request.patch.body).toEqual({
      attributes: { world_summary: '', era_type: 'golden-age' },
    });
    expect(patchRelationshipMutateAsync).not.toHaveBeenCalled();
  });
});

// ─── Canvas wiring: "新建 era" entry + T2-M2 alt-view precedence ───────────
// The entry-visibility case is live since V1.160 P1 T2 (F-001 closed — the
// gate is `showCreateEra={activeLayer === 'brief'}`); the T2-M2 precedence
// case pins the band-panel-vs-alt-view behavior.

describe('TimelineCanvas — Brief create entry + T2-M2 fix', () => {
  it('shows the New era entry on the Brief layer and opens the dialog', async () => {
    // V1.160 P1 T2 — the entry is live: Brief is the default layer when era
    // entities exist, and the gate `showCreateEra={activeLayer === 'brief'}`
    // renders the "新建 era" button in the header chrome.
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(eraGraph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    const entry = screen.getByTestId('timeline-create-era-entry');
    expect(entry).toBeInTheDocument();
    expect(entry).toHaveTextContent(/New era/i);

    await userEvent.click(entry);
    expect(screen.getByLabelText(/Era name/i)).toBeInTheDocument();
  });

  it('T2-M2 — Brief time-bands take precedence over the alt view; toggle hidden on Brief', async () => {
    const user = userEvent.setup();
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(eraGraph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    // Brief layer renders the time-bands panel.
    expect(screen.getByTestId('brief-time-bands')).toBeInTheDocument();

    // The alt-view toggle is NOT available on the Brief layer when the
    // time-band panel is the rendering model (T2-M2).
    expect(screen.queryByRole('button', { name: /Show list view/i })).toBeNull();

    // Switching to Narrative restores the spatial canvas + its toggle.
    await user.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.queryByTestId('brief-time-bands')).toBeNull();
    });
    expect(
      screen.getByRole('button', { name: /Show list view/i }),
    ).toBeInTheDocument();
  });
});
