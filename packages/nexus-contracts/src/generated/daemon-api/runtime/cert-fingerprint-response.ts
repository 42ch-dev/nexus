/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/runtime/cert-fingerprint. Returns the SHA-256 fingerprint of the daemon's TLS certificate for TOFU pinning. No authentication required. When the daemon is loopback-only and has no TLS cert, the fingerprint field is an empty string and created_at is absent.
 */
export interface CertFingerprintResponse {
  /**
   * SHA-256 fingerprint of the DER-encoded TLS certificate, colon-hex format (lowercase) with 'SHA256:' prefix. Example: 'SHA256:aa:bb:cc:dd:ee:ff:...'. Empty string when the daemon has no TLS cert (loopback-only mode).
   */
  fingerprint: string;
  /**
   * Hash algorithm used for the fingerprint. Always 'sha256' for the initial implementation.
   */
  algorithm: "sha256";
  /**
   * ISO 8601 timestamp of when the TLS certificate was generated or loaded at boot. Optional — present when the daemon has a cert; absent when loopback-only.
   */
  created_at?: string;
}
