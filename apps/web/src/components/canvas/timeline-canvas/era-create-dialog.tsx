/**
 * Era create dialog — V1.159 P1 Task 3 (DF-V1123-ERA-TAXONOMY).
 *
 * ⚠️ DEFERRED (F-001 / R-V1159P1-001) — do not activate until the World KB
 * entity-creation backend gap lands. World KB has NO entity creation route:
 * `patch-entity` is edit-only (pre-reads the entity → 500 DATABASE_ERROR
 * when the minted id does not exist), so this dialog's create path would
 * fail at runtime. The "新建 era" entry in the Brief-layer chrome is
 * hard-hidden (`showCreateEra={false}` in `timeline-canvas.tsx`) while
 * deferred. The component is retained verbatim for one-flag activation when
 * a backend create carrier exists (next iteration, backend scope). The
 * QC1-I-002 split-create edge (entity created but parent relationship step
 * fails) is likewise deferred with F-001 — it is moot while the dialog is
 * unreachable.
 *
 * Original purpose (kept for context): the "新建 era" entry point for the
 * World Timeline Brief layer (spec
 * `.mstar/specs/canvas-strategy-surface.md` §3.3.3 V1.159 amendment —
 * "Create entry"). Opens from the Brief-layer header chrome and creates a
 * new `block_type=era` KnowledgeEntry via the existing World KB write
 * boundary:
 *
 *   1. `world_kb.patch_entity` (V1.73, `POST .../kb/patch-entity`) — create
 *      the era entity. The client mints the new `entity_id` (`kb_<uuid>`,
 *      mirroring the pack-import `mint_entry_id` convention) and sends
 *      `expected_version: 0` (the create convention documented by the
 *      V1.143 daemon test `patch_entity_create_on_existing_returns_409` —
 *      the pre-orchestrator OCC guard 409s when the id already exists).
 *      The patch carries `{ title, body: { attributes: { era_type?,
 *      world_summary: "" } }, block_type: "era" }` — `era_type` rides the
 *      freeform `body.attributes` carrier (no validation enum).
 *
 *   2. `world_kb.patch_relationship` (V1.74, `POST .../kb/patch-relationship`)
 *      — when a parent era is chosen, add the nesting edge:
 *      `relation_type: "custom"`, `custom_label: "parent_era"`,
 *      `source_entity_id` = parent (coarser), `target_entity_id` = new era
 *      (finer), `symmetric: false` (architect VC-1 option c; directed).
 *
 * Success closes the dialog and fires `onSuccess(newEraId)` — the mutation
 * hooks already invalidate the World KB graph query, so the time-bands
 * reflow with the new era (no manual `graph.refetch()` — QC3-S-002).
 * Errors surface inline in the dialog: 422
 * (`world_kb_validation_failed`) shows `validation_summary.errors[]`; 409
 * (`world_kb_conflict`) shows a retry hint (the minted id already exists /
 * concurrent write); any other failure falls through to the hook's global
 * error toast plus a generic inline message.
 *
 * Form fields (per task brief):
 *   1. Era name (required) — becomes `canonical_name`.
 *   2. Era type (optional) — dropdown with the recommended taxonomy values
 *      (kingdom/age/epoch/period/sub-age) + a freeform "custom" input;
 *      becomes `body.attributes.era_type`.
 *   3. Parent era (optional) — searchable dropdown listing `existingEras`;
 *      selected parent creates the `parent_era` relationship.
 */
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Check, Search, X } from 'lucide-react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Button, Input, Label, Select } from '@/components/ui';
import {
  isWorldKbConflictError,
  isWorldKbValidationError,
  usePatchWorldKbEntity,
  usePatchWorldKbRelationship,
} from '@/lib/canvas/use-world-kb-data';
import { PARENT_ERA_LABEL } from './brief-era-tree';

export interface EraCreateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  worldId: string;
  /**
   * Existing era entities for the optional parent picker
   * (`{ entity_id, canonical_name }` pairs).
   */
  existingEras: { entity_id: string; canonical_name: string }[];
  /**
   * Fired with the new era's `key_block_id` after BOTH the entity create and
   * the optional parent-relationship create succeed. The mutation hooks
   * already invalidate the graph query; this callback lets the orchestrator
   * react (e.g. explicit refetch).
   */
  onSuccess?: (newEraId: string) => void;
}

/**
 * Recommended era taxonomy values (spec §3.3.3 V1.159 amendment). Freeform
 * strings — the daemon enforces no enum; unknown values render with the
 * default band color.
 */
const RECOMMENDED_ERA_TYPES = [
  'kingdom',
  'age',
  'epoch',
  'period',
  'sub-age',
] as const;

/** Sentinel `<option>` value for the freeform era-type input. */
const CUSTOM_ERA_TYPE = '__custom__';

/**
 * Mint a fresh World KB entity id. Mirrors the pack-import convention
 * (`kb_<uuid-without-dashes>`) so the daemon's id shape stays consistent
 * across client surfaces.
 */
function mintEraEntityId(): string {
  return `kb_${crypto.randomUUID().replaceAll('-', '')}`;
}

