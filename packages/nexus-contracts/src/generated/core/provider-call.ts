/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Schema-owned provider effect port envelope (probe|launch|execute|cancel|shutdown).
 */
export interface ProviderCall {
  method: "probe" | "launch" | "execute" | "cancel" | "shutdown";
  request_id: string;
  session_id?: string | null;
  operation_id?: string | null;
  deadline_ms: number;
  payload: {
    [k: string]: unknown | undefined;
  };
}
