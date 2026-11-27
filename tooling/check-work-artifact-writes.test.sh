#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-work-artifact-writes.test.sh — fixture checks for the Work artifact
# byte-write guard's `cfg` classification (I-000408).
#
# Each fixture is scanned through the guard's fixture mode. Only predicates
# that require `test` compilation cut a span (`cfg(test)`,
# `cfg(all(…, test))`; a trailing comma does not change them), and only for an
# item whose `{ … }` body is matchable (a `mod … { }` or a macro invocation,
# including `path::mac!`). `cfg(any(test, …))`, `cfg(not(test))`, a field
# annotation and an out-of-line `mod tests;` declaration stay scanned; a body
# the tracker cannot delimit fails closed.
#
# Usage (from repository root): bash tooling/check-work-artifact-writes.test.sh
# Exit codes: 0 — every fixture matched its expected result; 1 — mismatch.
# ---------------------------------------------------------------------------
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GUARD="$ROOT/tooling/check-work-artifact-writes.sh"
FIXTURES="$ROOT/tooling/fixtures/work-artifact-write-cfg"
failures=0

# expect <fixture> <exit> [required diagnostic text]
expect() {
    local fixture="$1" want="$2" want_msg="${3:-}" out got
    out="$(bash "$GUARD" "$FIXTURES/$fixture" 2>&1)"
    got=$?
    if [ "$got" -ne "$want" ]; then
        printf 'FAIL %s: exit %s, want %s\n%s\n' "$fixture" "$got" "$want" "$out"
        failures=$((failures + 1))
        return
    fi
    if [ -n "$want_msg" ]; then
        case "$out" in
            *"$want_msg"*) ;;
            *)
                printf 'FAIL %s: missing diagnostic %s\n%s\n' "$fixture" "$want_msg" "$out"
                failures=$((failures + 1))
                return
                ;;
        esac
    fi
    printf 'ok   %s: exit %s\n' "$fixture" "$got"
}

WRITE_MSG="work artifact direct write"
OPAQUE_MSG="test-gated attribute on an opaque body"

expect cfg_test.rs 0
expect cfg_all_feature_test.rs 0
expect cfg_any_test_trailing_comma.rs 0
expect cfg_any_test_wasm.rs 1 "$WRITE_MSG"
expect cfg_not_test.rs 1 "$WRITE_MSG"
expect cfg_any_test_field.rs 1 "$WRITE_MSG"
expect mod_decl_no_span.rs 1 "$WRITE_MSG"
expect macro_path_attr.rs 0
expect opaque_macro_attr.rs 1 "$OPAQUE_MSG"
expect opaque_block_attr.rs 1 "$OPAQUE_MSG"

if [ "$failures" -ne 0 ]; then
    echo "cfg fixture checks FAILED: $failures fixture(s) mismatched"
    exit 1
fi
echo "cfg fixture checks OK"
