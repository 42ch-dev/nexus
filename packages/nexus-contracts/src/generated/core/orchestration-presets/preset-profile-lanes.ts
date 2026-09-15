/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Trigger-lane classification: the four locked PL-3 lanes as a flat wire mirror the CLI `preset show --json` output matches verbatim (AR-25).
 */
export interface PresetProfileLanes {
  cron: boolean;
  wallClock: boolean;
  session: boolean;
  direct: boolean;
}
