/**
 * World KB non-spatial relationship table (V1.74 A6).
 *
 * Accessible equivalent for relationship editing: sortable columns, row-level
 * edit/delete actions, and a "New Relationship" button that opens the inspector
 * in create mode.
 */
import { useMemo, useState } from 'react';
import { Pencil, Plus, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import { relationshipEdgeLabel } from './relationship-projection';
import {
  SortHeader,
  type RelationshipTableSortDir,
  type RelationshipTableSortKey,
} from './world-kb-relationship-table-sort';
import { formatUpdated } from './world-kb-canvas-utils';

const ROW_HEIGHT = 44;

export interface WorldKbRelationshipTableProps {
  relationships: WorldKbRelationshipProjection[];
  entities: WorldKbEntityProjection[];
  selectedId: string | null;
  onSelect: (relationship: WorldKbRelationshipProjection) => void;
  onCreate: () => void;
  onDelete?: (relationship: WorldKbRelationshipProjection) => void;
}

export function WorldKbRelationshipTable({
  relationships,
  entities,
  selectedId,
  onSelect,
  onCreate,
  onDelete,
}: WorldKbRelationshipTableProps) {
  const [sortKey, setRelationshipTableSortKey] = useState<RelationshipTableSortKey>('type');
  const [sortDir, setRelationshipTableSortDir] = useState<RelationshipTableSortDir>('asc');

  const entityName = (id: string) => entities.find((e) => e.key_block_id === id)?.canonical_name ?? id;

  const sorted = useMemo(() => {
    const stored = relationships.filter((r) => r.projection_direction === 'stored');
    stored.sort((a, b) => {
      let cmp = 0;
      switch (sortKey) {
        case 'source':
          cmp = entityName(a.source_entity_id).localeCompare(entityName(b.source_entity_id));
          break;
        case 'target':
          cmp = entityName(a.target_entity_id).localeCompare(entityName(b.target_entity_id));
          break;
        case 'type':
          cmp = relationshipEdgeLabel(a).localeCompare(relationshipEdgeLabel(b));
          break;
        case 'symmetric':
          cmp = Number(a.symmetric) - Number(b.symmetric);
          break;
        case 'confidence':
          cmp = (a.confidence ?? 1) - (b.confidence ?? 1);
          break;
        case 'anchors':
          cmp = (a.source_anchor_ids?.length ?? 0) - (b.source_anchor_ids?.length ?? 0);
          break;
        case 'updated': {
          const at = Date.parse(a.updated_at ?? '');
          const bt = Date.parse(b.updated_at ?? '');
          if (Number.isNaN(at) && Number.isNaN(bt)) cmp = 0;
          else if (Number.isNaN(at)) cmp = 1;
          else if (Number.isNaN(bt)) cmp = -1;
          else cmp = at - bt;
          break;
        }
      }
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return stored;
  }, [relationships, sortKey, sortDir, entities]);

  function toggleSort(key: RelationshipTableSortKey) {
    if (key === sortKey) {
      setRelationshipTableSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setRelationshipTableSortKey(key);
      setRelationshipTableSortDir('asc');
    }
  }

  function handleDelete(rel: WorldKbRelationshipProjection) {
    if (!onDelete) return;
    const source = entityName(rel.source_entity_id);
    const target = entityName(rel.target_entity_id);
    if (
      window.confirm(
        `Delete the relationship "${relationshipEdgeLabel(rel)}" from ${source} to ${target}?`,
      )
    ) {
      onDelete(rel);
    }
  }

  const columnClass = 'px-3 py-2 text-left text-label-12 text-gray-700';

  return (
    <section
      aria-label="World KB relationships (sortable list)"
      className="rounded-card border border-gray-alpha-400 bg-background-100 shadow-card"
    >
      <div className="flex items-center justify-between border-b border-gray-alpha-200 px-3 py-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Relationships</h3>
        <Button type="button" variant="secondary" size="small" onClick={onCreate}>
          <Plus className="h-4 w-4" aria-hidden /> New Relationship
        </Button>
      </div>
      <div className="overflow-auto" style={{ maxHeight: 'calc(100vh - 420px)' }}>
        <table className="w-full table-fixed text-copy-14">
          <caption className="sr-only">
            World KB relationships sorted by {sortKey} ({sortDir}). Press Enter on a row to edit it.
          </caption>
          <thead className="sticky top-0 bg-background-200">
            <tr>
              <SortHeader label="Source" sortKey="source" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <SortHeader label="Target" sortKey="target" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <SortHeader label="Type" sortKey="type" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <SortHeader label="Symmetric" sortKey="symmetric" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <SortHeader label="Confidence" sortKey="confidence" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <SortHeader label="Anchors" sortKey="anchors" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <SortHeader label="Updated" sortKey="updated" current={sortKey} dir={sortDir} onToggle={toggleSort} />
              <th className={columnClass}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {sorted.length === 0 ? (
              <tr>
                <td colSpan={8} className="px-3 py-6 text-center text-copy-13 text-gray-700">
                  No relationships yet.
                </td>
              </tr>
            ) : (
              sorted.map((rel) => {
                const selected = rel.relationship_id === selectedId;
                const anchorCount = rel.source_anchor_ids?.length ?? 0;
                return (
                  <tr
                    key={rel.relationship_id}
                    tabIndex={0}
                    onClick={() => onSelect(rel)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        onSelect(rel);
                      }
                    }}
                    className={[
                      'cursor-pointer border-b border-gray-alpha-200 outline-none transition-colors duration-state ease-standard focus-visible:bg-canvas-worldkb-nonspatial-row-highlight',
                      selected ? 'bg-canvas-worldkb-nonspatial-row-highlight' : 'hover:bg-canvas-worldkb-nonspatial-row-highlight/60',
                    ].join(' ')}
                    style={{ height: ROW_HEIGHT }}
                    aria-selected={selected}
                  >
                    <td className="truncate px-3 py-2 text-gray-1000">{entityName(rel.source_entity_id)}</td>
                    <td className="truncate px-3 py-2 text-gray-1000">{entityName(rel.target_entity_id)}</td>
                    <td className="px-3 py-2 text-gray-900">{relationshipEdgeLabel(rel)}</td>
                    <td className="px-3 py-2 text-gray-900">{rel.symmetric ? 'Yes' : 'No'}</td>
                    <td className="px-3 py-2 tabular-nums text-gray-900">{(rel.confidence ?? 1).toFixed(2)}</td>
                    <td className="px-3 py-2 tabular-nums text-gray-900">{anchorCount}</td>
                    <td className="px-3 py-2 text-copy-13-mono text-gray-700">{formatUpdated(rel.updated_at)}</td>
                    <td className="px-3 py-2">
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            onSelect(rel);
                          }}
                          className="rounded p-1 text-gray-700 hover:bg-gray-alpha-200 hover:text-gray-1000 focus-visible:ring-2 focus-visible:ring-canvas-strategy-accent"
                          aria-label={`Edit relationship ${relationshipEdgeLabel(rel)}`}
                        >
                          <Pencil className="h-4 w-4" aria-hidden />
                        </button>
                        {onDelete && (
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDelete(rel);
                            }}
                            className="rounded p-1 text-red-700 hover:bg-red-100 focus-visible:ring-2 focus-visible:ring-red-700"
                            aria-label={`Delete relationship ${relationshipEdgeLabel(rel)}`}
                          >
                            <Trash2 className="h-4 w-4" aria-hidden />
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
      <p className="border-t border-gray-alpha-200 px-3 py-2 text-label-12 text-gray-700">
        {sorted.length} {sorted.length === 1 ? 'relationship' : 'relationships'}
      </p>
    </section>
  );
}
