/**
 * Timeline non-spatial alt-view (V1.122 P1 T5).
 *
 * Accessible sortable-table companion to the Timeline hero canvas, mirroring
 * the V1.114 World KB `WorldKbEntityTable` recipe (canvas-strategy-surface.md
 * §4.4 a11y parity). The table is the screen-reader + keyboard-primary
 * equivalent of the spatial when-axis projection:
 *
 *   - Every projected Timeline node (event + context KeyBlock) renders as a
 *     row with the entity's `canonical_name`, `block_type` label, optional
 *     `occurred_at` temporal signal, source-anchor count, and `updated_at`.
 *   - Rows are sortable by each column (click header to toggle asc/desc).
 *   - Row click / Enter / Space invokes `onSelectNode(nodeId)`, which the
 *     orchestrator turns into a React Flow node selection. The selection
 *     opens the `TimelineInspector`, which owns the `kb.patch_entity` write
 *     path. The alt-view itself performs NO writes (mirrors the World KB
 *     entity table — selection-only, inspector-owned writes).
 *
 * Architect lock (§4.2 write boundary): this alt-view MUST NOT wire
 * `timeline.patch_event` (Work-scoped). The negative assertion in
 * `timeline-a11y.test.tsx` enforces this — the table only renders and
 * selects.
 *
 * Honest empty-state (§7): a zero-row table renders the i18n empty copy so
 * the screen-reader live region announces the sparse-World state honestly.
 *
 * `simplify:` no windowing/virtualization. The World KB entity table windows
 * rows by scroll position because confirmed + candidate counts can climb into
 * the hundreds. The Timeline hero is MVP-scoped to World-building graphs that
 * are expected to stay < 100 entities; if a World grows past that, lift this
 * table to the shared virtualized row primitive (DF-V1122-ALT-VIEW-VIRTUAL).
 */
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Node } from '@xyflow/react';

import { BLOCK_TYPE_LABELS } from '../world-kb/types';
import type { TimelineNodeData } from './timeline-canvas-adapter';

type SortKey = 'title' | 'kind' | 'occurredAt' | 'anchors' | 'updated';
type SortDir = 'asc' | 'desc';

const COLUMN_LABEL_KEYS: Record<SortKey, string> = {
  title: 'timeline.altView.column.title',
  kind: 'timeline.altView.column.kind',
  occurredAt: 'timeline.altView.column.occurredAt',
  anchors: 'timeline.altView.column.anchors',
  updated: 'timeline.altView.column.updated',
};

const SORT_KEY_WORDS: Record<SortKey, string> = {
  title: 'timeline.altView.sortKey.title',
  kind: 'timeline.altView.sortKey.kind',
  occurredAt: 'timeline.altView.sortKey.occurredAt',
  anchors: 'timeline.altView.sortKey.anchors',
  updated: 'timeline.altView.sortKey.updated',
};

export interface TimelineAltViewProps {
  /** Projected Timeline nodes (event + context KeyBlock rows). */
  nodes: Node<TimelineNodeData>[];
  /** Currently-selected node id (highlights the matching row). */
  selectedNodeId: string | null;
  /** Selection callback — opens the inspector that owns the write path. */
  onSelectNode: (nodeId: string) => void;
}

/**
 * Render the Timeline alt-view sortable table. Reads the projected nodes
 * directly; the adapter's `TimelineAltViewWrapper` supplies them from the
 * orchestrator-owned ctxRef at render time (mirrors `WorldKbAltViewWrapper`).
 */
