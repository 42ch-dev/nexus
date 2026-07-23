/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Platform conflict response for bundle push operations. HTTP 200 with success:false indicates a conflict requiring resolution. See hard-vs-soft-validation-v1.md §7.
 */
export interface ConflictResponse {
  /**
   * Always false for conflict responses.
   */
  success: false;
  /**
   * Category of conflict.
   */
  conflict_type: "version_mismatch" | "sequence_conflict" | "hard_validation_failure" | "soft_validation_warning";
  /**
   * Array of individual conflict details.
   *
   * @minItems 1
   */
  conflicts: [
    {
      /**
       * Machine-readable conflict code (e.g., 'revision_outdated', 'delta_sequence_gap').
       */
      code: string;
      /**
       * Human-readable conflict description.
       */
      message: string;
      /**
       * Index into the conflicting deltas[] array, if applicable.
       */
      delta_index?: number;
      /**
       * Expected value that caused the conflict.
       */
      expected?: {
        [k: string]: unknown | undefined;
      };
      /**
       * Actual value received.
       */
      actual?: {
        [k: string]: unknown | undefined;
      };
      /**
       * Suggested resolution strategy.
       */
      resolution_hint?: "auto_accept" | "auto_reject" | "manual_review";
      [k: string]: unknown | undefined;
    },
    ...{
      /**
       * Machine-readable conflict code (e.g., 'revision_outdated', 'delta_sequence_gap').
       */
      code: string;
      /**
       * Human-readable conflict description.
       */
      message: string;
      /**
       * Index into the conflicting deltas[] array, if applicable.
       */
      delta_index?: number;
      /**
       * Expected value that caused the conflict.
       */
      expected?: {
        [k: string]: unknown | undefined;
      };
      /**
       * Actual value received.
       */
      actual?: {
        [k: string]: unknown | undefined;
      };
      /**
       * Suggested resolution strategy.
       */
      resolution_hint?: "auto_accept" | "auto_reject" | "manual_review";
      [k: string]: unknown | undefined;
    }[]
  ];
  /**
   * Current world revision on the server.
   */
  server_world_revision: number;
  /**
   * Current confirmed delta sequence on the server.
   */
  server_delta_sequence?: number;
  /**
   * Suggested retry delay in seconds, if applicable.
   */
  retry_after?: number | null;
}
