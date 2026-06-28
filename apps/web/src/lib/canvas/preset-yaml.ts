/**
 * Preset YAML parser — client-side projection of a preset bundle manifest.
 *
 * `GET /v1/local/presets/{id}` returns `GetPresetResponse { yaml: string }`
 * (A5 verdict — option a: existing endpoint suffices, no new read route). The
 * canvas parses that raw YAML into the typed subset defined in
 * `orchestration-engine.md` §7.2 and feeds it to the Strategy graph adapter.
 *
 * This is a **read projection** — it never writes back. The daemon's preset
 * validator remains authoritative; this parser is lenient about unknown keys
 * so a newer preset grammar does not crash an older canvas.
 */
import { parse as parseYaml } from 'yaml';

/** Converge (merge-point) strategy — orchestration-engine.md §7.5. */
export type ConvergeStrategy = 'wait_for_all' | 'first_completed' | 'any';

/** A single `enter` task on a state (capability / inner_graph / acp_prompt …). */
export interface PresetEnterTask {
  kind: string;
  name?: string;
  args?: Record<string, unknown>;
}

/** Conditional `next` form — orchestration-engine.md §7.5. */
export interface PresetConditionalNext {
  kind?: 'conditional';
  rules?: Array<{ when?: string; to: string }>;
  default?: string;
}

/** Context-update hook attached to a state (orchestration-engine.md §7). */
export interface PresetContextUpdate {
  op?: { kind?: string };
  template_file?: string;
}

/** One outer state-machine state — orchestration-engine.md §7.2. */
export interface PresetState {
  id: string;
  description?: string;
  enter?: PresetEnterTask[];
  exit_when?: { kind?: string };
  /** Linear target (string) or conditional form (object). */
  next?: string | PresetConditionalNext;
  converge?: { strategy?: ConvergeStrategy };
  terminal?: boolean;
  /** Optional context-update hook (may carry a prompt `template_file`). */
  context_update?: PresetContextUpdate;
}

/** A node inside an inner DAG — orchestration-engine.md §7.2 `inner_graphs`. */
export interface PresetInnerNode {
  id: string;
  kind: string;
  depends_on?: string[];
  template_file?: string;
}

/** A named inner DAG launched by an `inner_graph` enter task. */
export interface PresetInnerGraph {
  nodes: PresetInnerNode[];
  output_binding?: string;
}

/** The typed subset of `preset.yaml` the canvas reads. */
export interface PresetManifest {
  preset: {
    id: string;
    version?: number;
    kind?: string;
    description?: string;
    initial?: string;
    terminal?: string;
  };
  states: PresetState[];
  inner_graphs?: Record<string, PresetInnerGraph>;
}

export interface ParsedPreset {
  manifest: PresetManifest;
  /** Bundle revision from the YAML `revision:` header (used for patch consistency). */
  revision?: number;
  /** Non-fatal parse/validation problems (empty when clean). */
  problems: string[];
}

/**
 * Parse a preset manifest YAML string into the typed canvas projection.
 *
 * Lenient: unknown top-level keys and unknown per-state fields are ignored so a
 * newer grammar does not break an older canvas. Hard failures (not YAML, no
 * `preset.id`, no `states`) are reported in `problems` and a best-effort
 * manifest is returned so the UI can still render a partial graph.
 */
export function parsePresetYaml(yaml: string): ParsedPreset {
  const problems: string[] = [];
  let root: unknown;
  try {
    root = parseYaml(yaml);
  } catch (err) {
    problems.push(
      `preset.yaml could not be parsed: ${err instanceof Error ? err.message : String(err)}`,
    );
    return { manifest: { preset: { id: 'unknown' }, states: [] }, problems };
  }

  if (root === null || typeof root !== 'object') {
    problems.push('preset.yaml is empty or not a mapping.');
    return { manifest: { preset: { id: 'unknown' }, states: [] }, problems };
  }

  const obj = root as Record<string, unknown>;
  const presetRaw = obj.preset;
  if (presetRaw === null || typeof presetRaw !== 'object') {
    problems.push('preset.yaml is missing the top-level `preset` mapping.');
    return { manifest: { preset: { id: 'unknown' }, states: [] }, problems };
  }

  const preset = presetRaw as Record<string, unknown>;
  const id = typeof preset.id === 'string' ? preset.id : 'unknown';
  if (id === 'unknown') problems.push('preset.id is missing or not a string.');

  const statesRaw = obj.states;
  let states: PresetState[] = [];
  if (Array.isArray(statesRaw)) {
    states = statesRaw.map((s, i) => coerceState(s, i, problems)).filter((s): s is PresetState => s !== null);
  } else {
    problems.push('preset.yaml is missing the `states` array.');
  }

  const innerGraphs = coerceInnerGraphs(obj.inner_graphs);

  const revision = typeof obj.revision === 'number' ? obj.revision : undefined;

  const manifest: PresetManifest = {
    preset: {
      id,
      version: typeof preset.version === 'number' ? preset.version : undefined,
      kind: typeof preset.kind === 'string' ? preset.kind : undefined,
      description: typeof preset.description === 'string' ? preset.description : undefined,
      initial: typeof preset.initial === 'string' ? preset.initial : undefined,
      terminal: typeof preset.terminal === 'string' ? preset.terminal : undefined,
    },
    states,
    inner_graphs: innerGraphs,
  };

  if (manifest.preset.initial && !states.some((s) => s.id === manifest.preset.initial)) {
    problems.push(`preset.initial references unknown state "${manifest.preset.initial}".`);
  }

  return { manifest, revision, problems };
}

