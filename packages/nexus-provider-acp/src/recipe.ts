import { isAbsolute, resolve } from 'node:path';
import { statSync } from 'node:fs';
import type { ValidatedProviderRecipe } from '@42ch/nexus-contracts';
import { parseProcessIdentity } from './identity.js';

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

function validateCanonicalCwd(cwd: string): string {
  if (!isAbsolute(cwd)) throw new Error('invalid_recipe');
  const normalized = resolve(cwd);
  if (normalized !== cwd) throw new Error('invalid_recipe');
  try {
    const stat = statSync(normalized);
    if (!stat.isDirectory()) throw new Error('invalid_recipe');
  } catch {
    throw new Error('invalid_recipe');
  }
  return normalized;
}

/**
 * Require the exact nested Rust-admitted recipe; reject payload cwd overrides.
 * `process_identity` null means observe-at-spawn; non-null is the admitted
 * resulting identity that must match the spawned child.
 */
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
  const cwd = validateCanonicalCwd(record.cwd);
  const processIdentity = parseProcessIdentity(record.process_identity);
  return {
    provider_id: record.provider_id,
    recipe_generation: record.recipe_generation,
    executable: record.executable,
    args: record.args,
    env: record.env,
    cwd,
    permissions_ref:
      record.permissions_ref === null || typeof record.permissions_ref === 'string'
        ? (record.permissions_ref ?? null)
        : null,
    config_ref:
      record.config_ref === null || typeof record.config_ref === 'string'
        ? (record.config_ref ?? null)
        : null,
    process_identity: processIdentity,
  };
}
