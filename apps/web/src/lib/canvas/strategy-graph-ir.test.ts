/**
 * Strategy IR projection (v239-p3-canvas-compose-ui T1, T8).
 *
 * The composition path projects workflow-ir DTOs — no YAML round-trip. The
 * three-level nesting proof (T8): group → expanded slot group → nested slot /
 * slot-binding rows, parents emitted before children, children positioned
 * relative to their parent (no manual absolute positioning).
 */
import { describe, expect, it } from 'vitest';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { buildStrategyGraphFromIr } from '@/lib/canvas/strategy-graph-ir';
import { candidateDocument } from '@/test/simulation-fixture';

function composedDocument(): WorkflowDocument {
  return {
    ...candidateDocument(),
    nodes: [
      { id: 'draft', kind: 'outer_state', effect_class: 'pure' },
      {
        id: 'workshop',
        kind: 'outer_state',
        effect_class: 'read',
        enter: [{ kind: 'inner_graph', name: 'workshop-graph' }],
      },
      { id: 'workshop-graph/describe', kind: 'inner_step', effect_class: 'external' },
      { id: 'workshop-graph/critique', kind: 'inner_step', effect_class: 'read', depends_on: ['workshop-graph/describe'] },
      { id: 'collect', kind: 'pattern_slot', effect_class: 'external', pattern: { registry_id: 'prompt-step', version: '1.0.0' } },
      { id: 'done', kind: 'terminal', effect_class: 'pure' },
    ],
    edges: [
      { id: 'draft->workshop', from: 'draft', to: 'workshop', kind: 'linear' },
      { id: 'workshop->collect', from: 'workshop', to: 'collect', kind: 'linear' },
      { id: 'collect->done', from: 'collect', to: 'done', kind: 'linear' },
    ],
    nests: [
      {
        id: 'collect-region',
        parent_node: 'collect',
        expansion: 'expand_in_candidate',
        pin_policy: 'exact',
        child: { kind: 'pattern', registry_id: 'prompt-step', version: '1.0.0' },
      },
    ],
  };
}

const PATTERNS = [
  {
    package_dir: 'prompt-step',
    id: 'prompt-step',
    version: '1.0.0',
    description: 'Prompt step',
    content_hash: 'abc',
    effect_class: 'external',
    slots: [{ name: 'topic', kind: 'string', required: true }],
    requires_capabilities: [],
    available: true,
    reason: null,
  },
  {
    package_dir: 'judge-step',
    id: 'judge-step',
    version: '2.0.0',
    description: 'Judge step',
    content_hash: 'def',
    effect_class: 'read',
    slots: [],
    requires_capabilities: [],
    available: true,
    reason: null,
  },
];

describe('buildStrategyGraphFromIr', () => {
  it('projects node kinds and document edges without a YAML round-trip', () => {
    const graph = buildStrategyGraphFromIr(composedDocument());
    const byId = new Map(graph.nodes.map((node) => [node.id, node]));

    expect(byId.get('draft')?.type).toBe('strategy-state');
    expect(byId.get('workshop')?.type).toBe('strategy-group');
    expect(byId.get('done')?.type).toBe('strategy-terminal');
    expect(byId.get('collect')?.type).toBe('strategy-pattern-slot');

    const edgeKinds = new Map(graph.edges.map((edge) => [`${edge.source}->${edge.target}`, edge.data?.transitionKind]));
    expect(edgeKinds.get('draft->workshop')).toBe('next');
    expect(edgeKinds.get('workshop->collect')).toBe('next');
    expect(edgeKinds.get('collect->done')).toBe('next');
  });

  it('parents inner steps under the outer state that enters their graph and re-materialises depends_on', () => {
    const graph = buildStrategyGraphFromIr(composedDocument());
    const byId = new Map(graph.nodes.map((node) => [node.id, node]));

    const inner = byId.get('workshop-graph/critique');
    expect(inner?.parentId).toBe('workshop');
    expect(inner?.extent).toBe('parent');

    const dep = graph.edges.find(
      (edge) => edge.source === 'workshop-graph/describe' && edge.target === 'workshop-graph/critique',
    );
    expect(dep?.data?.transitionKind).toBe('depends_on');
  });

  it('emits parents before children at three nesting levels with relative child positions (T8)', () => {
    const document = composedDocument();
    // A second slot nested inside the expanded 'collect' slot (authored chrome).
    document.nodes.push({
      id: 'collect/judge',
      kind: 'pattern_slot',
      effect_class: 'read',
      pattern: { registry_id: 'judge-step', version: '2.0.0' },
    });
    const graph = buildStrategyGraphFromIr(document, {
      expandedSlots: new Set(['collect']),
      patterns: PATTERNS,
      containment: new Map([['collect/judge', 'collect']]),
    });
    const order = graph.nodes.map((node) => node.id);
    const byId = new Map(graph.nodes.map((node) => [node.id, node]));

    // Parents-first ordering: React Flow requires the parent earlier in the array.
    expect(order.indexOf('workshop')).toBeLessThan(order.indexOf('workshop-graph/describe'));
    expect(order.indexOf('collect')).toBeLessThan(order.indexOf('collect::slot-row::topic'));
    expect(order.indexOf('collect')).toBeLessThan(order.indexOf('collect/judge'));

    // Level 1 group, level 2 expanded slot group, level 3 nested slot — all
    // children carry parent-relative positions, never manual absolute ones.
    const nested = byId.get('collect/judge');
    expect(nested?.parentId).toBe('collect');
    expect(nested?.extent).toBe('parent');
    expect(nested?.position.x).toBeGreaterThanOrEqual(0);
    expect(nested?.position.x).toBeLessThan(400);
    expect(byId.get('collect')?.data.expanded).toBe(true);
    expect(byId.get('collect/judge')?.data.patternId).toBe('judge-step');
  });

  it('carries the pin and the binding diagnostic on the collapsed slot node (T2 data)', () => {
    const graph = buildStrategyGraphFromIr(composedDocument(), { patterns: PATTERNS });
    const slot = graph.nodes.find((node) => node.id === 'collect');

    expect(slot?.data.patternId).toBe('prompt-step');
    expect(slot?.data.patternVersion).toBe('1.0.0');
    expect(slot?.data.patternEffectClass).toBe('external');
    expect(slot?.data.nestId).toBe('collect-region');
    expect(slot?.data.unboundRequired).toEqual(['topic']);
    expect(slot?.data.expanded).toBe(false);
  });

  it('reports edges that name no node as dangling targets', () => {
    const document = composedDocument();
    document.edges.push({ id: 'bad', from: 'draft', to: 'missing', kind: 'linear' });
    const graph = buildStrategyGraphFromIr(document);
    expect(graph.danglingTargets).toContain('draft → missing');
  });
});
