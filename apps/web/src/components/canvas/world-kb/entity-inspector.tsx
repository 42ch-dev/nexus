/**
 * World KB entity inspector — edits a confirmed/rejected/merged KeyBlock entity
 * via `world_kb.patch_entity` (V1.73 P0 A6).
 *
 * Edits title / body / aliases / block_type plus — on holder-capable kinds
 * (`character` / `faction` / `organization`, V1.164 PD-8/PD-12) — the
 * functional-dialect `modules.mental` and `modules.belief` carriers (v1.203
 * P2, whole-first-level-value upsert per AR-4/PD-12). Daemon 422s use the
 * frozen field-prefix grammar `modules.<dialect>[.<index>].<field>: <reason>`
 * and are mapped 1:1 back onto the offending form fields; 409s hand off to
 * the parent canvas conflict modal. Body is shown as a JSON summary field
 * because the V1.73 entity body is a free-form `Record<string, unknown>`
 * projection; a rich body editor is V1.74.
 *
 * STOP-condition contract: stored `modules.mental` keys outside the locked
 * nine-field vocabulary are never silently dropped — they render as raw-JSON
 * fallback rows (key + JSON text area) so unknown inner keys round-trip
 * verbatim (PD-13).
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
  /**
   * `modules.mental` as JSON text per locked nine-field key ('' = absent).
   * Whole-first-level-value upsert: the parsed object is written back complete.
   */
  mental: Record<(typeof MENTAL_FIELD_ORDER)[number], string>;
  /**
   * STOP-condition fallback rows: stored mental keys outside the locked
   * vocabulary, edited as raw JSON so unknown inner keys round-trip verbatim.
   * A blanked row clears that inner key on save (container-level semantics).
   */
  mentalExtras: Array<{ key: string; text: string }>;
  /** `modules.belief` rows; '' on a field means "absent" on that row. */
  beliefs: BeliefRowForm[];
}

/** One editable `modules.belief` row (BeliefPropositionRaw as flat strings). */
export interface BeliefRowForm {
  holder: string;
  proposition: string;
  order: string;
  truth: string;
  access: string;
  representation: string;
  content_type: string;
  source: string;
  context: string;
  /**
   * Complete stored row this form row was seeded from (absent for rows added
   * in the form). Preserved so untouched members — unknown inner keys, nulls,
   * intentional whitespace — round-trip verbatim on whole-value write
   * (frozen write contract §3 / PD-13, L2-T2-002).
   */
  original?: Record<string, unknown>;
}

/** Functional module dialects carried under `modules` (mental / belief). */
export type ModuleDialect = 'mental' | 'belief';

