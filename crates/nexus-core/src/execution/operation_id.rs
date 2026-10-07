//! Stable durable operation ids (v1.207 P3, spec §B.1).
//!
//! A logical write is identified by a *stable* id that is identical across
//! retries/replays of the same logical call and collision-free across
//! actor/session/action scopes. One syntax on every surface:
//!
//! ```text
//! op_<hex32>          ^op_[0-9a-f]{32}$
//! ```
//!
//! The default derivation is `SHA-256(canonical_json({actor, session,
//! action, args}))` truncated to 128 bits (`hex32`). The tuple is the whole
//! scope: a difference in any of actor/session/action/args changes the hash,
//! so two scopes never collide, while a retried call with the same scope
//! derives the same id and the receipt store dedupes it.
//!
//! A caller MAY instead supply the id at `args.operation_id`; it is used
//! verbatim after shape validation. The first-writer-wins conflict check
//! against the stored `request_fingerprint` (spec §B.1) lives with the
//! receipt store, not here.
//!
//! Explicit non-model: `HostOperationId::new()` (random-unique per attempt)
//! is stable *uniqueness*, not replay stability — receipts exist to cover
//! what that scheme cannot.

use crate::error::CoreError;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Wire prefix of a durable operation id.
pub const OPERATION_ID_PREFIX: &str = "op_";
/// Number of lowercase hex characters after [`OPERATION_ID_PREFIX`] (128 bits).
pub const OPERATION_ID_HEX_LEN: usize = 32;
/// Exact wire shape of a durable operation id (spec §B.1).
pub const OPERATION_ID_PATTERN: &str = "^op_[0-9a-f]{32}$";
/// Wire field a caller may use to supply its own id (spec §B.1).
pub const OPERATION_ID_FIELD: &str = "operation_id";

/// The scope tuple the operation id is derived from (spec §B.1).
///
/// - `actor` — the principal (`creator_id`);
/// - `session` — Connect: the authenticated peer session id; compute Run:
///   the run's session correlation;
/// - `action` — the op/tool id string;
/// - `args` — the operation arguments.
#[derive(Debug, Clone, Copy)]
pub struct OperationScope<'a> {
    /// The principal (`creator_id`).
    pub actor: &'a str,
    /// The consumer's session scope.
    pub session: &'a str,
    /// The op/tool id string.
    pub action: &'a str,
    /// The operation arguments.
    pub args: &'a Value,
}

/// Refusal from the operation-id shape/derivation layer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OperationIdError {
    /// A caller-supplied id does not match [`OPERATION_ID_PATTERN`].
    #[error("invalid operation_id {value:?}: must match op_<32 lowercase hex>")]
    Malformed {
        /// The rejected value, verbatim.
        value: String,
    },
    /// `args.operation_id` is present but is not a string.
    #[error("args.operation_id must be a string")]
    NotAString,
}

impl From<OperationIdError> for CoreError {
    /// The malformed caller id is the retained `invalid_input` (422) refusal
    /// at adapters, naming the same wire field the caller used.
    fn from(err: OperationIdError) -> Self {
        Self::InvalidInput {
            field: OPERATION_ID_FIELD.to_string(),
            reason: err.to_string(),
        }
    }
}

/// Canonicalize a JSON value: object keys are emitted in sorted order at
/// every level so `{"a":1,"b":2}` and `{"b":2,"a":1}` hash identically.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            let mut sorted = serde_json::Map::new();
            for key in keys {
                sorted.insert(key.clone(), canonicalize(&map[key]));
            }
            Value::Object(sorted)
        }
        other => other.clone(),
    }
}

/// Canonical JSON of the scope tuple `{actor, session, action, args}`.
fn canonical_operation_json(scope: &OperationScope<'_>) -> String {
    let tuple = serde_json::json!({
        "actor": scope.actor,
        "session": scope.session,
        "action": scope.action,
        "args": scope.args,
    });
    // `Value` is always string-keyed, so this serialization cannot fail.
    serde_json::to_string(&canonicalize(&tuple))
        .expect("serializing a serde_json::Value is infallible")
}

/// Validate the `op_<hex32>` shape of a durable operation id.
///
/// # Errors
///
/// Returns [`OperationIdError::Malformed`] when `id` does not match
/// [`OPERATION_ID_PATTERN`].
pub fn validate_operation_id(id: &str) -> Result<(), OperationIdError> {
    let malformed = || OperationIdError::Malformed {
        value: id.to_string(),
    };
    let Some(hex) = id.strip_prefix(OPERATION_ID_PREFIX) else {
        return Err(malformed());
    };
    if hex.len() != OPERATION_ID_HEX_LEN
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(malformed());
    }
    Ok(())
}

/// Derive the default `op_<hex32>` id for a logical call (spec §B.1).
///
/// Same logical call retried/replayed ⇒ identical tuple ⇒ identical id; any
/// scope difference ⇒ a different id.
#[must_use]
pub fn derive_operation_id(scope: &OperationScope<'_>) -> String {
    let digest = Sha256::digest(canonical_operation_json(scope).as_bytes());
    let hex = hex::encode(digest);
    format!("{OPERATION_ID_PREFIX}{}", &hex[..OPERATION_ID_HEX_LEN])
}

