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
import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight } from 'lucide-react';

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

const FIELD_LABEL_KEYS: Record<WorldKbEntityField, string> = {
  title: 'worldKb.entityInspector.field.title',
  body: 'worldKb.entityInspector.field.body',
  aliases: 'worldKb.entityInspector.field.aliases',
  block_type: 'worldKb.entityInspector.field.blockType',
};

/** Handbook order for the nine-field mental table (product locks §Mental field vocabulary). */
const MENTAL_FIELD_ORDER = [
  'identity',
  'beliefs',
  'attention',
  'goals',
  'intentions',
  'emotions',
  'dispositions',
  'norms',
  'constraints',
] as const;

const MENTAL_FIELD_LABEL_KEYS: Record<(typeof MENTAL_FIELD_ORDER)[number], string> = {
  identity: 'worldKb.entityInspector.mentalState.field.identity',
  beliefs: 'worldKb.entityInspector.mentalState.field.beliefs',
  attention: 'worldKb.entityInspector.mentalState.field.attention',
  goals: 'worldKb.entityInspector.mentalState.field.goals',
  intentions: 'worldKb.entityInspector.mentalState.field.intentions',
  emotions: 'worldKb.entityInspector.mentalState.field.emotions',
  dispositions: 'worldKb.entityInspector.mentalState.field.dispositions',
  norms: 'worldKb.entityInspector.mentalState.field.norms',
  constraints: 'worldKb.entityInspector.mentalState.field.constraints',
};

/**
 * Read-only "Mental State" section (V1.164 P3 Task 3, AC-V1164-12/15 + PD-16).
 *
 * Collapsible via the header toggle. Renders every populated nine-field key
 * (bold label + JSON value row, no input controls). Returns null when
 * `modules.mental` is absent / null / has no populated keys — no empty panel,
 * no "N/A" placeholders.
 */
function MentalStateSection({ mental }: { mental: Record<string, unknown> }) {
  const { t } = useTranslation('canvas');
  const [open, setOpen] = useState(true);
  const regionId = useId();
  const fields = MENTAL_FIELD_ORDER.filter((key) => mental[key] !== undefined);
  if (fields.length === 0) {
    return null;
  }
  const title = t('worldKb.entityInspector.mentalState.title');
  return (
    <section
      className="mt-3 border-t border-gray-alpha-300 pt-2"
      data-testid="mental-state-section"
      aria-label={title}
    >
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-controls={regionId}
        className="flex w-full items-center gap-1.5 text-left text-label-14 font-semibold text-gray-900"
      >
        {open ? (
          <ChevronDown className="h-4 w-4 text-gray-700" aria-hidden />
        ) : (
          <ChevronRight className="h-4 w-4 text-gray-700" aria-hidden />
        )}
        {title}
      </button>
      {open ? (
        <dl id={regionId} className="mt-1.5 flex flex-col gap-2">
          {fields.map((key) => (
            <div key={key} className="flex flex-col gap-0.5">
              <dt className="text-label-14 font-semibold text-gray-900">
                {t(MENTAL_FIELD_LABEL_KEYS[key])}
              </dt>
              <dd className="text-copy-13 font-mono text-gray-1000 whitespace-pre-wrap break-words">
                {JSON.stringify(mental[key], null, 2)}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
    </section>
  );
}

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
  const { t } = useTranslation('canvas');
  const [form, setForm] = useState<EntityEditForm>(() => formFromEntity(entity));
  const [validationErrors, setValidationErrors] = useState<string[]>([]);

  // PD-16: null / undefined / non-object modules.mental skips the whole
  // section — no empty panel, no placeholder rows.
  const mental =
    entity.modules?.mental !== undefined &&
    entity.modules.mental !== null &&
    typeof entity.modules.mental === 'object'
      ? (entity.modules.mental as Record<string, unknown>)
      : undefined;

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
        setValidationErrors([t('worldKb.entityInspector.bodyJsonError')]);
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
            setValidationErrors(details?.validation_summary?.errors ?? [t('worldKb.entityInspector.validationFailed')]);
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
          // Any other status (500/403/dropped network) is surfaced as a toast
          // by the hook's global onError (see usePatchWorldKbEntity) — never
          // silently swallowed.
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
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('worldKb.entityInspector.title')}</h3>
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          v{node.version}
        </span>
      </div>
      <p className="text-copy-13 text-gray-700">{t('worldKb.entityInspector.description')}</p>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-title">{t('worldKb.entityInspector.field.title')}</Label>
        <Input
          id="wkbe-title"
          value={form.title}
          onChange={(e) => update('title', e.target.value)}
        />
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-blocktype">{t('worldKb.entityInspector.field.blockType')}</Label>
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
        <Label htmlFor="wkbe-aliases">{t('worldKb.entityInspector.field.aliases')}</Label>
        <Input
          id="wkbe-aliases"
          value={form.aliasesText}
          onChange={(e) => update('aliasesText', e.target.value)}
          placeholder={t('worldKb.entityInspector.aliasesPlaceholder')}
        />
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="wkbe-body">{t('worldKb.entityInspector.field.body')}</Label>
        <Textarea
          id="wkbe-body"
          rows={6}
          className="font-mono text-copy-13-mono"
          value={form.bodyText}
          onChange={(e) => update('bodyText', e.target.value)}
          placeholder={t('worldKb.entityInspector.bodyPlaceholder')}
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
            ? t('worldKb.entityInspector.noChanges')
            : t('worldKb.entityInspector.editing', {
                fields: dirty.map((d) => t(FIELD_LABEL_KEYS[d])).join(', '),
              })}
        </span>
        <Button type="submit" disabled={dirty.length === 0 || patch.isPending}>
          {patch.isPending ? t('worldKb.entityInspector.saving') : t('worldKb.entityInspector.save')}
        </Button>
      </div>

      {mental ? (
        // Keyed by entity id (S-3, QC fix wave): React otherwise keeps the
        // component instance (and its `open` collapse state) when switching
        // between two entities that both have `modules.mental` — the section
        // must reset to expanded for the newly selected entity.
        <MentalStateSection key={entity.key_block_id} mental={mental} />
      ) : null}
    </form>
  );
}
