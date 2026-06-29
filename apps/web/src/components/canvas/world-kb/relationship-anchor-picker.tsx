/**
 * World KB relationship anchor picker (V1.74 A6 split).
 *
 * Multi-checkbox picker for source-anchor grounding, extracted from the
 * relationship inspector to keep each module under the 250-line cap.
 */
import type { WorldKbSourceAnchorProjection } from '@42ch/nexus-contracts';

interface RelationshipAnchorPickerProps {
  anchors: WorldKbSourceAnchorProjection[];
  selectedIds: string[];
  onChange: (selectedIds: string[]) => void;
}

export function RelationshipAnchorPicker({
  anchors,
  selectedIds,
  onChange,
}: RelationshipAnchorPickerProps) {
  if (anchors.length === 0) {
    return <span className="text-copy-13 text-gray-700">No source anchors available.</span>;
  }
  return (
    <div className="max-h-32 overflow-auto rounded-control border border-gray-alpha-400 bg-background-100 p-2">
      {anchors.map((a) => (
        <label key={a.source_anchor_id} className="flex items-center gap-2 py-1 text-copy-14">
          <input
            type="checkbox"
            className="h-4 w-4 rounded border-gray-alpha-400"
            checked={selectedIds.includes(a.source_anchor_id)}
            onChange={(e) => {
              const ids = new Set(selectedIds);
              if (e.target.checked) ids.add(a.source_anchor_id);
              else ids.delete(a.source_anchor_id);
              onChange([...ids]);
            }}
          />
          {a.reference}
        </label>
      ))}
    </div>
  );
}
