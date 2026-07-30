//! V1.145 — spoke 0.6.0 `Scope.extensions` round-trip smoke test.
//!
//! Proves the spoke-native `Scope.extensions` mechanism (added in spoke 0.6.0)
//! carries the P2 typed-carrier fields (`text_search` / `limit` / `offset` /
//! `canonical_name` / `computable`) under the `"nexus"` namespace and survives a
//! serde round-trip. This is the mechanism the P2 redo will move onto once the
//! typed-carrier workaround is retired: the fields go into
//! `scope.extensions["nexus"]` instead of a nexus-local carrier struct.
//!
//! It also locks in two 0.6.0 properties the rest of the crate relies on:
//!  - the typify `ScopeExtensionsKey` newtype exists and constructs the
//!    `"nexus"` key via `try_from` (mirrors `KnowledgeEntryExtensionsKey`);
//!  - `extensions` is `#[serde(default)]`, so pre-0.6.0 minimal scopes
//!    (`scope_id` only, like `scope_query_port::scope_for`) still deserialize.

use nexus_spoke_adapter::{Scope, ScopeExtensionsKey};
use serde_json::{json, Value};

#[test]
fn scope_extensions_nexus_namespace_round_trips() {
    // Wire construction — mirrors how the daemon serializes a scope for the
    // spoke orchestrator. Populate the `"nexus"` namespace with the P2
    // typed-carrier fields.
    let wire = json!({
        "scope_id": "wld_mira",
        "extensions": {
            "nexus": {
                "text_search": "harbor",
                "limit": 10,
                "offset": 0,
                "canonical_name": "Battle of Harbor",
                "computable": true
            }
        }
    });

    let scope: Scope =
        serde_json::from_value(wire).expect("Scope with extensions.nexus deserializes");

    // The typed-key path: the typify `ScopeExtensionsKey` newtype is the only
    // way to look up the namespace (it does not impl `Borrow<str>`, mirroring
    // `KnowledgeEntryExtensionsKey`). Constructing it via `try_from` proves the
    // 0.6.0 newtype name + the "nexus" key the P2 redo depends on.
    let nexus_key = ScopeExtensionsKey::try_from("nexus").expect("valid namespace key");
    let nexus_ns = scope
        .extensions
        .get(&nexus_key)
        .expect("nexus namespace carried on Scope");

    assert_eq!(nexus_ns.get("text_search"), Some(&Value::from("harbor")));
    assert_eq!(nexus_ns.get("limit"), Some(&Value::from(10)));
    assert_eq!(nexus_ns.get("offset"), Some(&Value::from(0)));
    assert_eq!(
        nexus_ns.get("canonical_name"),
        Some(&Value::from("Battle of Harbor"))
    );
    assert_eq!(nexus_ns.get("computable"), Some(&Value::from(true)));

    // Serde round-trip preserves the namespace verbatim.
    let back = serde_json::to_value(&scope).expect("Scope serializes");
    let round_tripped: Scope = serde_json::from_value(back.clone()).expect("Scope re-deserializes");
    assert_eq!(
        round_tripped.extensions.get(&nexus_key),
        scope.extensions.get(&nexus_key),
        "extensions.nexus survives serialize -> deserialize"
    );
    // The wire still names the nexus namespace.
    assert_eq!(
        back["extensions"]["nexus"]["text_search"],
        Value::from("harbor")
    );
}

#[test]
fn minimal_scope_without_extensions_still_deserializes() {
    // `extensions` is `#[serde(default)]` — pre-0.6.0 minimal scopes (`scope_id`
    // only, like `scope_query_port::scope_for`) stay valid. This guards the
    // backward-compat property the existing serde-based construction relies on.
    let scope: Scope = serde_json::from_value(json!({ "scope_id": "wld_any" }))
        .expect("minimal scope without extensions deserializes");
    assert!(scope.extensions.is_empty());
    assert_eq!(scope.scope_id, "wld_any");
}
