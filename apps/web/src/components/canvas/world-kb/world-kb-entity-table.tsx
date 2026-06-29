/**
 * World KB non-spatial entity table (V1.73 P0 A6/A8).
 *
 * Accessible equivalent to the entity graph: sortable columns, keyboard-focusable
 * rows, and Enter/Space activation to open the inspector.
 *
 * Virtualization: rows are windowed by scroll position with a fixed row height.
 */
import { useLayoutEffect, useMemo, useRef, useState } from 'react';

import { BLOCK_TYPE_LABELS, worldKbNodeId, type EntityLifecycle, type WorldKbNodeData } from './types';

type SortKey = 'name' | 'blockType' | 'lifecycle' | 'anchors' | 'updated';
type SortDir = 'asc' | 'desc';

const ROW_HEIGHT = 44;
const OVERSCAN = 8;

const LIFECYCLE_RANK: Record<EntityLifecycle, number> = {
  pending: 0,
  confirmed: 1,
  merged: 2,
  rejected: 3,
};

const LIFECYCLE_BADGE_CLASS: Record<EntityLifecycle, string> = {
  pending: 'text-canvas-worldkb-promotion-pending',
  confirmed: 'text-canvas-worldkb-promotion-confirmed',
  rejected: 'text-canvas-worldkb-promotion-rejected',
  merged: 'text-canvas-worldkb-promotion-merged',
};

const COLUMN_LABELS: Record<SortKey, string> = {
  name: 'Name',
  blockType: 'Block Type',
  lifecycle: 'Lifecycle',
  anchors: 'Source Anchors',
  updated: 'Updated',
};

export interface WorldKbEntityTableProps {
  nodes: WorldKbNodeData[];
  selectedId: string | null;
  onSelect: (node: WorldKbNodeData) => void;
}

export function WorldKbEntityTable({ nodes, selectedId, onSelect }: WorldKbEntityTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>('name');
  const [sortDir, setSortDir] = useState<SortDir>('asc');
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportH, setViewportH] = useState(480);
  const scrollRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    setViewportH(el.clientHeight);
    const onScroll = () => setScrollTop(el.scrollTop);
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  const sorted = useMemo(() => {
    const copy = [...nodes];
    copy.sort((a, b) => {
      const cmp = compare(a, b, sortKey);
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return copy;
  }, [nodes, sortKey, sortDir]);

  const startIdx = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const visibleCount = Math.ceil(viewportH / ROW_HEIGHT) + OVERSCAN * 2;
  const endIdx = Math.min(sorted.length, startIdx + visibleCount);
  const visible = sorted.slice(startIdx, endIdx);

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
      aria-label="World KB entities (sortable list)"
      className="rounded-card border border-gray-alpha-400 bg-background-100 shadow-card"
    >
      <div className="overflow-auto" style={{ maxHeight: 'calc(100vh - 260px)' }} ref={scrollRef}>
        <table className="w-full table-fixed text-copy-14">
          <caption className="sr-only">
            World KB entities sorted by {COLUMN_LABELS[sortKey]} ({sortDir}). Press Enter on a row to
            open its inspector.
          </caption>
          <thead className="sticky top-0 bg-background-200 text-left text-label-12 text-gray-700">
            <tr>
              {(Object.keys(COLUMN_LABELS) as SortKey[]).map((key) => (
                <th
                  key={key}
                  scope="col"
                  aria-sort={sortKey === key ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}
                  className="px-3 py-2"
                >
                  <button
                    type="button"
                    onClick={() => toggleSort(key)}
                    className="inline-flex items-center gap-1 hover:text-gray-1000"
                  >
                    {COLUMN_LABELS[key]}
                    {sortKey === key ? <span aria-hidden>{sortDir === 'asc' ? '▲' : '▼'}</span> : null}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-center text-copy-13 text-gray-700">
                  No World KB entities yet.
                </td>
              </tr>
            ) : (
              <>
                {startIdx > 0 ? (
                  <tr style={{ height: startIdx * ROW_HEIGHT }} aria-hidden>
                    <td colSpan={5} />
                  </tr>
                ) : null}
                {visible.map((node) => {
                  const id = worldKbNodeId(node);
                  const selected = id === selectedId;
                  return (
                    <tr
                      key={id}
                      tabIndex={0}
                      onClick={() => onSelect(node)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          onSelect(node);
                        }
                      }}
                      className={[
                        'cursor-pointer border-b border-gray-alpha-200 outline-none transition-colors duration-state ease-standard focus-visible:bg-canvas-worldkb-nonspatial-row-highlight',
                        selected ? 'bg-canvas-worldkb-nonspatial-row-highlight' : 'hover:bg-canvas-worldkb-nonspatial-row-highlight/60',
                      ].join(' ')}
                      style={{ height: ROW_HEIGHT }}
                      aria-selected={selected}
                    >
                      <td className="truncate px-3 py-2 text-gray-1000" title={node.name}>
                        {node.name || '(unnamed)'}
                      </td>
                      <td className="px-3 py-2 text-gray-900">{BLOCK_TYPE_LABELS[node.entityKind]}</td>
                      <td className={`px-3 py-2 capitalize ${LIFECYCLE_BADGE_CLASS[node.lifecycle]}`}>
                        {node.lifecycle}
                      </td>
                      <td className="px-3 py-2 tabular-nums text-gray-900">{node.sourceAnchorCount}</td>
                      <td className="px-3 py-2 text-copy-13-mono text-gray-700">
                        {formatUpdated(node.updatedAt)}
                      </td>
                    </tr>
                  );
                })}
                {endIdx < sorted.length ? (
                  <tr style={{ height: (sorted.length - endIdx) * ROW_HEIGHT }} aria-hidden>
                    <td colSpan={5} />
                  </tr>
                ) : null}
              </>
            )}
          </tbody>
        </table>
      </div>
      <p className="border-t border-gray-alpha-200 px-3 py-2 text-label-12 text-gray-700">
        {sorted.length} {sorted.length === 1 ? 'entry' : 'entries'} · list view
      </p>
    </section>
  );
}

function compare(a: WorldKbNodeData, b: WorldKbNodeData, key: SortKey): number {
  switch (key) {
    case 'name':
      return (a.name ?? '').localeCompare(b.name ?? '');
    case 'blockType':
      return a.entityKind.localeCompare(b.entityKind);
    case 'lifecycle':
      return LIFECYCLE_RANK[a.lifecycle] - LIFECYCLE_RANK[b.lifecycle];
    case 'anchors':
      return a.sourceAnchorCount - b.sourceAnchorCount;
    case 'updated': {
      const at = Date.parse(a.updatedAt ?? '');
      const bt = Date.parse(b.updatedAt ?? '');
      if (Number.isNaN(at) && Number.isNaN(bt)) return 0;
      if (Number.isNaN(at)) return 1;
      if (Number.isNaN(bt)) return -1;
      return at - bt;
    }
  }
}

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