export function TimelineAltView({
  nodes,
  selectedNodeId,
  onSelectNode,
}: TimelineAltViewProps) {
  const { t } = useTranslation('canvas');
  const [sortKey, setSortKey] = useState<SortKey>('title');
  const [sortDir, setSortDir] = useState<SortDir>('asc');

  const sorted = useMemo(() => {
    const copy = [...nodes];
    copy.sort((a, b) => {
      const cmp = compareNodes(a.data, b.data, sortKey);
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return copy;
  }, [nodes, sortKey, sortDir]);

  function toggleSort(key: SortKey) {
    if (key === sortKey) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDir('asc');
    }
  }

  return (
    <section
      aria-label={t('timeline.altView.ariaLabel')}
      className="rounded-card border border-gray-alpha-400 bg-background-100 shadow-card"
    >
      <div className="overflow-auto" style={{ maxHeight: 'calc(100vh - 260px)' }}>
        <table className="w-full table-fixed text-copy-14">
          <caption className="sr-only">
            {t('timeline.altView.caption', {
              sortKey: t(SORT_KEY_WORDS[sortKey]),
              sortDir: t(`timeline.altView.sortDir.${sortDir}`),
            })}
          </caption>
          <thead className="sticky top-0 bg-background-200 text-left text-label-12 text-gray-700">
            <tr>
              {(Object.keys(COLUMN_LABEL_KEYS) as SortKey[]).map((key) => (
                <th
                  key={key}
                  scope="col"
                  aria-sort={
                    sortKey === key
                      ? sortDir === 'asc'
                        ? 'ascending'
                        : 'descending'
                      : 'none'
                  }
                  className="px-3 py-2"
                >
                  <button
                    type="button"
                    onClick={() => toggleSort(key)}
                    className="inline-flex items-center gap-1 hover:text-gray-1000"
                  >
                    {t(COLUMN_LABEL_KEYS[key])}
                    {sortKey === key ? (
                      <span aria-hidden>{sortDir === 'asc' ? '▲' : '▼'}</span>
                    ) : null}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.length === 0 ? (
              <tr>
                <td
                  colSpan={5}
                  className="px-3 py-6 text-center text-copy-13 text-gray-700"
                >
                  {t('timeline.altView.empty')}
                </td>
              </tr>
            ) : (
              sorted.map((node) => {
                const selected = node.id === selectedNodeId;
                const data = node.data;
                return (
                  <tr
                    key={node.id}
                    tabIndex={0}
                    onClick={() => onSelectNode(node.id)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        onSelectNode(node.id);
                      }
                    }}
                    className={[
                      'cursor-pointer border-b border-gray-alpha-200 outline-none transition-colors duration-state ease-standard focus-visible:bg-canvas-worldkb-nonspatial-row-highlight',
                      selected
                        ? 'bg-canvas-worldkb-nonspatial-row-highlight'
                        : 'hover:bg-canvas-worldkb-nonspatial-row-highlight/60',
                    ].join(' ')}
                    aria-selected={selected}
                  >
                    <td
                      className="truncate px-3 py-2 text-gray-1000"
                      title={data.canonical_name}
                    >
                      {data.canonical_name || t('timeline.altView.unnamed')}
                    </td>
                    <td className="px-3 py-2 text-gray-900">
                      {/* V1.147 P2 T3 — compute rows show the compute kind
                          label ("Compute result"), not the synthetic
                          `block_type='event'` of the log-event family. */}
                      {data.layoutHint === 'compute'
                        ? t('timeline.computeNode.kindLabel')
                        : BLOCK_TYPE_LABELS[data.block_type] ?? data.block_type}
                    </td>
                    <td
                      className="px-3 py-2 text-copy-13-mono text-gray-700"
                      title={data.occurredAtHint ?? ''}
                    >
                      {data.occurredAtHint ?? t('timeline.altView.occurredAtUnknown')}
                    </td>
                    <td className="px-3 py-2 tabular-nums text-gray-900">
                      {data.source_anchor_count ?? 0}
                    </td>
                    <td className="px-3 py-2 text-copy-13-mono text-gray-700">
                      {formatUpdated(data.updated_at)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
      <p className="border-t border-gray-alpha-200 px-3 py-2 text-label-12 text-gray-700">
        {t('timeline.altView.entryCount', { count: sorted.length })} ·{' '}
        {t('timeline.altView.listView')}
      </p>
    </section>
  );
}

// ─── Sort + format helpers ──────────────────────────────────────────────────

function compareNodes(
  a: TimelineNodeData,
  b: TimelineNodeData,
  key: SortKey,
): number {
  switch (key) {
    case 'title':
      return (a.canonical_name ?? '').localeCompare(b.canonical_name ?? '');
    case 'kind':
      // V1.156 P1 fix-wave 1 (F2) — null-safe: non-entity node families
      // (Moment scene/beat carriers, the decoration spine) carry no
      // `block_type` at runtime even though the type requires it. Mirror
      // the title sort's null-safety so the Kind sort never throws.
      return (a.block_type ?? '').localeCompare(b.block_type ?? '');
    case 'occurredAt': {
      // Undated events sort AFTER dated events (stable for both asc/desc by
      // inverting consistently — the sort dir wrapper handles the sign).
      const av = a.occurredAtHint ?? '';
      const bv = b.occurredAtHint ?? '';
      if (av === '' && bv === '') return 0;
      if (av === '') return 1;
      if (bv === '') return -1;
      return av < bv ? -1 : av > bv ? 1 : 0;
    }
    case 'anchors':
      return (a.source_anchor_count ?? 0) - (b.source_anchor_count ?? 0);
    case 'updated': {
      const at = Date.parse(a.updated_at ?? '');
      const bt = Date.parse(b.updated_at ?? '');
      if (Number.isNaN(at) && Number.isNaN(bt)) return 0;
      if (Number.isNaN(at)) return 1;
      if (Number.isNaN(bt)) return -1;
      return at - bt;
    }
  }
}

/**
 * Relative-time formatter for the `updated_at` column. Mirrors the World KB
 * entity table helper so the two surfaces share a consistent "Xd ago" voice.
 */
function formatUpdated(iso: string | undefined): string {
  if (!iso) return '—';
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return '—';
  const diff = Date.now() - t;
  const mins = Math.round(diff / 60_000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.round(hrs / 24);
  return `${days}d ago`;
}
