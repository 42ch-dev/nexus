#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-module-sdk-drift.sh — Compute-module SDK drift gate (V1.170 P0, AR-7)
#
# Two gates (both python3 — python3 is already used in ci.yml):
#
#   1. Mirror-gap: parses the compute input/output wire schemas' `properties`
#      keys (top level + the nested typed parts `world_ref` and
#      `state_delta[]`) and greps the SDK's typed envelope structs
#      (`ComputeInput` / `ComputeOutput` / `WorldRef` / `StateDeltaOp` in
#      modules/nexus-module-sdk/src/types.rs), failing when a wire field has
#      no SDK counterpart. The state_delta `op` enum values (add/sub/set)
#      must likewise have `DeltaOp` variants.
#   2. Fixture-parity: the canonical mini-host fixture
#      (modules/nexus-module-test/fixtures/combat-input.json, the AR-10 SSOT)
#      must be value-identical to the inline `combat_input()` JSON in
#      crates/nexus-wasm-host/tests/basic_combat.rs.
#
# The third drift-guard layer is behavioral: the `module-dx` CI job (AR-12)
# compiles basic-combat against the current SDK and re-runs the real-host
# fixtures — the wasm round-trip is the ground truth.
#
# Exit codes:
#   0 — All wire fields mirrored in the SDK AND fixtures are value-identical
#   1 — Drift detected
# ---------------------------------------------------------------------------
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> 1/2 Envelope mirror-gap: wire schemas vs SDK typed structs..."
python3 - <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path(".")
sdk_types = (root / "modules/nexus-module-sdk/src/types.rs").read_text()

INPUT_SCHEMA = root / "schemas/daemon-api/compute/compute-input.schema.json"
OUTPUT_SCHEMA = root / "schemas/daemon-api/compute/compute-output.schema.json"


def struct_fields(struct_name: str) -> set[str]:
    """`pub <field>:` names inside the named struct's braces."""
    m = re.search(r"pub struct %s\s*\{(.*?)\n\}" % re.escape(struct_name), sdk_types, re.S)
    if not m:
        print(f"FAIL: struct `{struct_name}` not found in nexus-module-sdk/src/types.rs")
        sys.exit(1)
    return set(re.findall(r"pub\s+(\w+)\s*:", m.group(1)))


def check(label: str, wire_fields: set[str], struct_name: str) -> None:
    missing = sorted(wire_fields - struct_fields(struct_name))
    if missing:
        print(
            f"FAIL: {label}: wire field(s) with no SDK `{struct_name}` counterpart: "
            + ", ".join(missing)
        )
        sys.exit(1)
    print(f"OK: {label}: {len(wire_fields)} wire field(s) all mirrored in `{struct_name}`")


input_schema = json.loads(INPUT_SCHEMA.read_text())
output_schema = json.loads(OUTPUT_SCHEMA.read_text())

check("compute-input top-level", set(input_schema["properties"]), "ComputeInput")
check("compute-output top-level", set(output_schema["properties"]), "ComputeOutput")
check(
    "compute-input.world_ref",
    set(input_schema["properties"]["world_ref"]["properties"]),
    "WorldRef",
)
state_delta_items = output_schema["properties"]["state_delta"]["items"]["properties"]
check("compute-output.state_delta[]", set(state_delta_items), "StateDeltaOp")

# Delta-op enum mirror: every wire op value must have a `DeltaOp` variant
# (serde `rename_all = "lowercase"` maps Add|Sub|Set -> add|sub|set).
wire_ops = set(state_delta_items["op"]["enum"])
enum_body = re.search(r"pub enum DeltaOp\s*\{(.*?)\n\}", sdk_types, re.S)
if not enum_body:
    print("FAIL: `DeltaOp` enum not found in types-module")
    sys.exit(1)
sdk_ops = set(re.findall(r"\b(Add|Sub|Set)\b", enum_body.group(1)))
# Each wire value's capitalized variant must exist (Add|Sub|Set -> add|sub|set).
missing_ops = sorted(op for op in wire_ops if op.capitalize() not in sdk_ops)
if missing_ops:
    print("FAIL: wire op enum value(s) with no `DeltaOp` variant: " + ", ".join(missing_ops))
    sys.exit(1)
print(f"OK: compute-output.state_delta[].op: {len(wire_ops)} wire op(s) mirrored in `DeltaOp`")
PY

echo ""
echo "==> 2/2 Fixture parity: mini-host fixture vs basic_combat.rs inline JSON"
python3 - <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path(".")
fixture_path = root / "modules/nexus-module-test/fixtures/combat-input.json"
test_path = root / "crates/nexus-wasm-host/tests/basic_combat.rs"

fixture = json.loads(fixture_path.read_text())
test_src = test_path.read_text()
m = re.search(r'let raw = r#"(.*?)"#;', test_src, re.S)
if not m:
    print("FAIL: could not extract the inline `combat_input()` JSON from basic_combat.rs")
    sys.exit(1)
inline = json.loads(m.group(1))

if fixture != inline:
    print(
        "FAIL: fixture parity drift — modules/nexus-module-test/fixtures/combat-input.json "
        "is not value-identical to the inline JSON in crates/nexus-wasm-host/tests/basic_combat.rs"
    )
    sys.exit(1)
print("OK: fixtures/combat-input.json is value-identical to the basic_combat.rs inline JSON")
PY

echo "✅ All module SDK drift checks passed."
