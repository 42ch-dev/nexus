/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Durable workspace commit result: revision identifier for the commit and whether it was accepted (always true for successful commits).
 */
export interface CoreWorkspaceCommitResponse {
  /**
   * Revision identifier for this commit.
   */
  revision: string;
  committed: boolean;
}
