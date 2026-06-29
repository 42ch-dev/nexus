/**
 * World KB non-spatial alternate view (V1.74 A6/A8; V1.76 adds Suggested tab).
 *
 * Three-pane accessible equivalent to the canvas graph:
 *   1. Entities — sortable entity table (keyboard/screen-reader primary).
 *   2. Relationships — sortable relationship table with full write parity.
 *   3. Suggested — extraction-suggested relationships (needs_review) triage.
 *
 * Selection in either pane opens the matching inspector.
 */
import { useState } from 'react';

import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import { WorldKbEntityTable } from './world-kb-entity-table';
import { WorldKbRelationshipTable } from './world-kb-relationship-table';
import { SuggestedRelationshipsPane } from './suggested-relationships-pane';
import type { WorldKbNodeData } from './types';

type Tab = 'entities' | 'relationships' | 'suggested';

export interface WorldKbAltViewProps {
  nodes: WorldKbNodeData[];
  relationships: WorldKbRelationshipProjection[];
  entities: WorldKbEntityProjection[];
  selectedNodeId: string | null;
  selectedRelationshipId: string | null;
  onSelectNode: (node: WorldKbNodeData) => void;
  onSelectRelationship: (relationship: WorldKbRelationshipProjection) => void;
  onCreateRelationship: () => void;
  onDeleteRelationship?: (relationship: WorldKbRelationshipProjection) => void;
  /** V1.76: promote a suggested relationship (clear needs_review). */
  onPromoteSuggestion?: (rel: WorldKbRelationshipProjection) => void;
  /** V1.76: delete a suggested relationship. */
  onDeleteSuggestion?: (rel: WorldKbRelationshipProjection) => void;
  /** V1.76: bulk-promote all visible suggestions. */
  onPromoteAllSuggestions?: (rels: WorldKbRelationshipProjection[]) => void;
  /** V1.76: whether a promote/delete mutation is in flight. */
  suggestionPending?: boolean;
}

export function WorldKbAltView({
  nodes,
  relationships,
  entities,
  selectedNodeId,
  selectedRelationshipId,
  onSelectNode,
  onSelectRelationship,
  onCreateRelationship,
  onDeleteRelationship,
  onPromoteSuggestion,
  onDeleteSuggestion,
  onPromoteAllSuggestions,
  suggestionPending,
}: WorldKbAltViewProps) {
  const [activeTab, setActiveTab] = useState<Tab>('entities');

  // V1.76: split relationships into confirmed (default table) and suggested.
  const storedRels = relationships.filter((r) => r.projection_direction === 'stored');
  const confirmedRels = storedRels.filter((r) => !r.needs_review);
  const suggestedRels = storedRels.filter((r) => r.needs_review);

  return (
    <div className="flex flex-col gap-3">
      <div className="inline-flex rounded-card border border-gray-alpha-400 bg-background-200 p-1">
        <TabButton
          label="Entities"
          active={activeTab === 'entities'}
          onClick={() => setActiveTab('entities')}
          count={nodes.length}
        />
        <TabButton
          label="Relationships"
          active={activeTab === 'relationships'}
          onClick={() => setActiveTab('relationships')}
          count={confirmedRels.length}
        />
        <TabButton
          label="Suggested"
          active={activeTab === 'suggested'}
          onClick={() => setActiveTab('suggested')}
          count={suggestedRels.length}
        />
      </div>

      {activeTab === 'entities' ? (
        <WorldKbEntityTable nodes={nodes} selectedId={selectedNodeId} onSelect={onSelectNode} />
      ) : activeTab === 'relationships' ? (
        <WorldKbRelationshipTable
          relationships={confirmedRels}
          entities={entities}
          selectedId={selectedRelationshipId}
          onSelect={onSelectRelationship}
          onCreate={onCreateRelationship}
          onDelete={onDeleteRelationship}
        />
      ) : (
        <SuggestedRelationshipsPane
          suggestions={suggestedRels}
          entities={entities}
          onPromote={(rel) => onPromoteSuggestion?.(rel)}
          onDelete={(rel) => onDeleteSuggestion?.(rel)}
          onPromoteAll={(rels) => onPromoteAllSuggestions?.(rels)}
          pending={suggestionPending}
        />
      )}
    </div>
  );
}

function TabButton({
  label,
  active,
  onClick,
  count,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  count: number;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-selected={active}
      role="tab"
      className={[
        'flex-1 rounded-md px-3 py-1.5 text-label-13 transition-colors duration-state ease-standard',
        active
          ? 'bg-background-100 text-gray-1000 shadow-sm'
          : 'text-gray-700 hover:bg-background-100/50 hover:text-gray-1000',
      ].join(' ')}
    >
      {label}
      <span className="ml-1.5 rounded-full bg-gray-alpha-200 px-1.5 py-0.5 text-label-11 text-gray-700">
        {count}
      </span>
    </button>
  );
}
