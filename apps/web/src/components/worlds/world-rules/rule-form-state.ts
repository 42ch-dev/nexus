/**
 * RuleForm state + client validation mirror (V1.169 P2 T1, DF-82).
 *
 * The mirror validates the AR-2 carrier grammar (`constraint.rs`
 * member-aware parse) just enough to intercept obvious mistakes at submit
 * time — empty module_key, min > max, required_field operand misuse, blank
 * name/summary after trim. The API's field-level `invalid_input` errors are
 * the SSOT (locks AR-2): error keys use the AR-2 closed field vocabulary
 * verbatim (`canonical_name`, `statement`, `constraint.family`,
 * `constraint.module_key`, `constraint.field`, `constraint.min`,
 * `constraint.max`, `target_entry_types`, ...) so an API `details.field`
 * maps onto the form with zero translation.
 *
 * The carrier shapes mirror `constraint.rs`'s six locked forms:
 *   { family: module_presence|module_absence, module_key }
 *   { family: required_field, field ∈ body.summary|body.tags }
 *   { family: required_field, module_key, field }
 *   { family: observer_cardinality, min?, max? }  (≥1 bound, min ≤ max)
 */
import type { WorldRuleCreateRequest } from '@42ch/nexus-contracts';

/** The closed four-family set (DR-70 lane owns any fifth family). */
export const RULE_FAMILIES = [
  'module_presence',
  'module_absence',
  'required_field',
  'observer_cardinality',
] as const;
export type RuleFamily = (typeof RULE_FAMILIES)[number];

/** The closed entry-level `required_field` field set (AR-2). */
export const ENTRY_FIELDS = ['body.summary', 'body.tags'] as const;

/** Core severity vocabulary the picker sends (omitted → evaluation defaults warning, PD-1). */
export const SEVERITY_OPTIONS = ['info', 'warning', 'error'] as const;
export type SeverityOption = (typeof SEVERITY_OPTIONS)[number];

/** Create statuses: default `active` (auto-include), optional `draft`. */
export const RULE_STATUS_OPTIONS = ['active', 'draft'] as const;

/** `required_field` operand forms: entry-level closed field vs module-row. */
export type RequiredFieldOperand = 'entry' | 'module-row';

export interface RuleFormState {
  /** Null until the author picks a family (family picker first). */
  family: RuleFamily | null;
  canonicalName: string;
  statement: string;
  /** module_presence / module_absence operand. */
  moduleKey: string;
  /** required_field operand form (radio); null = neither chosen. */
  requiredFieldOperand: RequiredFieldOperand | null;
  /** Entry-level operand: one of {@link ENTRY_FIELDS}. */
  entryField: string;
  /** Module-row operand: module key. */
  requiredModuleKey: string;
  /** Module-row operand: free-form field. */
  requiredModuleField: string;
  /** observer_cardinality bounds as raw input strings. */
  min: string;
  max: string;
  status: (typeof RULE_STATUS_OPTIONS)[number];
  /** '' = omitted → the daemon stores NULL → evaluation defaults `warning`. */
  severityHint: '' | SeverityOption;
  /** Target axis for the three entry families (mutually exclusive with observer_cardinality). */
  targetEntryTypes: string[];
}

export function initialRuleFormState(): RuleFormState {
  return {
    family: null,
    canonicalName: '',
    statement: '',
    moduleKey: '',
    requiredFieldOperand: null,
    entryField: '',
    requiredModuleKey: '',
    requiredModuleField: '',
    min: '',
    max: '',
    status: 'active',
    severityHint: '',
    targetEntryTypes: [],
  };
}

/**
 * AR-2 closed field vocabulary — keys mirror `details.field` verbatim so API
 * errors map 1:1 onto the form (locks AR-2). `constraint` / `patch` are
 * form-level keys (never produced by this form, mapped defensively).
 */
export type RuleFormErrorKey =
  | 'canonical_name'
  | 'statement'
  | 'constraint'
  | 'constraint.family'
  | 'constraint.module_key'
  | 'constraint.field'
  | 'constraint.min'
  | 'constraint.max'
  | 'target_entry_types'
  | 'severity_hint'
  | 'status'
  | 'patch';

/** Validation result: AR-2 field → error code (component maps codes → copy). */
export type RuleFormErrorCode =
  | 'familyRequired'
  | 'nameRequired'
  | 'statementRequired'
  | 'moduleKeyRequired'
  | 'operandRequired'
  | 'entryFieldRequired'
  | 'moduleFieldRequired'
  | 'moduleFieldReserved'
  | 'minRequired'
  | 'boundInvalid'
  | 'minMax'
  | 'targetTypesConflict';

export type RuleFormErrors = Partial<Record<RuleFormErrorKey, string>>;

/** Parse an observer bound: absent (empty), invalid (non-whole), or a valid u64. */
type Bound = { kind: 'absent' } | { kind: 'invalid' } | { kind: 'valid'; value: number };

function parseBound(value: string): Bound {
  const trimmed = value.trim();
  if (trimmed.length === 0) return { kind: 'absent' };
  if (!/^\d+$/.test(trimmed)) return { kind: 'invalid' };
  const n = Number(trimmed);
  return Number.isSafeInteger(n) ? { kind: 'valid', value: n } : { kind: 'invalid' };
}

