// Side-effect-free exit-code mapping for spawned children, shared by the
// nexus-service test runner entry point. Kept in its own module so importing
// it never runs the suite.
import { constants } from "node:os";

/** Forward the child's exit status; a signal-killed child maps to the
 * conventional 128+signal status (1 when the signal is unknown). */
export function mapChildExit(result) {
  if (result.status !== null && result.status !== undefined) return result.status;
  if (result.signal) return 128 + (constants.signals[result.signal] ?? 1);
  return 1;
}

/** Self-test fixtures for mapChildExit: [name, spawnSync-like result, expected]. */
/** @type {Array<[string, { status: number | null, signal: string | null }, number]>} */
export const CHILD_EXIT_CASES = [
  ["status 0", { status: 0, signal: null }, 0],
  ["status 3", { status: 3, signal: null }, 3],
  ["SIGTERM", { status: null, signal: "SIGTERM" }, 128 + constants.signals.SIGTERM],
  ["SIGKILL", { status: null, signal: "SIGKILL" }, 128 + constants.signals.SIGKILL],
  ["unknown signal", { status: null, signal: "SIGWONTEXIST" }, 129],
  ["no status no signal", { status: null, signal: null }, 1],
];
