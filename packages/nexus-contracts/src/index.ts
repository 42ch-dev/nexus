/**
 * Nexus Wire Contracts (Generated from JSON Schema)
 *
 * This package contains TypeScript type definitions generated from `schemas/` JSON Schema files.
 * All wire types are auto-generated - do not modify manually.
 */

// Re-export all generated types
export * from './generated';

/**
 * Runtime mode controlling platform dependency behavior.
 * See schemas/domain/runtime-mode.schema.json for authoritative definition.
 * ADR-015 (local_first / cloud_enhanced), ADR-017 (local_only).
 */
export type RuntimeMode = 'local_only' | 'local_first' | 'cloud_enhanced';
