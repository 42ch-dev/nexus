/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/worlds/:world_id/kb/pack/import (V1.152 P0 DF-77). Per-atom-type summary counts plus a details list, the quarantined atoms of an import run (v1.191 P1 T10) and the bounded payload of the read-only review arm. On the review arm the counts are zero and `details` is empty; the batch's atoms are in `review`.
 */
export interface PackImportResponse {
  entries: AtomCounts;
  relations: AtomCounts1;
  /**
   * Per-atom outcome details (entry or relation). A quarantined atom is also reported here as `rejected`, so the retained counts stay complete.
   */
  details: ImportDetail[];
  /**
   * Atoms this import held outside the KB stores (v1.191 P1 T10, holder-governance.md §6): ids, reasons and original wire governance. These atoms are absent from every ordinary read, export, search and compute path until a mapping adopts them.
   */
  quarantined?: QuarantinedAtom[];
  review?: ReviewBatch;
}
/**
 * Entry-level counts.
 */
export interface AtomCounts {
  created: number;
  skipped: number;
  rejected: number;
  renamed: number;
  overwritten: number;
}
/**
 * Relation-level counts.
 */
export interface AtomCounts1 {
  created: number;
  skipped: number;
  rejected: number;
  renamed: number;
  overwritten: number;
}
/**
 * This interface was referenced by `PackImportResponse`'s JSON-Schema
 * via the `definition` "import_detail".
 */
export interface ImportDetail {
  kind: "entry" | "relation";
  id: string;
  outcome: "created" | "skipped" | "rejected" | "renamed" | "overwritten";
  /**
   * Optional human-readable reason (e.g. why rejected, skipped or quarantined).
   */
  reason?: string;
}
/**
 * This interface was referenced by `PackImportResponse`'s JSON-Schema
 * via the `definition` "quarantined_atom".
 */
export interface QuarantinedAtom {
  /**
   * Stable qrn_ id of the quarantine row.
   */
  quarantine_id: string;
  /**
   * The import batch that first held this atom; pass it to review_import.
   */
  batch_id: string;
  /**
   * The pack atom's entry_id.
   */
  entry_id: string;
  /**
   * Why the atom is held: its foreign holder id is unmapped, or its disclosure is outside the native vocabulary.
   */
  reason: "unresolved_holder" | "unknown_disclosure";
  /**
   * The atom's original wire owner, exactly as the pack carried it.
   */
  original_owner?: string | null;
  /**
   * The atom's original wire disclosure, exactly as the pack carried it.
   */
  original_disclosure?: string | null;
}
/**
 * Present on the read-only review arm: one import batch's bounded quarantined atoms with their immutable original KE JSON.
 */
export interface ReviewBatch {
  batch_id: string;
  /**
   * Whether the batch held more atoms than the review bound returns.
   */
  truncated: boolean;
  /**
   * Up to the review bound atoms, stable-id order, each with its immutable original KE JSON.
   */
  atoms: {
    quarantine_id: string;
    batch_id?: string;
    entry_id: string;
    reason: "unresolved_holder" | "unknown_disclosure";
    original_owner?: string | null;
    original_disclosure?: string | null;
    /**
     * The immutable original wire KnowledgeEntry JSON, verbatim as the pack carried it. Isolated local review only — never a knowledge view.
     */
    original_entry: {
      [k: string]: unknown | undefined;
    };
  }[];
}
/**
 * This interface was referenced by `PackImportResponse`'s JSON-Schema
 * via the `definition` "atom_counts".
 */
export interface AtomCounts2 {
  created: number;
  skipped: number;
  rejected: number;
  renamed: number;
  overwritten: number;
}
/**
 * This interface was referenced by `PackImportResponse`'s JSON-Schema
 * via the `definition` "review_batch".
 */
export interface ReviewBatch1 {
  batch_id: string;
  /**
   * Whether the batch held more atoms than the review bound returns.
   */
  truncated: boolean;
  /**
   * Up to the review bound atoms, stable-id order, each with its immutable original KE JSON.
   */
  atoms: {
    quarantine_id: string;
    batch_id?: string;
    entry_id: string;
    reason: "unresolved_holder" | "unknown_disclosure";
    original_owner?: string | null;
    original_disclosure?: string | null;
    /**
     * The immutable original wire KnowledgeEntry JSON, verbatim as the pack carried it. Isolated local review only — never a knowledge view.
     */
    original_entry: {
      [k: string]: unknown | undefined;
    };
  }[];
}
