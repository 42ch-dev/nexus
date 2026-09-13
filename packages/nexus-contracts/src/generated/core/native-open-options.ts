/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Native core open options. allow_uninitialized is service-only and denies domain/provider effects until explicit reopen.
 */
export interface NativeOpenOptions {
  /**
   * Raw user home directory; home-layout adds .nexus42 exactly once.
   */
  user_home: string;
  access: "read_only" | "direct_writer" | "engine_owner";
  /**
   * Service-only: open with readiness uninitialized, no DB/Host/engine owner; effects denied until reopen.
   */
  allow_uninitialized?: boolean;
}
