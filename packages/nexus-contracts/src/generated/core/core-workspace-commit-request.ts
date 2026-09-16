/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Workspace session commit request. Wire field spelling is camelCase, preserved from the existing workspace.commit surface. The changes manifest is validated against the session snapshot (OCC).
 */
export interface CoreWorkspaceCommitRequest {
  /**
   * Session id returned by workspace open; must not be empty.
   */
  sessionId: string;
  /**
   * Manifest of changes to commit, each with path, op, optional pre-image hash and optional base64 content.
   */
  changes?: {
    /**
     * Scope-relative path for this change.
     */
    path: string;
    op: "create" | "modify" | "delete";
    /**
     * Lowercase SHA-256 of the pre-image; omitted for create.
     */
    expectedHash?: string;
    /**
     * Canonical base64 content for create/modify; forbidden for delete.
     */
    contentBase64?: string;
  }[];
}
