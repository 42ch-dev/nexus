#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-work-artifact-writes.test.sh — fixture checks for the Work artifact
# byte-write guard's `cfg` classification (I-000408).
#
# Each fixture is scanned through the guard's fixture mode. Test-gated modules
# (`cfg(test)`, `cfg(all(…, test))`, `cfg(any(test, …))`) are excluded;
# `cfg(not(test))` stays production, so its direct write must be reported.
#
# Usage (from repository root): bash tooling/check-work-artifact-writes.test.sh
# Exit codes: 0 — every fixture matched its expected result; 1 — mismatch.
# ---------------------------------------------------------------------------
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GUARD="$ROOT/tooling/check-work-artifact-writes.sh"
FIXTURES="$ROOT/tooling/fixtures/work-artifact-write-cfg"
failures=0

expect() {
    local fixture="$1" want="$2" out got
    out="$(bash "$GUARD" "$FIXTURES/$fixture" 2>&1)"
    got=$?
    if [ "$got" -ne "$want" ]; then
        printf 'FAIL %s: exit %s, want %s\n%s\n' "$fixture" "$got" "$want" "$out"
        failures=$((failures + 1))
        return
    fi
    printf 'ok   %s: exit %s\n' "$fixture" "$got"
}

expect cfg_test.rs 0
expect cfg_all_feature_test.rs 0
expect cfg_any_test_feature.rs 0
expect cfg_not_test.rs 1

if [ "$failures" -ne 0 ]; then
    echo "cfg fixture checks FAILED: $failures fixture(s) mismatched"
    exit 1
fi
echo "cfg fixture checks OK"
