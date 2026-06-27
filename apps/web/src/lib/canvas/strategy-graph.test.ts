import { describe, expect, it } from 'vitest';

import { parsePresetYaml } from './preset-yaml';
import { buildStrategyGraph } from './strategy-graph';

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

describe('buildStrategyGraph', () => {
  const graph = buildStrategyGraph(parsePresetYaml(SAMPLE_YAML));

  it('creates one top-level node per outer state', () => {
    const outer = graph.nodes.filter((n) => n.parentId === undefined);
    expect(outer).toHaveLength(5);
    expect(outer.map((n) => n.id).sort()).toEqual(
      ['brainstorming', 'done', 'gathering', 'merged', 'outlining'],
    );
  });

  it('marks the initial state and terminal state', () => {
    const gathering = graph.nodes.find((n) => n.id === 'gathering')!;
    expect(gathering.data.isInitial).toBe(true);
    const done = graph.nodes.find((n) => n.id === 'done')!;
    expect(done.data.isTerminal).toBe(true);
    expect(done.type).toBe('strategy-terminal');
  });

  it('renders the inner-graph state as a group with child nodes', () => {
    const group = graph.nodes.find((n) => n.id === 'brainstorming')!;
    expect(group.type).toBe('strategy-group');
    expect(group.data.innerGraphId).toBe('brainstorm_graph');
    const children = graph.nodes.filter((n) => n.parentId === 'brainstorming');
    expect(children).toHaveLength(3);
    expect(children.every((c) => c.extent === 'parent')).toBe(true);
  });

  it('renders a converge state as a join node', () => {
    const merged = graph.nodes.find((n) => n.id === 'merged')!;
    expect(merged.type).toBe('strategy-join');
    expect(merged.data.convergeStrategy).toBe('first_completed');
  });

  it('creates linear next edges and conditional branch/default edges', () => {
    const linear = graph.edges.find(
      (e) => e.source === 'gathering' && e.target === 'brainstorming',
    );
    expect(linear?.data?.transitionKind).toBe('next');

    const branch = graph.edges.find(
      (e) => e.source === 'brainstorming' && e.target === 'outlining' && e.data?.transitionKind === 'branch',
    );
    expect(branch).toBeDefined();
    expect(branch?.label).toContain('length > 2000');

    const defaultEdge = graph.edges.find(
      (e) => e.source === 'brainstorming' && e.data?.transitionKind === 'default',
    );
    expect(defaultEdge?.target).toBe('outlining');
  });

  it('creates depends_on edges inside the inner graph', () => {
    const dep = graph.edges.find(
      (e) => e.data?.transitionKind === 'depends_on' && e.target === 'brainstorming::cluster',
    );
    expect(dep?.source).toBe('brainstorming::diverge');
  });

  it('layers the initial state above its successors', () => {
    const gathering = graph.nodes.find((n) => n.id === 'gathering')!;
    const done = graph.nodes.find((n) => n.id === 'done')!;
    expect(done.position.y).toBeGreaterThan(gathering.position.y);
  });
});

describe('buildStrategyGraph — validation tolerance', () => {
  it('does not crash on a dangling `next` target (validation surfaces it instead)', () => {
    // Typo'd target — `outlinig` does not exist. The BFS should skip the
    // dangling id at dequeue time and the ValidationPanel surfaces the
    // warning. Before the fix this threw TypeError mid-BFS, which TanStack
    // Query surfaced as "Could not load the Strategy preset" and hid the
    // graph + validation panel entirely.
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
    const parsed = parsePresetYaml(yaml);
    expect(() => buildStrategyGraph(parsed)).not.toThrow();
    const { nodes, edges, danglingTargets } = buildStrategyGraph(parsed);
    expect(nodes.find((n) => n.id === 'gathering')).toBeDefined();
    expect(danglingTargets.length).toBeGreaterThan(0);
    expect(danglingTargets[0]).toContain('outlinig');
    // The dangling edge is NOT pushed (pushOuterEdge drops missing targets),
    // so the edges array has no synthetic edge for the dangling next.
    expect(edges.length).toBe(0);
  });
});
