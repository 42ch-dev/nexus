/**
 * P4-T3 development-only proof marker (RFT-M1 browser vertical).
 *
 * A stable, dev-only seam the real-browser proof runner mutates to measure the
 * edit→visible loop for the browser/shared-UI surface. The value is surfaced on
 * `window.__RFT_NATIVE_PROOF_MARKER__` by {@link RftNativeProofPage}; the runner
 * writes a new sampled value, waits for the running page to observe it, then
 * restores this file byte-identically.
 *
 * It exists in the shipped development bundle only (the proof page imports it),
 * carries no production behavior, and retires with the development-only mount.
 */
export const RFT_NATIVE_PROOF_MARKER = 'baseline';
