/**
 * Strategy alternate view — non-spatial companion to the graph
 * (canvas-strategy-surface.md Draft §4.4 #2).
 *
 * Every canvas must have a list/tree/table companion so the Strategy is
 * understandable without spatial navigation. This renders:
 *   1. States in execution order (BFS from initial) with kind + status.
 *   2. A branch table (source → condition → target, transition kind).
 *
 * This is both accessibility and productivity: keyboard users and screen
 * readers get a linear reading order, and authors can scan transitions that a
 * spatial graph hides.
 */
import type { ParsedPreset } from '@/lib/canvas/preset-yaml';
import { innerGraphIdOf, stateKind } from '@/lib/canvas/preset-yaml';

export interface StrategyAltViewProps {
  parsed: ParsedPreset;
  /** Optional overlay status by state id (from the live session). */
  statusByState?: Record<string, string>;
}

/** BFS execution order from the initial state (falls back to declared order). */
function executionOrder(parsed: ParsedPreset): string[] {
  const { manifest } = parsed;
  const byId = new Map(manifest.states.map((s) => [s.id, s]));
  const initial = manifest.preset.initial;
  if (!initial || !byId.has(initial)) return manifest.states.map((s) => s.id);

  const order: string[] = [];
  const seen = new Set<string>();
  const queue: string[] = [initial];
  seen.add(initial);
  while (queue.length > 0) {
    const id = queue.shift()!;
    order.push(id);
    const state = byId.get(id);
    if (!state) continue;
    const next = state.next;
    const targets: string[] =
      typeof next === 'string'
        ? [next]
        : next && typeof next === 'object'
          ? [...(next.rules ?? []).map((r) => r.to), ...(next.default ? [next.default] : [])]
          : [];
    for (const t of targets) {
      if (byId.has(t) && !seen.has(t)) {
        seen.add(t);
        queue.push(t);
      }
    }
  }
  // Append any unreachable states so nothing is silently dropped.
  for (const s of manifest.states) if (!seen.has(s.id)) order.push(s.id);
  return order;
}

export function StrategyAltView({ parsed, statusByState }: StrategyAltViewProps) {
  const { manifest } = parsed;
  const byId = new Map(manifest.states.map((s) => [s.id, s]));
  const order = executionOrder(parsed);

  const branches: Array<{ source: string; condition?: string; target: string; kind: string }> = [];
  for (const state of manifest.states) {
    const next = state.next;
    if (typeof next === 'string') {
      branches.push({ source: state.id, target: next, kind: 'next' });
    } else if (next && typeof next === 'object') {
      for (const rule of next.rules ?? []) {
        branches.push({ source: state.id, condition: rule.when, target: rule.to, kind: 'branch' });
      }
      if (next.default) branches.push({ source: state.id, target: next.default, kind: 'default' });
    }
  }

  return (
    <section
      aria-label="Strategy states in execution order"
      className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
    >
      <h3 className="text-heading-16 font-heading text-gray-1000">States (execution order)</h3>
      <ol className="mt-2 flex flex-col gap-1">
        {order.map((id, i) => {
          const state = byId.get(id);
          if (!state) return null;
          const kind = stateKind(state);
          const graph = innerGraphIdOf(state);
          const status = statusByState?.[id];
          return (
            <li
              key={id}
              className="flex items-center gap-2 rounded-control px-2 py-1 text-copy-14"
            >
              <span className="w-6 shrink-0 text-copy-13-mono text-gray-700 tabular-nums">{i + 1}.</span>
              <span className="font-mono text-gray-1000">{id}</span>
              <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">{kind}</span>
              {graph ? <span className="text-label-12 text-purple-700">inner: {graph}</span> : null}
              {state.converge ? (
                <span className="rounded-pill bg-[color-mix(in_srgb,var(--color-purple-700)_12%,transparent)] px-1.5 py-0.5 text-label-12 text-purple-1000">
                  join · {state.converge.strategy ?? 'wait_for_all'}
                </span>
              ) : null}
              {status ? <span className="text-label-12 text-blue-700">{status}</span> : null}
            </li>
          );
        })}
      </ol>

      <h3 className="mt-4 text-heading-16 font-heading text-gray-1000">Transitions</h3>
      <div className="mt-2 overflow-x-auto">
        <table className="w-full text-copy-14">
          <thead>
            <tr className="border-b border-gray-alpha-400 text-left text-label-12 text-gray-700">
              <th className="py-1 pr-3">From</th>
              <th className="py-1 pr-3">Condition</th>
              <th className="py-1 pr-3">To</th>
              <th className="py-1">Kind</th>
            </tr>
          </thead>
          <tbody>
            {branches.map((b, i) => (
              <tr key={`${b.source}-${b.target}-${i}`} className="border-b border-gray-alpha-200">
                <td className="py-1 pr-3 font-mono text-gray-1000">{b.source}</td>
                <td className="py-1 pr-3 text-gray-900">{b.condition ?? '—'}</td>
                <td className="py-1 pr-3 font-mono text-gray-1000">{b.target}</td>
                <td className="py-1 text-gray-700">{b.kind}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
