import { isAbsolute } from 'node:path';
import type { ValidatedProviderRecipe } from '@42ch/nexus-contracts';

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((entry) => typeof entry === 'string');
}

function isStringRecord(value: unknown): value is Record<string, string> {
  if (typeof value !== 'object' || value === null) return false;
  return Object.values(value).every((entry) => typeof entry === 'string');
}

/** Require the exact nested Rust-admitted recipe; reject payload cwd overrides. */
export function parseAdmittedRecipe(payload: Record<string, unknown>): ValidatedProviderRecipe {
  const recipe = payload.recipe;
  if (typeof recipe !== 'object' || recipe === null) {
    throw new Error('invalid_recipe');
  }
  const record = recipe as Record<string, unknown>;
  if (
    !isNonEmptyString(record.provider_id) ||
    !isNonEmptyString(record.recipe_generation) ||
    !isNonEmptyString(record.executable) ||
    !isAbsolute(record.executable) ||
    !isStringArray(record.args) ||
    !isNonEmptyString(record.cwd) ||
    !isStringRecord(record.env)
  ) {
    throw new Error('invalid_recipe');
  }
  return {
    provider_id: record.provider_id,
    recipe_generation: record.recipe_generation,
    executable: record.executable,
    args: record.args,
    env: record.env,
    cwd: record.cwd,
    permissions_ref:
      record.permissions_ref === null || typeof record.permissions_ref === 'string'
        ? (record.permissions_ref ?? null)
        : null,
    config_ref:
      record.config_ref === null || typeof record.config_ref === 'string'
        ? (record.config_ref ?? null)
        : null,
    process_identity:
      typeof record.process_identity === 'object' && record.process_identity !== null
        ? (record.process_identity as ValidatedProviderRecipe['process_identity'])
        : null,
  };
}
