/**
 * World KB entity authoring vocabulary + pure `modules.mental` / `modules.belief`
 * serialization (v1.203 P2 module carriers, extracted per QC1-S001 /
 * `R-V1203P2QC1-S001`).
 *
 * Lower-layer home for the shared holder-kind policy and the module dialects:
 * everything here depends on the wire-contract types only — no React, no
 * component-module import — so any surface consumes the vocabulary without
 * reaching into another surface's component. Per-inspector field mapping (form
 * seeding, dirty checks, 422 label mapping) stays with each editor.
 */
import type { BlockType, WorldKbEntityPatch } from '@42ch/nexus-contracts';

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
   * Raw-JSON fallback mode: non-null when the stored `modules.mental` member
   * is nonrepresentable for the structured nine-field editor (a stored
   * non-object value — array, string, number, …). Holds the JSON text of the
   * COMPLETE stored value so an untouched seed stays byte-identical (not
   * dirty) and an unrelated save never rewrites the dialect (mirror of
   * `beliefRawText`, QC1-F001/QC3-001; preservation discipline rule 1–2).
   * The author repairs it deliberately as JSON; structured fields stay
   * active only when the stored value is a representable object.
   * Optional: `formFromEntity` always sets it; hand-rolled form literals in
   * reapply-path tests may omit it (absent = structured mode).
   */
  mentalRawText?: string | null;
  /**
   * STOP-condition fallback rows: stored mental keys outside the locked
   * vocabulary, edited as raw JSON so unknown inner keys round-trip verbatim.
   * A blanked row clears that inner key on save (container-level semantics).
   */
  mentalExtras: Array<{ key: string; text: string }>;
  /** `modules.belief` rows; '' on a field means "absent" on that row. */
  beliefs: BeliefRowForm[];
  /**
   * Raw-JSON fallback mode (QC1-F001/QC3-001): non-null when the stored
   * `modules.belief` member is nonrepresentable for the structured row
   * editor (non-array value, or an array containing a non-object row).
   * Holds the JSON text of the COMPLETE stored value so an untouched seed
   * stays byte-identical (not dirty) and an unrelated save never rewrites
   * the dialect; the author repairs it deliberately as JSON (mirrors the
   * O3 observation raw fallback, timeline-inspector C1).
   */
  beliefRawText: string | null;
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

/** Kinds that can hold mental/belief modules (V1.164 PD-8/PD-12 — do not widen). */
export const HOLDER_BLOCK_TYPES: readonly BlockType[] = ['character', 'faction', 'organization'];

