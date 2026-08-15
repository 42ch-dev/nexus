/**
 * Constraint-carrier summary renderer (V1.166 P2, AR-2/AR-3).
 *
 * The rules DTO projects the AR-2 carrier first-class as `constraint` — a raw
 * typed JSON object discriminated by `family`. This pure function renders a
 * compact one-line summary from the raw object fields:
 *
 *   module_presence: belief
 *   module_absence: belief
 *   required_field: body.summary
 *   required_field: journal.tags        (module-row form: <module_key>.<field>)
 *   observer_cardinality: min 0 · max 3 (only the present bound(s) render)
 *
 * Rendering is defensive by design (the carrier is opaque on the wire and the
 * CLI validator is the only shape gate — a rule row may predate validation or
 * be hand-edited): unknown families render the family string plus generic
 * `key: value` operands, malformed carriers render no summary (`null`), and
 * odd shapes never crash. Absent `constraint` → `null` → no summary row.
 */
import type { WorldRulesListResponse } from '@42ch/nexus-contracts';

type Constraint = WorldRulesListResponse['rules'][number]['constraint'];

/** Operand joiner used across families (matches the plan's `min 0 · max 3`). */
const OPERAND_SEP = ' · ';

function isScalar(value: unknown): value is string | number | boolean {
  return typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean';
}

/**
 * Generic fallback for unknown families / malformed known families: the
 * family string plus every other scalar operand as `key: value`, sorted for
 * deterministic output. Non-scalar members (objects/arrays/null) are skipped
 * — they cannot be summarized as a one-liner.
 */
function genericSummary(family: string, constraint: Record<string, unknown>): string {
  const operands = Object.entries(constraint)
    .filter(([key, value]) => key !== 'family' && isScalar(value))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([key, value]) => `${key}: ${String(value)}`);
  return operands.length > 0 ? `${family}: ${operands.join(OPERAND_SEP)}` : family;
}

/**
 * Render the one-line constraint summary, or `null` when the carrier is
 * absent/malformed (no summary row). Never throws on odd shapes.
 */
export function renderConstraintSummary(constraint: Constraint): string | null {
  if (!constraint || typeof constraint !== 'object') return null;
  const family = constraint.family;
  if (typeof family !== 'string' || family.length === 0) return null;

  const moduleKey = constraint.module_key;
  const field = constraint.field;
  const min = constraint.min;
  const max = constraint.max;

  switch (family) {
    case 'module_presence':
    case 'module_absence':
      if (typeof moduleKey === 'string' && moduleKey.length > 0) {
        return `${family}: ${moduleKey}`;
      }
      return genericSummary(family, constraint);
    case 'required_field': {
      if (typeof moduleKey === 'string' && moduleKey.length > 0) {
        // Module-row form: <module_key>.<field> (field renders if present,
        // otherwise the module row alone is still honest partial info).
        return typeof field === 'string' && field.length > 0
          ? `${family}: ${moduleKey}.${field}`
          : `${family}: ${moduleKey}`;
      }
      if (typeof field === 'string' && field.length > 0) {
        return `${family}: ${field}`;
      }
      return genericSummary(family, constraint);
    }
    case 'observer_cardinality': {
      const bounds: string[] = [];
      if (typeof min === 'number' && Number.isFinite(min)) bounds.push(`min ${min}`);
      if (typeof max === 'number' && Number.isFinite(max)) bounds.push(`max ${max}`);
      if (bounds.length > 0) return `${family}: ${bounds.join(OPERAND_SEP)}`;
      return genericSummary(family, constraint);
    }
    default:
      // Unknown family — render the family string + generic operands. The
      // spoke carrier is a closed set this iteration, but rows written by
      // future tooling must not crash the section (defensive contract).
      return genericSummary(family, constraint);
  }
}
