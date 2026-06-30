/**
 * World KB Suggested relationships pane (V1.76 γ).
 *
 * The author's primary triage surface for extraction-suggested relationships
 * (`needs_review = 1`, `source = 'extraction'`). A sortable table with
 * per-row Promote / Delete actions + a bulk "Promote all". Default sort is
 * confidence descending so the author triages the highest-confidence
 * suggestions first (compass §Phase 2b).
 *
 * Promote clears `needs_review` via the existing patch-relationship update
 * route; Delete removes the row. Both reuse `usePatchWorldKbRelationship`.
 */
import { useMemo, useState } from 'react';
import { ArrowUp, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import { relationshipEdgeLabel } from './relationship-projection';
import {
  CONFIDENCE_BAND_COLOR_VAR,
  CONFIDENCE_BAND_LABEL,
  confidenceBand,
  formatConfidence,
} from './relationship-confidence';

export interface SuggestedRelationshipsPaneProps {
  /** Relationships with `needs_review = true` (extraction suggestions). */
  suggestions: WorldKbRelationshipProjection[];
  entities: WorldKbEntityProjection[];
  /** Promote a single suggestion (clears needs_review). */
  onPromote: (rel: WorldKbRelationshipProjection) => void;
  /** Delete a single suggestion. */
  onDelete: (rel: WorldKbRelationshipProjection) => void;
  /** Promote all currently-visible suggestions (bulk). */
  onPromoteAll: (rels: WorldKbRelationshipProjection[]) => void;
  /** Whether a promote/delete mutation is in flight (disables actions). */
  pending?: boolean;
}

type SortKey = 'confidence' | 'source' | 'target' | 'type';
type SortDir = 'asc' | 'desc';

/** A uniform 8px colored dot badge for a confidence band. */
function ConfidenceBadge({ confidence }: { confidence: number | undefined | null }) {
  const band = confidenceBand(confidence);
  return (
    <span
      className="inline-block h-2 w-2 shrink-0 rounded-full"
      style={{ backgroundColor: CONFIDENCE_BAND_COLOR_VAR[band] }}
      role="img"
      aria-label={`${CONFIDENCE_BAND_LABEL[band]} confidence`}
    />
  );
}

export function SuggestedRelationshipsPane({
  suggestions,
  entities,
  onPromote,
  onDelete,
  onPromoteAll,
  pending = false,
}: SuggestedRelationshipsPaneProps) {
  const [sortKey, setSortKey] = useState<SortKey>('confidence');
  const [sortDir, setSortDir] = useState<SortDir>('desc');

  const entityName = (id: string) =>
    entities.find((e) => e.key_block_id === id)?.canonical_name ?? id;

  const sorted = useMemo(() => {
    // Only stored-direction rows (avoid double-counting symmetric reverses).
    const stored = suggestions.filter((r) => r.projection_direction === 'stored');
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
        case 'confidence':
        default:
          // Suggestions without confidence sort last (treat as -1).
          cmp = (a.confidence ?? -1) - (b.confidence ?? -1);
          break;
      }
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return stored;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [suggestions, sortKey, sortDir, entities]);

  function toggleSort(key: SortKey) {
    if (key === sortKey) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      // Confidence defaults to desc (triage highest first); others asc.
      setSortDir(key === 'confidence' ? 'desc' : 'asc');
    }
  }

  function handleDelete(rel: WorldKbRelationshipProjection) {
    const source = entityName(rel.source_entity_id);
    const target = entityName(rel.target_entity_id);
    if (
      window.confirm(
        `Delete the suggested relationship "${relationshipEdgeLabel(rel)}" from ${source} to ${target}?`,
      )
    ) {
      onDelete(rel);
    }
  }

  function handlePromoteAll() {
    if (sorted.length === 0) return;
    const message =
      sorted.length > 1
        ? `Promote all ${sorted.length} suggested relationships? This confirms them all at once.`
        : `Promote this suggested relationship?`;
    if (window.confirm(message)) {
      onPromoteAll(sorted);
    }
  }

  const headerClass =
    'px-3 py-2 text-left text-label-12 text-gray-700 cursor-pointer select-none hover:text-gray-1000';

  return (
    <section
      aria-label="Suggested relationships (extraction)"
      className="rounded-card border border-gray-alpha-400 bg-background-100 shadow-card"
    >
      <div className="flex items-center justify-between border-b border-gray-alpha-200 px-3 py-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">
          Suggested <span className="text-gray-700">({sorted.length})</span>
        </h3>
        <Button
          type="button"
          variant="secondary"
          size="small"
          onClick={handlePromoteAll}
          disabled={pending || sorted.length === 0}
        >
          <ArrowUp className="h-4 w-4" aria-hidden /> Promote all
        </Button>
      </div>
      <p className="border-b border-gray-alpha-200 px-3 py-1.5 text-label-12 text-gray-700">
        Extraction-suggested relationships. Promote to confirm, or delete to dismiss.
      </p>
      <div className="overflow-auto" style={{ maxHeight: 'calc(100vh - 460px)' }}>
        <table className="w-full table-fixed text-copy-14">
          <caption className="sr-only">
            Extraction-suggested relationships sorted by {sortKey} ({sortDir}). Default is confidence
            high to low.
          </caption>
          <thead className="sticky top-0 bg-background-200">
            <tr>
              <th className={headerClass} onClick={() => toggleSort('source')}>
                Source {sortKey === 'source' && (sortDir === 'asc' ? '▲' : '▼')}
              </th>
              <th className={headerClass} onClick={() => toggleSort('target')}>
                Target {sortKey === 'target' && (sortDir === 'asc' ? '▲' : '▼')}
              </th>
              <th className={headerClass} onClick={() => toggleSort('type')}>
                Type {sortKey === 'type' && (sortDir === 'asc' ? '▲' : '▼')}
              </th>
              <th className={headerClass} onClick={() => toggleSort('confidence')}>
                Confidence {sortKey === 'confidence' && (sortDir === 'asc' ? '▲' : '▼')}
              </th>
              <th className="px-3 py-2 text-left text-label-12 text-gray-700">Actions</th>
            </tr>
          </thead>
          <tbody>
            {sorted.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-center text-copy-13 text-gray-700">
                  No suggested relationships. Run extraction on a chapter to populate suggestions.
                </td>
              </tr>
            ) : (
              sorted.map((rel) => (
                <tr
                  key={rel.relationship_id}
                  className="border-b border-gray-alpha-200 hover:bg-canvas-worldkb-nonspatial-row-highlight/60"
                >
                  <td className="truncate px-3 py-2 text-gray-1000">
                    {entityName(rel.source_entity_id)}
                  </td>
                  <td className="truncate px-3 py-2 text-gray-1000">
                    {entityName(rel.target_entity_id)}
                  </td>
                  <td className="px-3 py-2 text-gray-900">{relationshipEdgeLabel(rel)}</td>
                  <td className="px-3 py-2 tabular-nums text-gray-900">
                    <span className="flex items-center gap-1.5">
                      <ConfidenceBadge confidence={rel.confidence} />
                      <span>{formatConfidence(rel.confidence)}</span>
                    </span>
                  </td>
                  <td className="px-3 py-2">
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => onPromote(rel)}
                        disabled={pending}
                        className="rounded p-1 text-green-700 hover:bg-green-100 focus-visible:ring-2 focus-visible:ring-green-700 disabled:opacity-50"
                        aria-label={`Promote ${relationshipEdgeLabel(rel)}`}
                      >
                        <ArrowUp className="h-4 w-4" aria-hidden />
                      </button>
                      <button
                        type="button"
                        onClick={() => handleDelete(rel)}
                        disabled={pending}
                        className="rounded p-1 text-red-700 hover:bg-red-100 focus-visible:ring-2 focus-visible:ring-red-700 disabled:opacity-50"
                        aria-label={`Delete ${relationshipEdgeLabel(rel)}`}
                      >
                        <Trash2 className="h-4 w-4" aria-hidden />
                      </button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
