/**
 * World KB relationship inspector — create/edit/remove typed relationships
 * via `world_kb.patch_relationship` (V1.74 A6).
 *
 * Supports core taxonomy + custom_label escape hatch, symmetric toggle,
 * confidence (display-only), and optional source-anchor grounding. Exposes
 * 409 conflicts to the parent canvas for the KB-flavored conflict modal and
 * renders 422 validation inline.
 */
import { useMemo, useState } from 'react';
import { Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select } from '@/components/ui/select';
import {
  usePatchWorldKbRelationship,
  isWorldKbConflictError,
  isWorldKbValidationError,
} from '@/lib/canvas/use-world-kb-data';
import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

import { RELATIONSHIP_KIND_LABELS } from './relationship-projection';
import { Field } from './inspector-field';
import { RelationshipFieldGrid } from './relationship-inspector-field-grid';
import {
  CORE_KINDS,
  type RelationshipForm,
  type RelationshipFormErrors,
  buildRelationshipPatchRequest,
  buildRelationshipRemoveRequest,
  initialRelationshipForm,
  validateRelationshipForm,
} from './relationship-inspector-logic';

export type { RelationshipForm } from './relationship-inspector-logic';
export { entityName } from './relationship-inspector-logic';

export interface RelationshipInspectorProps {
  worldId: string;
  /** Existing relationship → edit mode. Omit → create mode. */
  relationship?: WorldKbRelationshipProjection;
  /** Pre-filled source/target for quick-create flows (canvas drag, context menu). */
  initialSourceEntityId?: string;
  initialTargetEntityId?: string;
  /** Confirmed entities for pickers + name resolution. */
  entities: WorldKbEntityProjection[];
  /** Available source anchors for grounding picker. */
  anchors: WorldKbSourceAnchorProjection[];
  /** Called on 409 so the canvas can render the conflict modal. */
  onConflict?: (payload: { currentVersion: number; relationshipId: string; draft: RelationshipForm }) => void;
  /** Called after a successful add/update/remove. */
  onSaved?: () => void;
}

export function RelationshipInspector({
  worldId,
  relationship,
  initialSourceEntityId,
  initialTargetEntityId,
  entities,
  anchors,
  onConflict,
  onSaved,
}: RelationshipInspectorProps) {
  const patchRelationship = usePatchWorldKbRelationship(worldId);
  const isEdit = Boolean(relationship);
  const [form, setForm] = useState<RelationshipForm>(() =>
    initialRelationshipForm(relationship, initialSourceEntityId, initialTargetEntityId),
  );
  const [errors, setErrors] = useState<RelationshipFormErrors>({});
  const [submitError, setSubmitError] = useState<string | null>(null);

  const selectableEntities = useMemo(
    () => entities.filter((e) => e.status?.toLowerCase() !== 'rejected'),
    [entities],
  );
  const targetEntities = useMemo(
    () => selectableEntities.filter((e) => e.key_block_id !== form.sourceEntityId),
    [selectableEntities, form.sourceEntityId],
  );

  function handleSubmit() {
    const validation = validateRelationshipForm(form);
    setErrors(validation);
    if (Object.keys(validation).length > 0) return;

    patchRelationship.mutate(buildRelationshipPatchRequest(form, relationship), {
      onSuccess: () => {
        setErrors({});
        setSubmitError(null);
        onSaved?.();
      },
      onError: (error) => {
        if (isWorldKbValidationError(error)) {
          setSubmitError(error.message);
        } else if (isWorldKbConflictError(error) && relationship) {
          onConflict?.({
            currentVersion: error.details.current_version,
            relationshipId: relationship.relationship_id,
            draft: form,
          });
        }
      },
    });
  }

  function handleRemove() {
    if (!relationship) return;
    patchRelationship.mutate(buildRelationshipRemoveRequest(relationship), {
      onSuccess: () => onSaved?.(),
      onError: (error) => {
        // A 409 on delete = the relationship changed concurrently (or was
        // already removed). The hook's global onError refetches the graph to
        // canonical state; here we dismiss the inspector since the stale row is
        // no longer editable.
        if (isWorldKbConflictError(error)) {
          onSaved?.();
        }
      },
    });
  }

  const isCustom = form.relationType === 'custom';

  return (
    <form
      className="flex flex-col gap-4 rounded-card border border-gray-alpha-400 bg-canvas-worldkb-relationship-inspector-fill p-4 shadow-card"
      onSubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
    >
      <div className="flex items-center justify-between">
        <h3 className="text-heading-16 font-heading text-gray-1000">
          {isEdit ? 'Edit Relationship' : 'New Relationship'}
        </h3>
        {isEdit && (
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={handleRemove}
            disabled={patchRelationship.isPending}
            aria-label="Remove relationship"
          >
            <Trash2 className="h-4 w-4 text-red-700" aria-hidden />
          </Button>
        )}
      </div>

      <div className="grid gap-4">
        <Field label="Source entity" htmlFor="rel-source" error={errors.sourceEntityId}>
          <Select
            id="rel-source"
            value={form.sourceEntityId}
            onChange={(e) => setForm((f) => ({ ...f, sourceEntityId: e.target.value }))}
            disabled={isEdit}
            invalid={Boolean(errors.sourceEntityId)}
          >
            <option value="">Select source…</option>
            {selectableEntities.map((e) => (
              <option key={e.key_block_id} value={e.key_block_id}>
                {e.canonical_name}
              </option>
            ))}
          </Select>
        </Field>

        <Field label="Target entity" htmlFor="rel-target" error={errors.targetEntityId}>
          <Select
            id="rel-target"
            value={form.targetEntityId}
            onChange={(e) => setForm((f) => ({ ...f, targetEntityId: e.target.value }))}
            disabled={isEdit}
            invalid={Boolean(errors.targetEntityId)}
          >
            <option value="">Select target…</option>
            {targetEntities.map((e) => (
              <option key={e.key_block_id} value={e.key_block_id}>
                {e.canonical_name}
              </option>
            ))}
          </Select>
        </Field>

        <Field label="Relation type" htmlFor="rel-type" error={errors.relationType}>
          <Select
            id="rel-type"
            value={form.relationType}
            onChange={(e) =>
              setForm((f) => ({
                ...f,
                relationType: e.target.value as RelationshipForm['relationType'],
              }))
            }
            invalid={Boolean(errors.relationType)}
          >
            {CORE_KINDS.map((k) => (
              <option key={k} value={k}>{RELATIONSHIP_KIND_LABELS[k]}</option>
            ))}
          </Select>
        </Field>

        {isCustom && (
          <Field label="Custom label" htmlFor="rel-custom" error={errors.customLabel}>
            <Input
              id="rel-custom"
              value={form.customLabel}
              onChange={(e) => setForm((f) => ({ ...f, customLabel: e.target.value }))}
              placeholder="e.g., Childhood Friend"
              invalid={Boolean(errors.customLabel)}
            />
          </Field>
        )}

        <RelationshipFieldGrid
          form={form}
          errors={errors}
          anchors={anchors}
          onChange={(patch) => setForm((f) => ({ ...f, ...patch }))}
        />
      </div>

      {submitError && <p className="text-copy-13 text-red-700">{submitError}</p>}

      <div className="flex justify-end gap-2">
        <Button type="submit" variant="primary" size="small" disabled={patchRelationship.isPending}>
          {isEdit ? 'Save changes' : 'Add relationship'}
        </Button>
      </div>
    </form>
  );
}
