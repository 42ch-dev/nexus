/**
 * Detect a port-conflict error from a daemon restart error message.
 *
 * R-V1130P0-QC1-W-004: the `raw.toLowerCase().includes('port') &&
 * raw.toLowerCase().includes('in use')` check was duplicated in the restart
 * handlers of {@link MainBanner} and {@link DaemonStatusBar}. Centralizing it
 * here keeps the detection logic in one place.
 *
 * The match is intentionally lenient (case-insensitive substring) because the
 * underlying message is produced by the desktop sidecar
 * (`apps/desktop/.../sidecar.rs`, "... port {n} is already in use ...") and
 * reaches the UI through several transport layers — exact-string matching would
 * be brittle. Returns `false` for an empty/blank input.
 *
 * Note: this covers the **restart-error** surface only.
 * `MainBanner.messageFor` matches a different, stricter shape
 * (`status.detail.includes('already in use')`, case-sensitive) against
 * `DaemonStatus.detail`; unifying the two is out of this fix's scope because it
 * would change `messageFor`'s matching behavior.
 */
export function isPortConflictError(raw: string): boolean {
  const lower = raw.toLowerCase();
  return lower.includes('port') && lower.includes('in use');
}