/// Resolve the id for a logical call: a caller-supplied `args.operation_id`
/// is used verbatim after shape validation, otherwise the default
/// derivation applies (spec §B.1).
///
/// # Errors
///
/// Returns [`OperationIdError::NotAString`] when `args.operation_id` is
/// present but not a string, and [`OperationIdError::Malformed`] when a
/// string-supplied id fails [`validate_operation_id`].
pub fn resolve_operation_id(scope: &OperationScope<'_>) -> Result<String, OperationIdError> {
    match scope.args.get(OPERATION_ID_FIELD) {
        None => Ok(derive_operation_id(scope)),
        Some(Value::String(id)) => {
            validate_operation_id(id)?;
            Ok(id.clone())
        }
        Some(_) => Err(OperationIdError::NotAString),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope<'a>(
        actor: &'a str,
        session: &'a str,
        action: &'a str,
        args: &'a Value,
    ) -> OperationScope<'a> {
        OperationScope {
            actor,
            session,
            action,
            args,
        }
    }

    #[test]
    fn derived_id_matches_wire_shape() {
        let args = json!({"path": "a.txt"});
        let id = derive_operation_id(&scope("creator-1", "session-1", "nexus.work.patch", &args));
        assert!(id.starts_with("op_"), "{id}");
        assert_eq!(id.len(), OPERATION_ID_PREFIX.len() + OPERATION_ID_HEX_LEN);
        assert!(id[3..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        validate_operation_id(&id).expect("derived id must validate");
    }

    #[test]
    fn same_logical_call_derives_same_id() {
        let args_a = json!({"path": "a.txt", "title": "T"});
        let args_b = json!({"path": "a.txt", "title": "T"});
        let first = derive_operation_id(&scope(
            "creator-1",
            "session-1",
            "nexus.work.patch",
            &args_a,
        ));
        let second = derive_operation_id(&scope(
            "creator-1",
            "session-1",
            "nexus.work.patch",
            &args_b,
        ));
        assert_eq!(
            first, second,
            "a retried logical call must derive the same id"
        );
    }

    #[test]
    fn canonicalization_ignores_argument_key_order() {
        let args_a = json!({"b": 2, "a": {"y": 1, "x": [1, 2]}});
        let args_b = json!({"a": {"x": [1, 2], "y": 1}, "b": 2});
        let first = derive_operation_id(&scope("creator-1", "session-1", "op", &args_a));
        let second = derive_operation_id(&scope("creator-1", "session-1", "op", &args_b));
        assert_eq!(first, second, "key order must not affect the derived id");
    }

    #[test]
    fn scope_differences_are_collision_free() {
        let args = json!({"k": "v"});
        let base = derive_operation_id(&scope("creator-1", "session-1", "op", &args));

        let other_actor = derive_operation_id(&scope("creator-2", "session-1", "op", &args));
        let other_session = derive_operation_id(&scope("creator-1", "session-2", "op", &args));
        let other_action = derive_operation_id(&scope("creator-1", "session-1", "op2", &args));
        let other_args =
            derive_operation_id(&scope("creator-1", "session-1", "op", &json!({"k": "w"})));

        for (label, id) in [
            ("actor", &other_actor),
            ("session", &other_session),
            ("action", &other_action),
            ("args", &other_args),
        ] {
            assert_ne!(&base, id, "{label} scope difference must change the id");
        }
    }

    #[test]
    fn caller_supplied_id_is_used_verbatim() {
        let supplied = "op_00000000000000000000000000000000";
        let args = json!({"operation_id": supplied, "k": "v"});
        let resolved = resolve_operation_id(&scope("creator-1", "session-1", "op", &args)).unwrap();
        assert_eq!(resolved, supplied);
        // ...and it is not silently re-derived.
        let derived =
            derive_operation_id(&scope("creator-1", "session-1", "op", &json!({"k": "v"})));
        assert_ne!(resolved, derived);
    }

    #[test]
    fn absent_caller_id_falls_back_to_derivation() {
        let args = json!({"k": "v"});
        let scope = scope("creator-1", "session-1", "op", &args);
        assert_eq!(
            resolve_operation_id(&scope).unwrap(),
            derive_operation_id(&scope)
        );
    }

    #[test]
    fn malformed_caller_ids_are_rejected() {
        for bad in [
            "",
            "op_",
            "op_123",
            "op_0000000000000000000000000000000",   // 31 hex
            "op_000000000000000000000000000000000", // 33 hex
            "op_0000000000000000000000000000000G",  // non-hex char
            "op_0000000000000000000000000000000A",  // uppercase hex
            "OP_00000000000000000000000000000000",  // uppercase prefix
            "00000000000000000000000000000000",     // missing prefix
        ] {
            let args = json!({"operation_id": bad});
            let err =
                resolve_operation_id(&scope("creator-1", "session-1", "op", &args)).unwrap_err();
            assert_eq!(
                err,
                OperationIdError::Malformed {
                    value: bad.to_string()
                },
                "must reject {bad:?}"
            );
        }
    }

    #[test]
    fn non_string_caller_id_is_rejected() {
        let args = json!({"operation_id": 7});
        let err = resolve_operation_id(&scope("creator-1", "session-1", "op", &args)).unwrap_err();
        assert_eq!(err, OperationIdError::NotAString);
    }

    #[test]
    fn non_object_args_derive_and_ignore_caller_field() {
        let args = json!([1, 2, 3]);
        let scope = scope("creator-1", "session-1", "op", &args);
        assert_eq!(
            resolve_operation_id(&scope).unwrap(),
            derive_operation_id(&scope)
        );
    }
}
