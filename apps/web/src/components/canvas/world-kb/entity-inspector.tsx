/**
 * World KB entity inspector — edits a confirmed/rejected/merged KeyBlock entity
 * via `world_kb.patch_entity` (V1.73 P0 A6).
 *
 * Edits title / body / aliases / block_type with inline validation (422) and
 * surfaces per-row OCC conflicts (409) to the parent canvas, which renders the
 * KB-flavored conflict modal. Body is shown as a JSON summary field because the
 * V1.73 entity body is a free-form `Record<string, unknown>` projection; a rich
 * body editor is V1.74.
 */
import { useEffect, useState } from 'react';

import { Textarea } from '@/components/ui/textarea';
import { Select } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { usePatchWorldKbEntity, isWorldKbValidationError } from '@/lib/canvas/use-world-kb-data';
import { BLOCK_TYPE_LABELS, type WorldKbNodeData } from './types';
import type { BlockType, WorldKbEntityPatch, WorldKbEntityProjection } from '@42ch/nexus-contracts';

/** Editable form derived from a selected entity node. */
export interface EntityEditForm {
  title: string;
  bodyText: string;
  aliasesText: string; // comma-separated in the UI; converted to string[] on submit
  block_type: BlockType;
}

/** Build the form from a selected node's backing projection. */
export function formFromEntity(entity: WorldKbEntityProjection): EntityEditForm {
  return {
    title: entity.canonical_name,
    bodyText: entity.body ? JSON.stringify(entity.body, null, 2) : '',
    aliasesText: (entity.aliases ?? []).join(', '),
    block_type: entity.block_type,
  };
}

/** Which form fields differ from the canonical entity (drives patch + overlap). */
function dirtyFields(form: EntityEditForm, entity: WorldKbEntityProjection): WorldKbEntityField[] {
  const fields: WorldKbEntityField[] = [];
  if (form.title !== entity.canonical_name) fields.push('title');
  if (form.aliasesText !== (entity.aliases ?? []).join(', ')) fields.push('aliases');
  if (form.block_type !== entity.block_type) fields.push('block_type');
  const canonBody = entity.body ? JSON.stringify(entity.body, null, 2) : '';
  if (form.bodyText !== canonBody) fields.push('body');
  return fields;
}

type WorldKbEntityField = 'title' | 'body' | 'aliases' | 'block_type';

const FIELD_LABELS: Record<WorldKbEntityField, string> = {
  title: 'Title',
  body: 'Body',
  aliases: 'Aliases',
  block_type: 'Block Type',
};

export interface EntityInspectorProps {
  worldId: string;
  /** The selected node (for display + version). */
  node: WorldKbNodeData;
  /** The canonical projection backing the node (for form seed + diff). */
  entity: WorldKbEntityProjection;
  /**
   * Called when a 409 conflict is detected. The canvas renders the
   * `patch_entity` conflict modal from this payload.
   */
  onConflict: (payload: {
    currentVersion: number;
    entityId: string;
    conflictingPath: string;
    draft: EntityEditForm;
    dirtyFields: WorldKbEntityField[];
  }) => void;
  /** Optional external reseed (e.g. after "Use current" in the conflict modal). */
  reseedSignal?: number;
}

export function EntityInspector({
  worldId,
  node,
  entity,
  onConflict,
  reseedSignal,
}: EntityInspectorProps) {
  const patch = usePatchWorldKbEntity(worldId);
  const [form, setForm] = useState<EntityEditForm>(() => formFromEntity(entity));
  const [validationErrors, setValidationErrors] = useState<string[]>([]);

  // Reseed when the selection (or an external reseed signal) changes.
  useEffect(() => {
    setForm(formFromEntity(entity));
    setValidationErrors([]);
  }, [entity.key_block_id, reseedSignal]); // eslint-disable-line react-hooks/exhaustive-deps

  function update<K extends keyof EntityEditForm>(field: K, value: EntityEditForm[K]) {
    setForm((prev) => ({ ...prev, [field]: value }));
  }

  const dirty = dirtyFields(form, entity);

  function handleSubmit() {
    if (dirty.length === 0) return;
    setValidationErrors([]);

    const patchBody: WorldKbEntityPatch = {};
    if (dirty.includes('title')) patchBody.title = form.title.trim();
    if (dirty.includes('block_type')) patchBody.block_type = form.block_type;
    if (dirty.includes('aliases')) {
      patchBody.aliases = form.aliasesText
        .split(',')
        .map((a) => a.trim())
        .filter(Boolean);
    }
    if (dirty.includes('body')) {
      try {
        patchBody.body = form.bodyText.trim() ? JSON.parse(form.bodyText) : undefined;
      } catch {
        setValidationErrors(['Body must be valid JSON (or empty).']);
        return;
      }
    }

    patch.mutate(
      {
        entity_id: entity.key_block_id,
        expected_version: node.version,
        patch: patchBody,
      },
      {
        onError: (error) => {
          if (isWorldKbValidationError(error)) {
            const details = error.details as { validation_summary?: { errors?: string[] } } | undefined;
            setValidationErrors(details?.validation_summary?.errors ?? ['Validation failed.']);
            return;
          }
          // Conflict (409) — hand off to the canvas to render the modal.
          const details = error as unknown as {
            status: number;
            details?: { current_version?: number; conflicting_path?: string; entity_id?: string };
          };
          if (details.status === 409) {
            onConflict({
              currentVersion: details.details?.current_version ?? node.version,
              entityId: details.details?.entity_id ?? entity.key_block_id,
              conflictingPath: details.details?.conflicting_path ?? dirty.join(','),
              draft: form,
              dirtyFields: dirty,
            });
          }
        },
      },
    );
  }

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Entity</h3>
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          v{node.version}
        </span>
      </div>
      <p className="text-copy-13 text-gray-700">
        Edit this world entry. Conflicts are detected per-row — Nexus blocks stale writes.
      </p>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-title">Title</Label>
        <Input
          id="wkbe-title"
          value={form.title}
          onChange={(e) => update('title', e.target.value)}
        />
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-blocktype">Block Type</Label>
        <Select
          id="wkbe-blocktype"
          value={form.block_type}
          onChange={(e) => update('block_type', e.target.value as BlockType)}
        >
          {(Object.keys(BLOCK_TYPE_LABELS) as BlockType[]).map((bt) => (
            <option key={bt} value={bt}>
              {BLOCK_TYPE_LABELS[bt]}
            </option>
          ))}
        </Select>
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-aliases">Aliases (comma-separated)</Label>
        <Input
          id="wkbe-aliases"
          value={form.aliasesText}
          onChange={(e) => update('aliasesText', e.target.value)}
          placeholder="e.g. Aria the Tempest, Stormwind"
        />
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-body">Body (JSON)</Label>
        <Textarea
          id="wkbe-body"
          rows={6}
          className="font-mono text-copy-13-mono"
          value={form.bodyText}
          onChange={(e) => update('bodyText', e.target.value)}
          placeholder="{}"
          spellCheck={false}
        />
      </div>

      {validationErrors.length > 0 ? (
        <ul
          className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
          aria-live="polite"
        >
          {validationErrors.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
      ) : null}

      <div className="flex items-center justify-between gap-2">
        <span className="text-label-12 text-gray-700">
          {dirty.length === 0
            ? 'No changes.'
            : `Editing: ${dirty.map((d) => FIELD_LABELS[d]).join(', ')}`}
        </span>
        <Button type="submit" disabled={dirty.length === 0 || patch.isPending}>
          {patch.isPending ? 'Saving…' : 'Save entity'}
        </Button>
      </div>
    </form>
  );
}
