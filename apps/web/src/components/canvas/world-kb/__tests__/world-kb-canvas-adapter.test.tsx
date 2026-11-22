/**
 * World KB canvas adapter — inspector routing tests (V1.115 P0 T3 / W002).
 *
 * Verifies that `renderInspector(node)` routes entity-vs-candidate from the
 * passed `node.data` (the contract authority), NOT from `ctxRef.current.selection`.
 * Before the fix the wrapper ignored `node` and read `selection`; if `selection`
 * was stale or pointed at a different node, the wrong inspector rendered.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Node } from '@xyflow/react';

import { makeQueryClient } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
} from '@42ch/nexus-contracts';

import { createWorldKbCanvasAdapter, type WorldKbCanvasAdapterContext } from '../world-kb-canvas-adapter';
import type { WorldKbNodeData } from '../types';

// Stub the data hooks so EntityInspector / PromotionInspector do not hit the
// network. We are testing ROUTING (which inspector + which projection), not
// submit behavior — that is covered by entity-inspector / promotion-inspector
// test suites.
const mocks = vi.hoisted(() => ({
  patchEntity: { mutate: vi.fn(), isPending: false },
  promote: { mutate: vi.fn(), isPending: false },
}));

vi.mock('@/lib/canvas/use-world-kb-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-world-kb-data')>();
  return {
    ...actual,
    usePatchWorldKbEntity: () => mocks.patchEntity,
    usePromoteWorldKbCandidate: () => mocks.promote,
  };
});

const noopClient = {
  health: () => Promise.resolve({ ok: true }),
} as unknown as NexusClient;

function renderWith(ui: React.ReactElement) {
  return render(
    <QueryClientProvider client={makeQueryClient()}>
      <ToastProvider>
        <ClientProvider client={noopClient}>{ui}</ClientProvider>
        <Toaster />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

function entity(id: string, name: string): WorldKbEntityProjection {
  return {
    key_block_id: id,
    world_id: 'w-1',
    block_type: 'character',
    canonical_name: name,
    status: 'confirmed',
    version: 1,
  };
}

function candidate(id: string, name: string): WorldKbCandidateProjection {
  return {
    candidate_id: id,
    job_id: 'job-1',
    world_id: 'w-1',
    block_type: 'character',
    canonical_name: name,
    version: 1,
  } as unknown as WorldKbCandidateProjection;
}

function entityNode(id: string, name: string): Node<WorldKbNodeData> {
  return {
    id: `entity:${id}`,
    type: 'worldkb-entity',
    position: { x: 0, y: 0 },
    data: {
      worldId: 'w-1',
      keyBlockId: id,
      entityKind: 'character',
      name,
      lifecycle: 'confirmed',
      version: 1,
      sourceAnchorCount: 0,
      computable: false,
    },
  };
}

function candidateNode(id: string, name: string): Node<WorldKbNodeData> {
  return {
    id: `candidate:${id}`,
    type: 'worldkb-entity',
    position: { x: 0, y: 0 },
    data: {
      worldId: 'w-1',
      candidateId: id,
      jobId: 'job-1',
      entityKind: 'character',
      name,
      lifecycle: 'pending',
      version: 1,
      sourceAnchorCount: 0,
      computable: false,
    },
  };
}

function makeContext(overrides: Partial<WorldKbCanvasAdapterContext> = {}): WorldKbCanvasAdapterContext {
  return {
    worldId: 'w-1',
    // Intentionally stale — the test verifies the adapter does NOT route here.
    selection: null,
    entities: [],
    candidates: [],
    confirmedEntities: [],
    anchors: [],
    relationships: [],
    reseedSignal: 0,
    onEntityConflict: vi.fn(),
    onPromoteConflict: vi.fn(),
    onRelationshipConflict: vi.fn(),
    onRelationshipSaved: vi.fn(),
    onSelectNode: vi.fn(),
    onSelectRelationship: vi.fn(),
    onCreateRelationship: vi.fn(),
    onDeleteRelationship: vi.fn(),
    onPromoteSuggestion: vi.fn(),
    onDeleteSuggestion: vi.fn(),
    onPromoteAllSuggestions: vi.fn(),
    patchRelationshipIsPending: false,
    onActiveTabChange: vi.fn(),
    selectedNodeId: null,
    selectedRelationshipId: null,
    nodes: [],
    ...overrides,
  };
}

describe('WorldKbCanvasAdapter.renderInspector — node-parameter authority (W002)', () => {
  it('renders the entity for the passed node, not whatever selection holds', () => {
    const entityA = entity('kb-A', 'Aria Stormwind');
    const entityB = entity('kb-B', 'Bran Halloway');
    const ctxRef = { current: makeContext({
      // selection points at entity B (stale / out of sync).
      selection: { kind: 'entity', node: { name: 'Bran Halloway' } as unknown as WorldKbNodeData, entity: entityB },
      entities: [entityA, entityB],
    }) };

    const adapter = createWorldKbCanvasAdapter(ctxRef);
    const inspector = adapter.renderInspector!(entityNode('kb-A', 'Aria Stormwind'));

    renderWith(<>{inspector}</>);

    // EntityInspector seeds its title input from entity.canonical_name.
    // Must show Aria (the passed node), NOT Bran (the stale selection).
    expect(screen.getByDisplayValue('Aria Stormwind')).toBeInTheDocument();
    expect(screen.queryByDisplayValue('Bran Halloway')).not.toBeInTheDocument();
  });

  it('renders the candidate for the passed node, ignoring a stale selection', () => {
    const candidateA = candidate('c-A', 'Elena Vale');
    const entityB = entity('kb-B', 'Bran Halloway');
    const ctxRef = { current: makeContext({
      // selection points at an entity — totally wrong kind for the passed node.
      selection: { kind: 'entity', node: {} as unknown as WorldKbNodeData, entity: entityB },
      candidates: [candidateA],
      confirmedEntities: [entityB],
    }) };

    const adapter = createWorldKbCanvasAdapter(ctxRef);
    const inspector = adapter.renderInspector!(candidateNode('c-A', 'Elena Vale'));

    renderWith(<>{inspector}</>);

    // PromotionInspector shows the candidate canonical_name in a definition row.
    expect(screen.getByText('Elena Vale')).toBeInTheDocument();
    expect(screen.queryByText('Bran Halloway')).not.toBeInTheDocument();
  });

  it('renders nothing when the passed node has no entity or candidate id', () => {
    const ctxRef = { current: makeContext({
      entities: [entity('kb-1', 'Aria')],
    }) };

    const adapter = createWorldKbCanvasAdapter(ctxRef);
    const node: Node<WorldKbNodeData> = {
      id: 'orphan',
      type: 'worldkb-entity',
      position: { x: 0, y: 0 },
      data: {
        worldId: 'w-1',
        entityKind: 'character',
        name: 'Orphan',
        lifecycle: 'confirmed',
        version: 1,
        sourceAnchorCount: 0,
        computable: false,
      },
    };
    const { container } = renderWith(<>{adapter.renderInspector!(node)}</>);
    expect(container.textContent).toBe('');
  });

  it('renders nothing when the entity projection is not found in the graph lists', () => {
    const ctxRef = { current: makeContext({
      entities: [], // node references an entity that was dropped from the graph
    }) };

    const adapter = createWorldKbCanvasAdapter(ctxRef);
    const { container } = renderWith(<>{adapter.renderInspector!(entityNode('kb-gone', 'Ghost'))}</>);
    expect(container.textContent).toBe('');
  });
});