/**
 * Client validation mirror — immediate feedback only, the API is the SSOT.
 * Returns AR-2-keyed error codes; `{}` means the form can submit.
 */
export function validateRuleForm(state: RuleFormState): Partial<Record<RuleFormErrorKey, RuleFormErrorCode>> {
  const errors: Partial<Record<RuleFormErrorKey, RuleFormErrorCode>> = {};

  if (state.canonicalName.trim().length === 0) errors.canonical_name = 'nameRequired';
  if (state.statement.trim().length === 0) errors.statement = 'statementRequired';

  if (state.family === null) {
    errors['constraint.family'] = 'familyRequired';
    return errors;
  }

  switch (state.family) {
    case 'module_presence':
    case 'module_absence':
      if (state.moduleKey.trim().length === 0) errors['constraint.module_key'] = 'moduleKeyRequired';
      break;
    case 'required_field': {
      if (state.requiredFieldOperand === null) {
        // Operand misuse: neither form chosen.
        errors['constraint.field'] = 'operandRequired';
      } else if (state.requiredFieldOperand === 'entry') {
        if (state.entryField.length === 0) errors['constraint.field'] = 'entryFieldRequired';
      } else {
        if (state.requiredModuleKey.trim().length === 0) {
          errors['constraint.module_key'] = 'moduleKeyRequired';
        }
        if (state.requiredModuleField.trim().length === 0) {
          errors['constraint.field'] = 'moduleFieldRequired';
        } else if ((ENTRY_FIELDS as readonly string[]).includes(state.requiredModuleField.trim())) {
          // Operand misuse: entry-level value combined with a module row.
          errors['constraint.field'] = 'moduleFieldReserved';
        }
      }
      break;
    }
    case 'observer_cardinality': {
      const min = parseBound(state.min);
      const max = parseBound(state.max);
      if (min.kind === 'absent' && max.kind === 'absent') {
        errors['constraint.min'] = 'minRequired';
      } else {
        if (min.kind === 'invalid') errors['constraint.min'] = 'boundInvalid';
        if (max.kind === 'invalid') errors['constraint.max'] = 'boundInvalid';
        if (min.kind === 'valid' && max.kind === 'valid' && min.value > max.value) {
          errors['constraint.min'] = 'minMax';
        }
      }
      break;
    }
  }

  // observer_cardinality × target axis: mutually exclusive (locked; the UI
  // disables the axis, this guard keeps the mirror honest).
  if (state.family === 'observer_cardinality' && state.targetEntryTypes.length > 0) {
    errors.target_entry_types = 'targetTypesConflict';
  }

  return errors;
}

/**
 * Build the AR-2 constraint carrier for the selected family. Returns `null`
 * when no family is chosen (validation rejects that state first).
 */
export function buildConstraintCarrier(state: RuleFormState): Record<string, unknown> | null {
  switch (state.family) {
    case 'module_presence':
    case 'module_absence':
      return { family: state.family, module_key: state.moduleKey.trim() };
    case 'required_field': {
      if (state.requiredFieldOperand === 'entry') {
        return { family: 'required_field', field: state.entryField };
      }
      if (state.requiredFieldOperand === 'module-row') {
        return {
          family: 'required_field',
          module_key: state.requiredModuleKey.trim(),
          field: state.requiredModuleField.trim(),
        };
      }
      return null;
    }
    case 'observer_cardinality': {
      const carrier: Record<string, unknown> = { family: 'observer_cardinality' };
      const min = parseBound(state.min);
      const max = parseBound(state.max);
      if (min.kind === 'valid') carrier.min = min.value;
      if (max.kind === 'valid') carrier.max = max.value;
      return carrier;
    }
    default:
      return null;
  }
}

/**
 * Build the `WorldRuleCreateRequest` (P1, AR-1/AR-3). Status is always sent;
 * `severity_hint` is omitted when unset (stored NULL → evaluation defaults
 * `warning`, PD-1); `target_entry_types` is omitted when empty (= all entry
 * types in check scope, AR-3). `kind` stays omitted → server default `rule`.
 */
export function buildCreateWorldRuleRequest(state: RuleFormState): WorldRuleCreateRequest | null {
  const constraint = buildConstraintCarrier(state);
  if (constraint === null) return null;
  const request: WorldRuleCreateRequest = {
    canonical_name: state.canonicalName.trim(),
    statement: state.statement.trim(),
    constraint,
    status: state.status,
  };
  if (state.severityHint !== '') request.severity_hint = state.severityHint;
  if (state.targetEntryTypes.length > 0) request.target_entry_types = [...state.targetEntryTypes];
  return request;
}

/**
 * Apply a family change: reset every per-family operand field (locked —
 * "changing family resets filled fields", no cross-family pseudo-equivalence)
 * while keeping the meta fields the author already filled. The target axis is
 * cleared when switching to `observer_cardinality` (the two are mutually
 * exclusive; a disabled-but-checked axis would be an uncorrectable state).
 */
export function withFamily(state: RuleFormState, family: RuleFamily | null): RuleFormState {
  const reset = initialRuleFormState();
  return {
    ...reset,
    family,
    canonicalName: state.canonicalName,
    statement: state.statement,
    status: state.status,
    severityHint: state.severityHint,
    targetEntryTypes: family === 'observer_cardinality' ? [] : state.targetEntryTypes,
  };
}
