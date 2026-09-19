/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/characters/:character_id/knowledge/:entry_id. Only canonical_name, summary and audience are mutable; null removes summary, omission keeps it. Governance moves under the same expected_revision CAS: an omitted audience preserves the stored holder/disclosure pair, and an explicit "shared" clears both. The legacy `creator_only` key is not a member of this schema and its presence is rejected (including `false`), never ignored.
 */
export interface UpdateKnowledgeEntryRequest {
  expected_revision: number;
  canonical_name?: string;
  /**
   * Plain UTF-8 summary stored in body.summary. Null removes the summary member; omission keeps it.
   */
  summary?: string | null;
  /**
   * Closed author audience for holder governance. Omission preserves the stored governance columns; explicit "shared" clears both. A private audience resolves against the admitted identity: the author never supplies a holder id or a management flag.
   */
  audience?: SharedKnowledgeAudience | AuthorOnlyKnowledgeAudience | CharacterPrivateKnowledgeAudience;
}
export interface SharedKnowledgeAudience {
  /**
   * In-scope shared knowledge: clears holder and disclosure.
   */
  kind: "shared";
}
export interface AuthorOnlyKnowledgeAudience {
  /**
   * Resolves to the admitted controlling Creator's own holder with disclosure owner-private.
   */
  kind: "author-only";
}
export interface CharacterPrivateKnowledgeAudience {
  /**
   * Resolves to a Character this Creator owns; for a World-owned row that Character must be bound to that World.
   */
  kind: "character-private";
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
}
