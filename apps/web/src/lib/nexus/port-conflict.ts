/**
 * Detect a port-conflict error from a daemon restart error message.
 *
 * R-V1130P0-QC1-W-004: the `raw.toLowerCase().includes('port') &&
 * raw.toLowerCase().includes('in use')` check was duplicated in the restart
 * handlers of the daemon status surfaces. Centralizing it
 * here keeps the detection logic in one place.
 *
 * The match is intentionally lenient (case-insensitive substring) because the
 * underlying message ("... port {n} is already in use ...") is produced by the
 * desktop host and reaches the UI through several transport layers —
 * exact-string matching would be brittle. Returns `false` for an empty/blank
 * input.
 *
 * Note: this covers the **restart-error** surface only.
 * `DaemonStatus.detail` matching remains a separate, stricter shape; unifying
 * the two is out of this fix's scope because it
 * would change `messageFor`'s matching behavior.
 */
export function isPortConflictError(raw: string): boolean {
  const lower = raw.toLowerCase();
  return lower.includes('port') && lower.includes('in use');
}
