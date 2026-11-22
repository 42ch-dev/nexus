/**
 * Relationship inspector field grid (V1.74 A10 split).
 *
 * Holds the symmetric toggle, confidence input, and source-anchor picker so
 * the relationship inspector stays under the 250-line cap.
 */
import type { WorldKbSourceAnchorProjection } from '@42ch/nexus-contracts';
import { Input } from '@/components/ui/input';

import { Field } from './inspector-field';
import { RelationshipAnchorPicker } from './relationship-anchor-picker';
import type { RelationshipForm, RelationshipFormErrors } from './relationship-inspector-logic';

export interface RelationshipFieldGridProps {
  form: RelationshipForm;
  errors: RelationshipFormErrors;
  anchors: WorldKbSourceAnchorProjection[];
  onChange: (patch: Partial<RelationshipForm>) => void;
}

export function RelationshipFieldGrid({
  form,
  errors,
  anchors,
  onChange,
}: RelationshipFieldGridProps) {
  return (
    <>
      <label className="flex items-center gap-2 text-copy-14 text-gray-1000">
        <input
          type="checkbox"
          className="h-4 w-4 rounded border-gray-alpha-400"
          checked={form.symmetric}
          onChange={(e) => onChange({ symmetric: e.target.checked })}
        />
        Symmetric (show reverse edge)
      </label>

      <Field
        label={`Confidence: ${form.confidence.toFixed(2)}`}
        htmlFor="rel-confidence"
        error={errors.confidence}
      >
        <Input
          id="rel-confidence"
          type="number"
          min={0}
          max={1}
          step={0.01}
          value={form.confidence}
          onChange={(e) => onChange({ confidence: Number(e.target.value) })}
          invalid={Boolean(errors.confidence)}
        />
      </Field>

      <Field label="Grounding anchors" htmlFor="rel-anchors">
        <RelationshipAnchorPicker
          id="rel-anchors"
          anchors={anchors}
          selectedIds={form.sourceAnchorIds}
          onChange={(ids) => onChange({ sourceAnchorIds: ids })}
        />
      </Field>
    </>
  );
}
