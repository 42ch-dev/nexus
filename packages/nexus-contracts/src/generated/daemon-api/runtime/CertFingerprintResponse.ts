import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CertFingerprintResponse
 *
 * Response for GET /v1/daemon/runtime/cert-fingerprint. Returns the SHA-256 fingerprint of the daemon's TLS certificate for TOFU pinning. No authentication required. When the daemon is loopback-only and has no TLS cert, the fingerprint field is an empty string and created_at is absent.
 *
 * @schema_version 1
 * @source cert-fingerprint-response.schema.json
 */

/** Inline enum type */
export type CertFingerprintResponseAlgorithm = 'sha256';

/** Response for GET /v1/daemon/runtime/cert-fingerprint. Returns the SHA-256 fingerprint of the daemon's TLS certificate for TOFU pinning. No authentication required. When the daemon is loopback-only and has no TLS cert, the fingerprint field is an empty string and created_at is absent. */
export interface CertFingerprintResponse {
  fingerprint: string;
  algorithm: CertFingerprintResponseAlgorithm;
  created_at?: string;
}