/** Handbook order for the nine-field mental table (product locks §Mental field vocabulary). */
export const MENTAL_FIELD_ORDER = [
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

/** Belief row fields in handbook order (BeliefPropositionRaw member names). */
export const BELIEF_FIELDS = [
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

export type BeliefField = (typeof BELIEF_FIELDS)[number];

export function stringifyJson(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

/**
 * Stored member → editable/compare text. Primitives keep their plain `String`
 * text (closed-label selects must match the frozen handbook strings exactly,
 * e.g. 'True'); structured values render as stable pretty JSON instead of
 * the `[object Object]` default stringification (no-base-to-string).
 */
export function memberText(value: unknown): string {
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint') {
    return String(value);
  }
  return stringifyJson(value);
}

/**
 * Parse the mental module texts into the complete first-level value.
 * Returns per-field errors keyed `mental.<key>` (the 422 mapping key shape).
 *
 * QC1-F002: arbitrary stored keys — including a legal unknown own key like
 * `__proto__` accepted under PD-13 — are collected as entries and built with
 * `Object.fromEntries`, which creates OWN DATA properties. Plain assignment
 * (`value[key] = …`) would invoke the inherited `__proto__` setter and lose
 * the key from JSON serialization on both submit and conflict reapply (this
 * builder backs both paths, including `modulesPatchFromForm`).
 *
 * Raw fallback (Greptile P1 fix) — `form.mentalRawText !== null` when the
 * stored member was a nonrepresentable non-object value. The parsed JSON
 * object becomes the complete value verbatim (deliberate repair); invalid
 * JSON or a non-object parse keeps the raw text in the built value so the
 * exact dirty check still sees the edit (mirrors the belief `raw` handling)
 * while the error blocks the whole write.
 */
export function buildMentalValue(
  form: EntityEditForm,
  jsonError: (field: string) => string,
): { value: Record<string, unknown>; errors: Record<string, string> } {
  // `!= null` (not `!== null`): hand-rolled form literals may leave the
  // field undefined — only a STRING enables raw mode.
  if (form.mentalRawText != null) {
    const rawText = form.mentalRawText.trim();
    try {
      const parsed: unknown = JSON.parse(rawText);
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
        throw new Error('mental must be a JSON object');
      }
      return { value: parsed as Record<string, unknown>, errors: {} };
    } catch {
      return {
        value: { raw: rawText },
        errors: { 'mental.raw': jsonError('raw') },
      };
    }
  }
  const entries: Array<[string, unknown]> = [];
  const errors: Record<string, string> = {};
  const put = (key: string, text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return; // blank clears / omits the inner key
    try {
      entries.push([key, JSON.parse(trimmed)]);
    } catch {
      errors[`mental.${key}`] = jsonError(key);
    }
  };
  for (const key of MENTAL_FIELD_ORDER) put(key, form.mental[key]);
  for (const extra of form.mentalExtras) put(extra.key, extra.text);
  return { value: Object.fromEntries(entries), errors };
}

/**
 * Parse the belief rows into the complete first-level array. Rows seeded from
 * the stored array start from their complete original content (`original`):
 * an edited member applies on top (blank clears the member), while untouched
 * members — unknown inner keys, nulls, intentional whitespace — round-trip
 * exactly (frozen write contract §3 / PD-13, L2-T2-002). Rows added in the
 * form have no `original` and contribute
 * non-empty trimmed members only; all-blank new rows are dropped. An
 * untouched seeded row — every editable member equal to its stored text,
 * even a degenerate stored `{}` — round-trips verbatim instead of being
 * dropped, so an unrelated edit cannot mark belief dirty or shorten the
 * stored array (L2-T2-002 re-review remainder). Explicit remove (row
 * deletion) and blank-to-clear of a populated row keep their delete
 * semantics: clearing every member of a stored row drops it, while clearing
 * members of an already-empty row is a no-op that preserves the row.
 * Returns per-field errors keyed `belief.<index>.<field>` (the 422 mapping
 * key shape) — `<index>` is the FORM row index for client-side errors.
 *
 * Raw fallback (QC1-F001/QC3-001) — `form.beliefRawText !== null` when the
 * stored member was nonrepresentable (non-array, or containing a non-object
 * row). The parsed JSON array becomes the value verbatim; invalid JSON or a
 * non-array parse keeps the raw text in the built value so the exact dirty
 * check still sees the edit (mirrors the `order` handling) while the error
 * blocks the whole write. Deliberate repair is the only way the value
 * changes.
 *
 * `wireToForm` (QC1-F003) — for each emitted wire element, the index of the
 * FORM row that produced it. The builder compacts the submitted array
 * (dropped blank rows / cleared stored rows), so daemon 422 row addresses
 * (wire indices) must be translated through this map before they can be
 * mapped onto form rows.
 */
export function buildBeliefValue(
  form: EntityEditForm,
  orderError: (field: string) => string,
): {
  value: Array<Record<string, unknown>>;
  errors: Record<string, string>;
  wireToForm: number[];
} {
  if (form.beliefRawText !== null) {
    const rawText = form.beliefRawText.trim();
    try {
      const parsed: unknown = JSON.parse(rawText);
      if (!Array.isArray(parsed)) throw new Error('belief must be a JSON array');
      return { value: parsed as Array<Record<string, unknown>>, errors: {}, wireToForm: [] };
    } catch {
      return {
        value: [{ raw: rawText }],
        errors: { 'belief.raw': orderError('raw') },
        wireToForm: [],
      };
    }
  }
  const value: Array<Record<string, unknown>> = [];
  const wireToForm: number[] = [];
  const errors: Record<string, string> = {};
  form.beliefs.forEach((row, index) => {
    let untouched = row.original !== undefined;
    if (untouched) {
      for (const field of BELIEF_FIELDS) {
        const originalValue = row.original![field];
        const originalText = originalValue === undefined || originalValue === null ? '' : memberText(originalValue);
        if (row[field] !== originalText) {
          untouched = false;
          break;
        }
      }
    }
    if (untouched) {
      // Seeded row with no author edit: emit the stored content exactly —
      // including a stored `{}`, which must survive at its original index.
      value.push({ ...row.original! });
      wireToForm.push(index);
      return;
    }
    const out: Record<string, unknown> = row.original ? { ...row.original } : {};
    for (const field of BELIEF_FIELDS) {
      const text = row[field];
      if (row.original) {
        const originalValue = row.original[field];
        const originalText = originalValue === undefined || originalValue === null ? '' : memberText(originalValue);
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
    if (Object.keys(out).length > 0) {
      value.push(out);
      wireToForm.push(index);
    }
  });
  return { value, errors, wireToForm };
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
