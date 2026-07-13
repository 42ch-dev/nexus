/**
 * Strategy graph adapter — projection tests (V1.115 P0 T2 / W001).
 *
 * Verifies that `adapter.projectGraph({ parsed })` produces identical nodes,
 * edges, and `parentId` nesting to the canonical `buildStrategyGraph` function.
 * The adapter is the single projection path — these tests target the adapter,
 * not the function directly (AC-P0-2: tests target adapter).
 *
 * Also verifies: danglingTargets byproduct is written to ctxRef, and the
 * projection is deterministic (same input → same output).
 */
import { describe, expect, it } from 'vitest';
import type { MutableRefObject } from 'react';

import { parsePresetYaml } from './preset-yaml';
import { buildStrategyGraph } from './strategy-graph';
import {
  createStrategyCanvasAdapter,
  type StrategyCanvasAdapterContext,
} from '@/components/canvas/strategy-canvas/strategy-canvas-adapter';

/** A representative preset exercising outer states, inner graph, and converge. */
const SAMPLE_YAML = `
preset:
  id: novel-writing
  version: 1
  kind: creator
  initial: gathering
  terminal: done
states:
  - id: gathering
    description: Collect inspiration.
    enter:
      - kind: capability
        name: creator.inject_prompt
    exit_when:
      kind: llm_judge
    next: brainstorming
  - id: brainstorming
    enter:
      - kind: inner_graph
        name: brainstorm_graph
    exit_when:
      kind: graph_complete
    next:
      kind: conditional
      rules:
        - when: "{{output | length > 2000}}"
          to: outlining
        - when: "{{output | contains 'unclear'}}"
          to: gathering
      default: outlining
  - id: outlining
    enter:
      - kind: capability
        name: creator.inject_prompt
    exit_when:
      kind: manual
    next: merged
  - id: merged
    converge:
      strategy: first_completed
    enter: []
    exit_when:
      kind: manual
    next: done
  - id: done
    terminal: true
inner_graphs:
  brainstorm_graph:
    nodes:
      - id: diverge
        kind: acp_prompt
        template_file: prompts/brainstorm-diverge.md
      - id: cluster
        kind: acp_prompt
        depends_on: [diverge]
        template_file: prompts/brainstorm-cluster.md
      - id: select
        kind: acp_prompt
        depends_on: [cluster]
        template_file: prompts/brainstorm-select.md
    output_binding: select.text
`;

// ---------------------------------------------------------------------------
// Adapter helpers — minimal ctxRef for projection tests
// ---------------------------------------------------------------------------

function makeCtxRef() {
  // Minimal ctxRef — projectGraph only reads/writes danglingTargets and localEdges.
  // Cast through unknown because the full StrategyCanvasAdapterContext has many
  // fields used only by renderInspector/adaptConflict, not by projectGraph.
  return { current: { danglingTargets: [], localEdges: [] } } as unknown as MutableRefObject<
    StrategyCanvasAdapterContext
  >;
}

function project(yaml: string) {
  const ctxRef = makeCtxRef();
  const adapter = createStrategyCanvasAdapter(ctxRef);
  const parsed = parsePresetYaml(yaml);
  const result = adapter.projectGraph({ parsed, revision: parsed.revision ?? 0, activeSession: null });
  return { result, ctxRef, parsed };
}

// ---------------------------------------------------------------------------

describe('parsePresetYaml', () => {
  it('parses a well-formed preset manifest', () => {
    const { manifest, problems } = parsePresetYaml(SAMPLE_YAML);
    expect(problems).toEqual([]);
    expect(manifest.preset.id).toBe('novel-writing');
    expect(manifest.preset.initial).toBe('gathering');
    expect(manifest.states).toHaveLength(5);
    expect(manifest.states[0].id).toBe('gathering');
    expect(manifest.inner_graphs?.brainstorm_graph.nodes).toHaveLength(3);
  });

  it('reports a problem for unparseable YAML', () => {
    const { manifest, problems } = parsePresetYaml('preset: { id: "x"\n  bad: [:');
    expect(problems.length).toBeGreaterThan(0);
    expect(manifest.preset.id).toBe('unknown');
    expect(manifest.states).toEqual([]);
  });

  it('reports a problem when initial references an unknown state', () => {
    const { problems } = parsePresetYaml(`
preset: { id: p, initial: nope }
states:
  - id: a
    next: b
`);
    expect(problems.some((p) => p.includes('nope'))).toBe(true);
  });
});