/** Build the form from a selected node's backing projection. */
export function formFromEntity(entity: WorldKbEntityProjection): EntityEditForm {
  const mental = mentalRecord(entity);
  const mentalForm = Object.fromEntries(MENTAL_FIELD_ORDER.map((key) => [key, ''])) as EntityEditForm['mental'];
  const mentalExtras: EntityEditForm['mentalExtras'] = [];
  for (const [key, value] of Object.entries(mental)) {
    if ((MENTAL_FIELD_ORDER as readonly string[]).includes(key)) {
      mentalForm[key as (typeof MENTAL_FIELD_ORDER)[number]] = stringifyJson(value);
    } else {
      mentalExtras.push({ key, text: stringifyJson(value) });
    }
  }
  return {
    title: entity.canonical_name,
    bodyText: entity.body ? JSON.stringify(entity.body, null, 2) : '',
    aliasesText: (entity.aliases ?? []).join(', '),
    block_type: entity.block_type,
    mental: mentalForm,
    mentalExtras,
    beliefs: beliefRowsFromEntity(entity),
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
  if (mentalDirty(form, entity) || beliefsDirty(form, entity)) fields.push('modules');
  return fields;
}

export type WorldKbEntityField = 'title' | 'body' | 'aliases' | 'block_type' | 'modules';

const FIELD_LABEL_KEYS: Record<WorldKbEntityField, string> = {
  title: 'worldKb.entityInspector.field.title',
  body: 'worldKb.entityInspector.field.body',
  aliases: 'worldKb.entityInspector.field.aliases',
  block_type: 'worldKb.entityInspector.field.blockType',
  modules: 'worldKb.entityInspector.field.modules',
};

/** Kinds that can hold mental/belief modules (V1.164 PD-8/PD-12 — do not widen). */
export const HOLDER_BLOCK_TYPES: readonly BlockType[] = ['character', 'faction', 'organization'];

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

/** Belief row fields in handbook order (BeliefPropositionRaw member names). */
const BELIEF_FIELDS = [
  'holder',
  'proposition',
  'order',
  'truth',
  'access',
  'representation',
  'content_type',
  'source',
  'context',
] as const;

type BeliefField = (typeof BELIEF_FIELDS)[number];

/** Closed handbook label spaces (mirrors the daemon's `validate_closed_belief_labels`). */
const BELIEF_CLOSED_LABELS: Record<Exclude<BeliefField, 'holder' | 'proposition' | 'order'>, readonly string[]> = {
  truth: ['True', 'False', 'Unknown'],
  access: ['Private', 'Shared', 'Public'],
  representation: ['Explicit', 'Implicit'],
  content_type: [
    'Location',
    'Contents/Physical State',
    'Identity/Relation',
    'Epistemic',
    'Desire/Intention',
    'Emotion',
    'Trait/Value',
    'Action/Event',
  ],
  source: ['Narration', 'Perception', 'Memory', 'Testimony', 'Inference', 'Imagination', 'Unknown'],
  context: ['Deceptive', 'Temporal', 'Counterfactual', 'Neutral'],
};

const BELIEF_FIELD_LABEL_KEYS: Record<BeliefField, string> = {
  holder: 'worldKb.entityInspector.belief.field.holder',
  proposition: 'worldKb.entityInspector.belief.field.proposition',
  order: 'worldKb.entityInspector.belief.field.order',
  truth: 'worldKb.entityInspector.belief.field.truth',
  access: 'worldKb.entityInspector.belief.field.access',
  representation: 'worldKb.entityInspector.belief.field.representation',
  content_type: 'worldKb.entityInspector.belief.field.contentType',
  source: 'worldKb.entityInspector.belief.field.source',
  context: 'worldKb.entityInspector.belief.field.context',
};

/** Closed-label belief fields as a lookup (422 prefix-mapping target check). */
const BELIEF_CLOSED_FIELDS: Record<string, true> = {
  truth: true,
  access: true,
  representation: true,
  content_type: true,
  source: true,
  context: true,
};

function emptyBeliefRow(): BeliefRowForm {
  return {
    holder: '',
    proposition: '',
    order: '',
    truth: '',
    access: '',
    representation: '',
    content_type: '',
    source: '',
    context: '',
  };
}

function stringifyJson(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

/** Stored `modules.mental` as a plain record ('{}' when absent / null / non-object). */
function mentalRecord(entity: WorldKbEntityProjection): Record<string, unknown> {
  const mental = entity.modules?.mental;
  return mental !== undefined && mental !== null && typeof mental === 'object' && !Array.isArray(mental)
    ? (mental as Record<string, unknown>)
    : {};
}

/** Stored `modules.belief` object rows verbatim ('[]' when absent / non-array). */
function beliefRowsRaw(entity: WorldKbEntityProjection): Array<Record<string, unknown>> {
  const belief = entity.modules?.belief;
  if (!Array.isArray(belief)) return [];
  return belief.filter(
    (raw): raw is Record<string, unknown> =>
      raw !== null && typeof raw === 'object' && !Array.isArray(raw),
  );
}

/**
 * Stored `modules.belief` as flat editable rows. Known members seed with the
 * exact stored text (absent / null → '', no trimming); every row retains its
 * complete stored content in `original` so untouched members round-trip
 * verbatim.
 */
function beliefRowsFromEntity(entity: WorldKbEntityProjection): BeliefRowForm[] {
  return beliefRowsRaw(entity).map((row) => {
    const form = emptyBeliefRow();
    for (const field of BELIEF_FIELDS) {
      const value = row[field];
      form[field] = value === undefined || value === null ? '' : String(value);
    }
    return { ...form, original: { ...row } };
  });
}

function mentalDirty(form: EntityEditForm, entity: WorldKbEntityProjection): boolean {
  const canonical = mentalRecord(entity);
  for (const key of MENTAL_FIELD_ORDER) {
    const canonText = key in canonical ? stringifyJson(canonical[key]) : '';
    if (form.mental[key].trim() !== canonText.trim()) return true;
  }
  const canonExtras = Object.keys(canonical).filter((key) => !(MENTAL_FIELD_ORDER as readonly string[]).includes(key));
  if (form.mentalExtras.length !== canonExtras.length) return true;
  return form.mentalExtras.some((extra, i) => {
    if (extra.key !== canonExtras[i]) return true;
    return extra.text.trim() !== stringifyJson(canonical[extra.key]).trim();
  });
}

/**
 * Parse the mental module texts into the complete first-level value.
 * Returns per-field errors keyed `mental.<key>` (the 422 mapping key shape).
 */
function buildMentalValue(
  form: EntityEditForm,
  jsonError: (field: string) => string,
): { value: Record<string, unknown>; errors: Record<string, string> } {
  const value: Record<string, unknown> = {};
  const errors: Record<string, string> = {};
  const put = (key: string, text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return; // blank clears / omits the inner key
    try {
      value[key] = JSON.parse(trimmed);
    } catch {
      errors[`mental.${key}`] = jsonError(key);
    }
  };
  for (const key of MENTAL_FIELD_ORDER) put(key, form.mental[key]);
  for (const extra of form.mentalExtras) put(extra.key, extra.text);
  return { value, errors };
}

/**
 * Exact dirty check: the built whole value must match the stored array
 * verbatim. No normalization on either side — a normalized comparison would
 * hide loss of unknown inner keys or stored whitespace (L2-T2-002).
 */
function beliefsDirty(form: EntityEditForm, entity: WorldKbEntityProjection): boolean {
  const belief = entity.modules?.belief;
  const canonical = Array.isArray(belief) ? belief : [];
  return JSON.stringify(buildBeliefValue(form, (field) => field).value) !== JSON.stringify(canonical);
}

/**
 * Parse the belief rows into the complete first-level array. Rows seeded from
 * the stored array start from their complete original content (`original`):
 * an edited member applies on top (blank clears the member), while untouched
 * members — unknown inner keys, nulls, intentional whitespace — round-trip
 * exactly (frozen write contract §3 / PD-13, L2-T2-002). Rows added in the
 * form have no `original` and contribute non-empty trimmed members only.
 * All-blank rows are dropped. Returns per-field errors keyed
 * `belief.<index>.<field>` (the 422 mapping key shape).
 */
function buildBeliefValue(
  form: EntityEditForm,
  orderError: (field: string) => string,
): { value: Array<Record<string, unknown>>; errors: Record<string, string> } {
  const value: Array<Record<string, unknown>> = [];
  const errors: Record<string, string> = {};
  form.beliefs.forEach((row, index) => {
    const out: Record<string, unknown> = row.original ? { ...row.original } : {};
    for (const field of BELIEF_FIELDS) {
      const text = row[field];
      if (row.original) {
        const originalValue = row.original[field];
        const originalText = originalValue === undefined || originalValue === null ? '' : String(originalValue);
        if (text === originalText) continue; // untouched: keep the stored value exactly
        if (!text.trim()) {
          delete out[field]; // blanked by the author: clear the member
          continue;
        }
      } else if (!text.trim()) {
        continue;
      }
      if (field === 'order') {
        const n = Number(text.trim());
        if (!Number.isInteger(n)) {
          errors[`belief.${index}.order`] = orderError(field);
          // Keep the invalid raw text in the built value so the exact dirty
          // check still sees the edit — otherwise the row would compare equal
          // to the stored one and the submit (which surfaces this error)
          // could never fire. The error blocks the write, so the raw string
          // never reaches the wire.
          out.order = text.trim();
          continue;
        }
        out.order = n;
      } else {
        out[field] = text.trim();
      }
    }
    if (Object.keys(out).length > 0) value.push(out);
  });
  return { value, errors };
}

/**
 * Best-effort complete modules value for the 409 reapply path, restricted to
 * the exact dialect set that was dirty at submit time (L2-T2-001): reapplying
 * must never rewrite the other dialect — a concurrent writer may have
 * populated it (or the captured form may predate that population). The
 * captured form already passed client-side validation at submit time, so
 * parsed values round-trip; any unparseable requested dialect is omitted
 * rather than blocking reapply.
 */
export function modulesPatchFromForm(
  form: EntityEditForm,
  dialects: readonly ModuleDialect[],
): NonNullable<WorldKbEntityPatch['modules']> | undefined {
  const modules: NonNullable<WorldKbEntityPatch['modules']> = {};
  if (dialects.includes('mental')) {
    const mental = buildMentalValue(form, (field) => field);
    if (Object.keys(mental.errors).length === 0) modules.mental = mental.value;
  }
  if (dialects.includes('belief')) {
    const belief = buildBeliefValue(form, (field) => field);
    if (Object.keys(belief.errors).length === 0) modules.belief = belief.value;
  }
  return Object.keys(modules).length > 0 ? modules : undefined;
}

/**
 * Map one daemon 422 entry onto a form field when it carries the frozen
 * prefix grammar; unmatched / whole-dialect entries stay section-level with
 * the original entry text verbatim. The complete editable belief-member set
 * maps 1:1 (holder / proposition / order included, L2-T2-003); a belief
 * address only maps when the addressed row actually exists in the form.
 */
function mapValidationEntry(entry: string, beliefRowCount: number): { key?: string; message: string } {
  if (!entry.startsWith('modules.')) return { message: entry };
  const sep = entry.indexOf(': ');
  if (sep === -1) return { message: entry };
  const addr = entry.slice('modules.'.length, sep);
  const reason = entry.slice(sep + 2);
  const parts = addr.split('.');
  if (parts[0] === 'mental' && parts.length === 2) {
    if ((MENTAL_FIELD_ORDER as readonly string[]).includes(parts[1])) {
      return { key: `mental.${parts[1]}`, message: reason };
    }
    return { message: entry };
  }
  if (
    parts[0] === 'belief' &&
    parts.length === 3 &&
    /^\d+$/.test(parts[1]) &&
    (BELIEF_FIELDS as readonly string[]).includes(parts[2]) &&
    Number(parts[1]) < beliefRowCount
  ) {
    return { key: `belief.${parts[1]}.${parts[2]}`, message: reason };
  }
  return { message: entry };
}

/**
 * Read-only "Mental State" section for non-holder kinds (V1.164 P3 Task 3,
 * AC-V1164-12/15 + PD-16): populated values render label + JSON rows, no
 * input controls, no create affordance. Returns null when nothing populated.
 */
function MentalStateReadOnlySection({ mental }: { mental: Record<string, unknown> }) {
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
                {stringifyJson(mental[key])}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
    </section>
  );
}

/** Editable "Mental State" section (holder kinds, v1.203 P2 O1). */
function MentalStateSection({
  form,
  fieldErrors,
  onMentalChange,
  onExtraChange,
}: {
  form: EntityEditForm;
  fieldErrors: Record<string, string>;
  onMentalChange: (key: (typeof MENTAL_FIELD_ORDER)[number], value: string) => void;
  onExtraChange: (index: number, value: string) => void;
}) {
  const { t } = useTranslation('canvas');
  const [open, setOpen] = useState(true);
  const regionId = useId();
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
        <div id={regionId} className="mt-1.5 flex flex-col gap-2">
          {MENTAL_FIELD_ORDER.map((key) => (
            <div key={key} className="flex flex-col gap-0.5">
              <Label htmlFor={`wkbe-mental-${key}`}>{t(MENTAL_FIELD_LABEL_KEYS[key])}</Label>
              <Textarea
                id={`wkbe-mental-${key}`}
                rows={2}
                className="font-mono text-copy-13-mono"
                value={form.mental[key]}
                onChange={(e) => onMentalChange(key, e.target.value)}
                placeholder="{}"
                spellCheck={false}
              />
              {fieldErrors[`mental.${key}`] ? (
                <p className="text-copy-13 text-red-1000">{fieldErrors[`mental.${key}`]}</p>
              ) : null}
            </div>
          ))}
          {form.mentalExtras.map((extra, i) => (
            <div key={extra.key} className="flex flex-col gap-0.5">
              <Label htmlFor={`wkbe-mental-extra-${i}`}>{extra.key}</Label>
              <Textarea
                id={`wkbe-mental-extra-${i}`}
                rows={2}
                className="font-mono text-copy-13-mono"
                value={extra.text}
                onChange={(e) => onExtraChange(i, e.target.value)}
                placeholder="{}"
                spellCheck={false}
              />
              {fieldErrors[`mental.${extra.key}`] ? (
                <p className="text-copy-13 text-red-1000">{fieldErrors[`mental.${extra.key}`]}</p>
              ) : null}
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

/** Editable "Belief Propositions" section (holder kinds, v1.203 P2 O2). */
function BeliefSection({
  beliefs,
  fieldErrors,
  onRowChange,
  onAdd,
  onRemove,
}: {
  beliefs: BeliefRowForm[];
  fieldErrors: Record<string, string>;
  onRowChange: (index: number, field: BeliefField, value: string) => void;
  onAdd: () => void;
  onRemove: (index: number) => void;
}) {
  const { t } = useTranslation('canvas');
  const [open, setOpen] = useState(true);
  const regionId = useId();
  const title = t('worldKb.entityInspector.belief.title');
  return (
    <section
      className="mt-3 border-t border-gray-alpha-300 pt-2"
      data-testid="belief-section"
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
        <div id={regionId} className="mt-1.5 flex flex-col gap-3">
          {beliefs.map((row, index) => (
            <fieldset
              key={index}
              className="flex flex-col gap-1.5 rounded-card border border-gray-alpha-300 p-2"
            >
              {BELIEF_FIELDS.map((field) => {
                const error = fieldErrors[`belief.${index}.${field}`];
                const label = t(BELIEF_FIELD_LABEL_KEYS[field]);
                const id = `wkbe-belief-${index}-${field}`;
                return (
                  <div key={field} className="flex flex-col gap-0.5">
                    <Label htmlFor={id}>{label}</Label>
                    {field === 'proposition' ? (
                      <Textarea
                        id={id}
                        rows={2}
                        value={row[field]}
                        onChange={(e) => onRowChange(index, field, e.target.value)}
                      />
                    ) : BELIEF_CLOSED_FIELDS[field] ? (
                      <Select
                        id={id}
                        value={row[field]}
                        onChange={(e) => onRowChange(index, field, e.target.value)}
                      >
                        <option value="">{t('worldKb.entityInspector.belief.emptyOption')}</option>
                        {BELIEF_CLOSED_LABELS[field as Exclude<BeliefField, 'holder' | 'proposition' | 'order'>].map(
                          (option) => (
                            <option key={option} value={option}>
                              {/*
                                The exact frozen handbook string stays the option *value*;
                                the author-visible label comes from the en/zh-CN catalog
                                (L2-T2-004). Key suffixes are generated with the same
                                non-alphanumeric→'_' slug as the locale JSON entries.
                              */}
                              {t(
                                `worldKb.entityInspector.belief.option.${field}.${option.replace(/[^A-Za-z0-9]+/g, '_')}`,
                                { defaultValue: option },
                              )}
                            </option>
                          ),
                        )}
                      </Select>
                    ) : (
                      <Input
                        id={id}
                        value={row[field]}
                        onChange={(e) => onRowChange(index, field, e.target.value)}
                      />
                    )}
                    {error ? <p className="text-copy-13 text-red-1000">{error}</p> : null}
                  </div>
                );
              })}
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={() => onRemove(index)}
                aria-label={t('worldKb.entityInspector.belief.removeRow', { index: index + 1 })}
              >
                {t('worldKb.entityInspector.belief.removeRow', { index: index + 1 })}
              </Button>
            </fieldset>
          ))}
          <Button type="button" variant="tertiary" size="small" onClick={onAdd}>
            {t('worldKb.entityInspector.belief.addRow')}
          </Button>
        </div>
      ) : null}
    </section>
  );
}

/** Read-only "Belief Propositions" section for non-holder kinds (PD-16 parity with O1). */
function BeliefReadOnlySection({ beliefs }: { beliefs: Array<Record<string, unknown>> }) {
  const { t } = useTranslation('canvas');
  const [open, setOpen] = useState(true);
  const regionId = useId();
  if (beliefs.length === 0) {
    return null;
  }
  const title = t('worldKb.entityInspector.belief.title');
  return (
    <section
      className="mt-3 border-t border-gray-alpha-300 pt-2"
      data-testid="belief-section"
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
          {beliefs.map((row, index) => (
            <div key={index} className="flex flex-col gap-0.5">
              <dt className="text-label-14 font-semibold text-gray-900">
                {t('worldKb.entityInspector.belief.readonlyRow', { index: index + 1 })}
              </dt>
              <dd className="text-copy-13 font-mono text-gray-1000 whitespace-pre-wrap break-words">
                {stringifyJson(row)}
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
    /** Exact module dialect set this submit intended to write ([] when modules was clean). */
    dirtyDialects: ModuleDialect[];
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
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

  // Holder-kind gate (V1.164 PD-8/PD-12): the module editors only open on
  // holder-capable kinds; other kinds keep the PD-16 read-only rendering.
  const isHolderKind = HOLDER_BLOCK_TYPES.includes(entity.block_type);

  // PD-16 read-only path: null / undefined / non-object modules.mental skips
  // the read-only section — no empty panel, no placeholder rows.
  const mentalReadOnly = mentalRecord(entity);

  // Reseed when the selection (or an external reseed signal) changes.
  useEffect(() => {
    setForm(formFromEntity(entity));
    setValidationErrors([]);
    setFieldErrors({});
  }, [entity.key_block_id, reseedSignal]); // eslint-disable-line react-hooks/exhaustive-deps

  function update<K extends keyof EntityEditForm>(field: K, value: EntityEditForm[K]) {
    setForm((prev) => ({ ...prev, [field]: value }));
  }

  const dirty = dirtyFields(form, entity);

  function handleSubmit() {
    if (dirty.length === 0) return;
    setValidationErrors([]);
    setFieldErrors({});

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
    if (dirty.includes('modules')) {
      // Whole-first-level-value upsert (AR-4/PD-12): each dirty dialect is
      // written complete — an untouched dialect is never included, so a
      // belief-only edit cannot wipe mental and vice versa. `audience` is
      // governance and is never emitted here.
      const modules: NonNullable<WorldKbEntityPatch['modules']> = {};
      const mental = buildMentalValue(form, (field) =>
        t('worldKb.entityInspector.mentalState.jsonError', { field }),
      );
      const belief = buildBeliefValue(form, (field) =>
        t('worldKb.entityInspector.belief.orderError', { field }),
      );
      const errors = { ...mental.errors, ...belief.errors };
      if (Object.keys(errors).length > 0) {
        setFieldErrors(errors);
        return;
      }
      if (mentalDirty(form, entity)) modules.mental = mental.value;
      if (beliefsDirty(form, entity)) modules.belief = belief.value;
      patchBody.modules = modules;
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
            const entries =
              details?.validation_summary?.errors ?? [t('worldKb.entityInspector.validationFailed')];
            // Frozen prefix grammar (write contract §3): split on the first
            // ": " for `modules.`-prefixed entries and map known prefixes 1:1
            // onto form fields; unmatched entries stay section-level verbatim.
            const mapped = entries.map((entry) => mapValidationEntry(entry, form.beliefs.length));
            setFieldErrors(
              Object.fromEntries(mapped.filter((m) => m.key).map((m) => [m.key as string, m.message])),
            );
            setValidationErrors(mapped.filter((m) => !m.key).map((m) => m.message));
            return;
          }
          // Conflict (409) — hand off to the canvas to render the modal.
          const details = error as unknown as {
            status: number;
            details?: { current_version?: number; conflicting_path?: string; entity_id?: string };
          };
          if (details.status === 409) {
            // Retain the exact per-dialect dirty set (L2-T2-001): the reapply
            // must rewrite only the dialects this submit intended — the other
            // dialect may have been populated concurrently while the modal
            // was open, and inferring intent from the captured form is
            // forbidden.
            const dirtyDialects: ModuleDialect[] = [];
            if (dirty.includes('modules')) {
              if (mentalDirty(form, entity)) dirtyDialects.push('mental');
              if (beliefsDirty(form, entity)) dirtyDialects.push('belief');
            }
            onConflict({
              currentVersion: details.details?.current_version ?? node.version,
              entityId: details.details?.entity_id ?? entity.key_block_id,
              conflictingPath: details.details?.conflicting_path ?? dirty.join(','),
              draft: form,
              dirtyFields: dirty,
              dirtyDialects,
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

      {isHolderKind ? (
        <>
          {/* Keyed by entity id (S-3, QC fix wave): React otherwise keeps the
              component instance (and its `open` collapse state) when switching
              between entities — sections must reset to expanded. */}
          <MentalStateSection
            key={`mental-${entity.key_block_id}`}
            form={form}
            fieldErrors={fieldErrors}
            onMentalChange={(key, value) =>
              update('mental', { ...form.mental, [key]: value })
            }
            onExtraChange={(index, value) =>
              update(
                'mentalExtras',
                form.mentalExtras.map((extra, i) => (i === index ? { ...extra, text: value } : extra)),
              )
            }
          />
          <BeliefSection
            key={`belief-${entity.key_block_id}`}
            beliefs={form.beliefs}
            fieldErrors={fieldErrors}
            onRowChange={(index, field, value) =>
              update(
                'beliefs',
                form.beliefs.map((row, i) => (i === index ? { ...row, [field]: value } : row)),
              )
            }
            onAdd={() => update('beliefs', [...form.beliefs, emptyBeliefRow()])}
            onRemove={(index) => update('beliefs', form.beliefs.filter((_, i) => i !== index))}
          />
        </>
      ) : (
        <>
          <MentalStateReadOnlySection key={`mental-ro-${entity.key_block_id}`} mental={mentalReadOnly} />
          <BeliefReadOnlySection
            key={`belief-ro-${entity.key_block_id}`}
            beliefs={beliefRowsRaw(entity)}
          />
        </>
      )}

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
    </form>
  );
}