function coerceState(raw: unknown, index: number, problems: string[]): PresetState | null {
  if (raw === null || typeof raw !== 'object') {
    problems.push(`states[${index}] is not a mapping; skipped.`);
    return null;
  }
  const s = raw as Record<string, unknown>;
  const id = typeof s.id === 'string' ? s.id : undefined;
  if (!id) {
    problems.push(`states[${index}] is missing an id; skipped.`);
    return null;
  }

  const enter = Array.isArray(s.enter) ? s.enter.map((e) => coerceEnter(e)).filter((e): e is PresetEnterTask => e !== null) : undefined;

  const next = coerceNext(s.next);

  const convergeRaw = s.converge;
  let converge: PresetState['converge'];
  if (convergeRaw !== null && typeof convergeRaw === 'object') {
    const strat = (convergeRaw as Record<string, unknown>).strategy;
    if (strat === 'wait_for_all' || strat === 'first_completed' || strat === 'any') {
      converge = { strategy: strat };
    } else if (strat === undefined) {
      converge = { strategy: 'wait_for_all' };
    }
  }

  return {
    id,
    description: typeof s.description === 'string' ? s.description : undefined,
    enter,
    exit_when: s.exit_when !== null && typeof s.exit_when === 'object'
      ? { kind: typeof (s.exit_when as Record<string, unknown>).kind === 'string' ? (s.exit_when as Record<string, unknown>).kind as string : undefined }
      : undefined,
    next,
    converge,
    terminal: s.terminal === true,
    context_update: coerceContextUpdate(s.context_update),
  };
}

function coerceEnter(raw: unknown): PresetEnterTask | null {
  if (raw === null || typeof raw !== 'object') return null;
  const e = raw as Record<string, unknown>;
  if (typeof e.kind !== 'string') return null;
  return {
    kind: e.kind,
    name: typeof e.name === 'string' ? e.name : undefined,
    args: e.args !== null && typeof e.args === 'object' ? (e.args as Record<string, unknown>) : undefined,
  };
}

function coerceContextUpdate(raw: unknown): PresetContextUpdate | undefined {
  if (raw === null || typeof raw !== 'object') return undefined;
  const c = raw as Record<string, unknown>;
  const opRaw = c.op;
  const op = opRaw !== null && typeof opRaw === 'object'
    ? { kind: typeof (opRaw as Record<string, unknown>).kind === 'string' ? (opRaw as Record<string, unknown>).kind as string : undefined }
    : undefined;
  const templateFile = typeof c.template_file === 'string' ? c.template_file : undefined;
  if (op === undefined && templateFile === undefined) return undefined;
  return { op, template_file: templateFile };
}

function coerceNext(raw: unknown): PresetState['next'] {
  if (typeof raw === 'string') return raw;
  if (raw === null || typeof raw !== 'object') return undefined;
  const n = raw as Record<string, unknown>;
  if (typeof n.default === 'string' || Array.isArray(n.rules)) {
    const rules: Array<{ when?: string; to: string }> = [];
    if (Array.isArray(n.rules)) {
      for (const r of n.rules) {
        if (r === null || typeof r !== 'object') continue;
        const rr = r as Record<string, unknown>;
        if (typeof rr.to !== 'string') continue;
        const when = typeof rr.when === 'string' ? rr.when : undefined;
        rules.push(when !== undefined ? { when, to: rr.to } : { to: rr.to });
      }
    }
    return {
      kind: n.kind === 'conditional' ? 'conditional' : undefined,
      rules: rules.length > 0 ? rules : undefined,
      default: typeof n.default === 'string' ? n.default : undefined,
    };
  }
  return undefined;
}

function coerceInnerGraphs(raw: unknown): Record<string, PresetInnerGraph> | undefined {
  if (raw === null || typeof raw !== 'object') return undefined;
  const out: Record<string, PresetInnerGraph> = {};
  for (const [name, graph] of Object.entries(raw as Record<string, unknown>)) {
    if (graph === null || typeof graph !== 'object') continue;
    const g = graph as Record<string, unknown>;
    const nodes: PresetInnerNode[] = [];
    if (Array.isArray(g.nodes)) {
      for (const n of g.nodes) {
        if (n === null || typeof n !== 'object') continue;
        const nn = n as Record<string, unknown>;
        if (typeof nn.id !== 'string') continue;
        const node: PresetInnerNode = {
          id: nn.id,
          kind: typeof nn.kind === 'string' ? nn.kind : 'unknown',
        };
        if (Array.isArray(nn.depends_on)) {
          node.depends_on = nn.depends_on.filter((d): d is string => typeof d === 'string');
        }
        if (typeof nn.template_file === 'string') {
          node.template_file = nn.template_file;
        }
        nodes.push(node);
      }
    }
    out[name] = { nodes, output_binding: typeof g.output_binding === 'string' ? g.output_binding : undefined };
  }
  return out;
}

/** Resolve a state's primary "kind" for node rendering (Draft §3.2). */
export function stateKind(state: PresetState): string {
  if (state.terminal) return 'terminal';
  if (state.converge) return 'converge';
  const firstEnter = state.enter?.[0]?.kind;
  return firstEnter ?? 'unknown';
}

/** The inner-graph id launched by a state, if any. */
export function innerGraphIdOf(state: PresetState): string | undefined {
  return state.enter?.find((e) => e.kind === 'inner_graph')?.name;
}