describe('StrategyCanvasAdapter.projectGraph — projection equivalence', () => {
  it('produces identical nodes/edges to buildStrategyGraph (adapter owns projection)', () => {
    const parsed = parsePresetYaml(SAMPLE_YAML);
    const { result } = project(SAMPLE_YAML);
    const direct = buildStrategyGraph(parsed);
    expect(result.nodes).toEqual(direct.nodes);
    expect(result.edges).toEqual(direct.edges);
  });

  it('creates one top-level node per outer state', () => {
    const { result } = project(SAMPLE_YAML);
    const outer = result.nodes.filter((n) => n.parentId === undefined);
    expect(outer).toHaveLength(5);
    expect(outer.map((n) => n.id).sort()).toEqual(
      ['brainstorming', 'done', 'gathering', 'merged', 'outlining'],
    );
  });

  it('marks the initial state and terminal state', () => {
    const { result } = project(SAMPLE_YAML);
    const gathering = result.nodes.find((n) => n.id === 'gathering')!;
    expect(gathering.data.isInitial).toBe(true);
    const done = result.nodes.find((n) => n.id === 'done')!;
    expect(done.data.isTerminal).toBe(true);
    expect(done.type).toBe('strategy-terminal');
  });

  it('renders the inner-graph state as a group with child nodes', () => {
    const { result } = project(SAMPLE_YAML);
    const group = result.nodes.find((n) => n.id === 'brainstorming')!;
    expect(group.type).toBe('strategy-group');
    expect(group.data.innerGraphId).toBe('brainstorm_graph');
    const children = result.nodes.filter((n) => n.parentId === 'brainstorming');
    expect(children).toHaveLength(3);
    expect(children.every((c) => c.extent === 'parent')).toBe(true);
  });

  it('renders a converge state as a join node', () => {
    const { result } = project(SAMPLE_YAML);
    const merged = result.nodes.find((n) => n.id === 'merged')!;
    expect(merged.type).toBe('strategy-join');
    expect(merged.data.convergeStrategy).toBe('first_completed');
  });

  it('creates linear next edges and conditional branch/default edges', () => {
    const { result } = project(SAMPLE_YAML);
    const linear = result.edges.find(
      (e) => e.source === 'gathering' && e.target === 'brainstorming',
    );
    expect(linear?.data?.transitionKind).toBe('next');

    const branch = result.edges.find(
      (e) => e.source === 'brainstorming' && e.target === 'outlining' && e.data?.transitionKind === 'branch',
    );
    expect(branch).toBeDefined();
    expect(branch?.label).toContain('length > 2000');

    const defaultEdge = result.edges.find(
      (e) => e.source === 'brainstorming' && e.data?.transitionKind === 'default',
    );
    expect(defaultEdge?.target).toBe('outlining');
  });

  it('creates depends_on edges inside the inner graph', () => {
    const { result } = project(SAMPLE_YAML);
    const dep = result.edges.find(
      (e) => e.data?.transitionKind === 'depends_on' && e.target === 'brainstorming::cluster',
    );
    expect(dep?.source).toBe('brainstorming::diverge');
  });

  it('layers the initial state above its successors', () => {
    const { result } = project(SAMPLE_YAML);
    const gathering = result.nodes.find((n) => n.id === 'gathering')!;
    const done = result.nodes.find((n) => n.id === 'done')!;
    expect(done.position.y).toBeGreaterThan(gathering.position.y);
  });

  it('is deterministic — same input produces same output', () => {
    const first = project(SAMPLE_YAML);
    const second = project(SAMPLE_YAML);
    expect(second.result.nodes).toEqual(first.result.nodes);
    expect(second.result.edges).toEqual(first.result.edges);
  });
});

describe('StrategyCanvasAdapter.projectGraph — validation tolerance', () => {
  it('does not crash on a dangling `next` target and surfaces it via ctxRef.danglingTargets', () => {
    // Typo'd target — `outlinig` does not exist. The BFS should skip the
    // dangling id at dequeue time and the adapter surfaces the warning via
    // ctxRef.current.danglingTargets for the ValidationPanel.
    const yaml = `
preset:
  id: typo-preset
  version: 1
  kind: creator
  initial: gathering
  terminal: done
states:
  - id: gathering
    next: outlinig
  - id: done
`;
    const { result, ctxRef } = project(yaml);
    expect(result.nodes.find((n) => n.id === 'gathering')).toBeDefined();
    expect(ctxRef.current.danglingTargets.length).toBeGreaterThan(0);
    expect(ctxRef.current.danglingTargets[0]).toContain('outlinig');
    // The dangling edge is NOT pushed (pushOuterEdge drops missing targets),
    // so the edges array has no synthetic edge for the dangling next.
    expect(result.edges.length).toBe(0);
  });
});