export function EraCreateDialog({
  open,
  onOpenChange,
  worldId,
  existingEras,
  onSuccess,
}: EraCreateDialogProps) {
  const { t } = useTranslation('canvas');
  const patchEntity = usePatchWorldKbEntity(worldId);
  const patchRelationship = usePatchWorldKbRelationship(worldId);

  const [name, setName] = useState('');
  const [eraTypeChoice, setEraTypeChoice] = useState<string>('');
  const [customEraType, setCustomEraType] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Parent picker state — a lightweight searchable combobox (input +
  // filtered listbox; mirrors the command-palette APG pattern).
  const [parentQuery, setParentQuery] = useState('');
  const [parentId, setParentId] = useState<string | null>(null);
  const [parentOpen, setParentOpen] = useState(false);
  const [parentActiveIndex, setParentActiveIndex] = useState(0);
  const parentInputRef = useRef<HTMLInputElement>(null);

  // Reset the form each time the dialog opens.
  useEffect(() => {
    if (open) {
      setName('');
      setEraTypeChoice('');
      setCustomEraType('');
      setParentQuery('');
      setParentId(null);
      setParentOpen(false);
      setParentActiveIndex(0);
      setError(null);
    }
  }, [open]);

  const filteredEras = useMemo(() => {
    const q = parentQuery.trim().toLowerCase();
    if (q.length === 0) return existingEras;
    return existingEras.filter((era) =>
      era.canonical_name.toLowerCase().includes(q),
    );
  }, [existingEras, parentQuery]);

  const selectedParent = existingEras.find((era) => era.entity_id === parentId) ?? null;

  const nameValid = name.trim().length > 0;
  const submitting = patchEntity.isPending || patchRelationship.isPending;

  const eraType =
    eraTypeChoice === CUSTOM_ERA_TYPE
      ? customEraType.trim()
      : eraTypeChoice.trim();

  function handleParentSelect(entityId: string) {
    const era = existingEras.find((e) => e.entity_id === entityId);
    setParentId(entityId);
    setParentQuery(era?.canonical_name ?? '');
    setParentOpen(false);
  }

  function handleParentKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (!parentOpen) {
        setParentOpen(true);
        return;
      }
      setParentActiveIndex((i) => Math.min(i + 1, filteredEras.length - 1));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setParentActiveIndex((i) => Math.max(i - 1, 0));
    } else if (event.key === 'Enter') {
      const active = filteredEras[parentActiveIndex];
      if (parentOpen && active) {
        event.preventDefault();
        handleParentSelect(active.entity_id);
      }
    } else if (event.key === 'Escape') {
      setParentOpen(false);
    }
  }

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!nameValid) {
      setError(t('timeline.eraCreateDialog.nameRequired'));
      return;
    }
    setError(null);

    const newEntityId = mintEraEntityId();
    const attributes: Record<string, unknown> = { world_summary: '' };
    if (eraType.length > 0) attributes.era_type = eraType;

    try {
      const res = await patchEntity.mutateAsync({
        entity_id: newEntityId,
        expected_version: 0,
        patch: {
          title: name.trim(),
          body: { attributes },
          block_type: 'era',
        },
      });
      const newEraId = res.entity.key_block_id;

      if (parentId) {
        await patchRelationship.mutateAsync({
          action: 'add',
          relationship: {
            source_entity_id: parentId,
            target_entity_id: newEraId,
            relation_type: 'custom',
            custom_label: PARENT_ERA_LABEL,
            symmetric: false,
          },
        });
      }

      onOpenChange(false);
      onSuccess?.(newEraId);
    } catch (err) {
      if (isWorldKbValidationError(err)) {
        const errors = extractValidationErrors(err);
        setError(
          errors.length > 0
            ? errors.join(' ')
            : t('timeline.eraCreateDialog.validationError'),
        );
      } else if (isWorldKbConflictError(err)) {
        // 409: the minted entity id already exists (retry-safe create
        // convention) or a concurrent write won the row. Retry hint —
        // closing and submitting again mints a fresh id.
        setError(t('timeline.eraCreateDialog.conflictError'));
      } else {
        // Non-409/422 failures are already surfaced by the hook's global
        // error toast; mirror a generic inline message so the dialog never
        // closes silently on failure.
        setError(t('timeline.eraCreateDialog.genericError'));
      }
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('timeline.eraCreateDialog.title')}
        description={t('timeline.eraCreateDialog.description')}
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="era-create-name">
              {t('timeline.eraCreateDialog.nameLabel')}
            </Label>
            <Input
              id="era-create-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t('timeline.eraCreateDialog.namePlaceholder')}
              invalid={Boolean(error) && !nameValid}
              autoFocus
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="era-create-type">
              {t('timeline.eraCreateDialog.typeLabel')}
            </Label>
            <Select
              id="era-create-type"
              value={eraTypeChoice}
              onChange={(e) => setEraTypeChoice(e.target.value)}
            >
              <option value="">
                {t('timeline.eraCreateDialog.typeNone')}
              </option>
              {RECOMMENDED_ERA_TYPES.map((type) => (
                <option key={type} value={type}>
                  {type}
                </option>
              ))}
              <option value={CUSTOM_ERA_TYPE}>
                {t('timeline.eraCreateDialog.typeCustom')}
              </option>
            </Select>
            {eraTypeChoice === CUSTOM_ERA_TYPE ? (
              <Input
                id="era-create-custom-type"
                value={customEraType}
                onChange={(e) => setCustomEraType(e.target.value)}
                placeholder={t('timeline.eraCreateDialog.typeCustomPlaceholder')}
              />
            ) : null}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="era-create-parent">
              {t('timeline.eraCreateDialog.parentLabel')}
            </Label>
            <div className="relative">
              <Input
                id="era-create-parent"
                ref={parentInputRef}
                role="combobox"
                aria-expanded={parentOpen && filteredEras.length > 0}
                aria-controls="era-create-parent-listbox"
                aria-autocomplete="list"
                aria-activedescendant={
                  parentOpen && filteredEras[parentActiveIndex]
                    ? `era-create-parent-option-${parentActiveIndex}`
                    : undefined
                }
                value={parentQuery}
                onChange={(e) => {
                  setParentId(null);
                  setParentQuery(e.target.value);
                  setParentOpen(true);
                  setParentActiveIndex(0);
                }}
                onFocus={() => setParentOpen(true)}
                onKeyDown={handleParentKeyDown}
                placeholder={
                  existingEras.length === 0
                    ? t('timeline.eraCreateDialog.parentEmpty')
                    : t('timeline.eraCreateDialog.parentPlaceholder')
                }
              />
              <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center" aria-hidden="true">
                <Search className="h-4 w-4 text-gray-700" />
              </span>
              {selectedParent ? (
                <button
                  type="button"
                  onClick={() => {
                    setParentId(null);
                    setParentQuery('');
                    parentInputRef.current?.focus();
                  }}
                  aria-label={t('timeline.eraCreateDialog.parentClear')}
                  className="absolute inset-y-0 right-9 flex items-center text-gray-700 hover:text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
                >
                  <X className="h-4 w-4" aria-hidden />
                </button>
              ) : null}
              {parentOpen && filteredEras.length > 0 ? (
                <ul
                  id="era-create-parent-listbox"
                  role="listbox"
                  aria-label={t('timeline.eraCreateDialog.parentLabel')}
                  className="absolute z-20 mt-1 max-h-48 w-full overflow-y-auto rounded-popover border border-gray-alpha-400 bg-background-100 p-1 shadow-elevation-4"
                >
                  {filteredEras.map((era, index) => (
                    <li
                      key={era.entity_id}
                      id={`era-create-parent-option-${index}`}
                      role="option"
                      aria-selected={era.entity_id === parentId}
                      data-testid="era-create-parent-option"
                      onMouseDown={(e) => {
                        // mousedown precedes blur — select before the input
                        // loses focus so the listbox stays interactive.
                        e.preventDefault();
                        handleParentSelect(era.entity_id);
                      }}
                      className={`flex cursor-pointer items-center justify-between gap-2 rounded-control px-2 py-1.5 text-copy-14 ${
                        index === parentActiveIndex
                          ? 'bg-gray-alpha-100 text-gray-1000'
                          : 'text-gray-900'
                      }`}
                    >
                      <span className="truncate">{era.canonical_name}</span>
                      {era.entity_id === parentId ? (
                        <Check className="h-4 w-4 flex-shrink-0 text-blue-700" aria-hidden />
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          </div>

          {error ? (
            <p
              className="text-copy-13 text-red-700"
              role="alert"
              data-testid="era-create-dialog-error"
            >
              {error}
            </p>
          ) : null}

          <div className="flex justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => onOpenChange(false)}
            >
              {t('common:action.cancel')}
            </Button>
            <Button
              type="submit"
              variant="primary"
              size="small"
              // Disabled only while a mutation is in flight — an empty-name
              // submit must stay clickable so the inline required-name error
              // surfaces (form validation lives in `handleSubmit`).
              disabled={submitting}
              data-testid="era-create-submit"
            >
              {submitting
                ? t('timeline.eraCreateDialog.creating')
                : t('timeline.eraCreateDialog.create')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Extract `validation_summary.errors[]` from a World KB 422 error. The
 * daemon's `WorldKbValidationError` carries the array under
 * `details.validation_summary.errors`; defensive `in`/`typeof` narrowing
 * keeps the dialog honest if the details shape drifts (no unchecked casts).
 */
function extractValidationErrors(err: unknown): string[] {
  if (err === null || typeof err !== 'object') return [];
  if (!('details' in err)) return [];
  const details = err.details;
  if (details === null || typeof details !== 'object') return [];
  if (!('validation_summary' in details)) return [];
  const summary = details.validation_summary;
  if (summary === null || typeof summary !== 'object') return [];
  if (!('errors' in summary)) return [];
  const errors = summary.errors;
  if (!Array.isArray(errors)) return [];
  return errors.filter((e): e is string => typeof e === 'string');
}
