//! Q3 canonical content hash: the one approval identity for a Workflow IR document.
//!
//! Inputs are length-prefixed so no member boundary can be forged by concatenation:
//! the canonical IR (top-level `content_hash` excluded, object keys sorted recursively,
//! compact UTF-8 JSON), then the sorted resolved asset hash set, then the sorted resolved
//! nest-child hash set. Nested members are hashed through the document body itself, so an
//! expanded nest needs no extra input; a `defer_until_apply` child is unresolved by contract
//! and is excluded until Apply records its hash.

use crate::{WorkflowDocument, WorkflowNestBindingExpansion};
use serde_json::Value;

/// A document member is missing the resolved hash the identity requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalHashError {
    /// The document could not be serialized to canonical JSON.
    Serialize(String),
    /// A resolved member carries no content hash, so the identity would be incomplete.
    MissingMemberHash(String),
}

impl std::fmt::Display for CanonicalHashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(reason) => write!(f, "canonical IR is not serializable: {reason}"),
            Self::MissingMemberHash(member) => {
                write!(f, "resolved member `{member}` carries no content hash")
            }
        }
    }
}

impl std::error::Error for CanonicalHashError {}

/// Compute the Q3 canonical content hash of `document`.
///
/// # Errors
///
/// Returns [`CanonicalHashError`] when the document cannot be canonicalized or a resolved
/// asset/nest child has no recorded hash. An unresolved hash is never silently skipped.
pub fn content_hash(document: &WorkflowDocument) -> Result<String, CanonicalHashError> {
    let mut body = serde_json::to_value(document)
        .map_err(|error| CanonicalHashError::Serialize(error.to_string()))?;
    // The hash never covers itself: the candidate's optional `content_hash` is not identity.
    if let Value::Object(members) = &mut body {
        members.remove("content_hash");
    }
    let canonical = serde_json::to_string(&sort_keys(&body))
        .map_err(|error| CanonicalHashError::Serialize(error.to_string()))?;

    let assets = resolved_hashes(document.assets.iter().map(|asset| {
        (
            format!("assets[{}]", asset.id),
            asset.content_hash.as_deref(),
        )
    }))?;
    let nests = resolved_hashes(
        document
            .nests
            .iter()
            .filter(|nest| nest.expansion == WorkflowNestBindingExpansion::ExpandInCandidate)
            .map(|nest| {
                (
                    format!("nests[{}]", nest.id),
                    nest.child.content_hash.as_deref(),
                )
            }),
    )?;

    let mut hasher = blake3::Hasher::new();
    feed(&mut hasher, b"ir", canonical.as_bytes());
    feed(&mut hasher, b"assets", assets.join("\n").as_bytes());
    feed(&mut hasher, b"nests", nests.join("\n").as_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

fn feed(hasher: &mut blake3::Hasher, tag: &[u8], bytes: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Sorted member hashes; a resolved member without one is an error, not an omission.
fn resolved_hashes<'a>(
    members: impl Iterator<Item = (String, Option<&'a str>)>,
) -> Result<Vec<&'a str>, CanonicalHashError> {
    let mut hashes = members
        .map(|(member, hash)| hash.ok_or(CanonicalHashError::MissingMemberHash(member)))
        .collect::<Result<Vec<_>, _>>()?;
    hashes.sort_unstable();
    Ok(hashes)
}

/// Rebuild every object with ascending keys so serialization order is content, not history.
fn sort_keys(value: &Value) -> Value {
    match value {
        Value::Object(members) => {
            let mut entries: Vec<_> = members.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, member)| (key.clone(), sort_keys(member)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_keys).collect()),
        scalar => scalar.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
        "ir_version": 0,
        "id": "probe",
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "hash probe",
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "a",
        "terminal": ["b"],
        "nodes": [
            {"id": "a", "kind": "outer_state", "effect_class": "read", "exit_when": {"kind": "rule"}},
            {"id": "b", "kind": "terminal", "effect_class": "pure"}
        ],
        "edges": [{"id": "a->b", "from": "a", "to": "b", "kind": "linear"}],
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "read",
        "provenance": {"source": "compose"}
    }"#;

    fn document(mut edit: impl FnMut(&mut Value)) -> WorkflowDocument {
        let mut value: Value = serde_json::from_str(MINIMAL).expect("fixture parses");
        edit(&mut value);
        serde_json::from_value(value).expect("edited fixture is a document")
    }

    fn hash_of(document: &WorkflowDocument) -> String {
        content_hash(document).expect("canonical hash")
    }

    #[test]
    fn object_key_order_does_not_change_the_hash() {
        let ascending = r#"{"description": "d", "kind": "creator", "requires_capabilities": [], "run_intents": []}"#;
        let descending = r#"{"run_intents": [], "requires_capabilities": [], "kind": "creator", "description": "d"}"#;
        let first = document(|value| {
            value["meta"] = serde_json::from_str(ascending).expect("meta");
        });
        let second = document(|value| {
            value["meta"] = serde_json::from_str(descending).expect("meta");
        });
        assert_eq!(hash_of(&first), hash_of(&second));
    }

    #[test]
    fn the_content_hash_member_is_excluded_from_its_own_input() {
        let plain = document(|_| {});
        let declared = document(|value| value["content_hash"] = Value::from("stale"));
        assert_eq!(hash_of(&plain), hash_of(&declared));
    }

    #[test]
    fn resolved_asset_hashes_participate_and_a_missing_one_is_rejected() {
        let hashed = document(|value| {
            value["assets"] = serde_json::from_str(
                r#"[{"id": "templates/a.md", "path": "templates/a.md", "kind": "template", "content_hash": "aa"}]"#,
            )
            .expect("assets");
        });
        let unresolved = document(|value| {
            value["assets"] = serde_json::from_str(
                r#"[{"id": "templates/a.md", "path": "templates/a.md", "kind": "template"}]"#,
            )
            .expect("assets");
        });
        assert_ne!(hash_of(&hashed), hash_of(&document(|_| {})));
        assert_eq!(
            content_hash(&unresolved),
            Err(CanonicalHashError::MissingMemberHash(
                "assets[templates/a.md]".into()
            ))
        );
    }

    #[test]
    fn expanded_nest_child_hash_is_required_and_deferred_children_are_excluded() {
        let nest = |expansion: &str, resolved: bool| {
            format!(
                r#"[{{"id": "n1", "parent_node": "a", "expansion": "{expansion}", "pin_policy": "exact",
                    "child": {{"kind": "pattern", "registry_id": "r", "version": "1"{}}}}}]"#,
                if resolved {
                    r#", "content_hash": "bb""#
                } else {
                    ""
                }
            )
        };

        let expanded = document(|value| {
            value["nests"] =
                serde_json::from_str(&nest("expand_in_candidate", true)).expect("nest");
        });
        let missing = document(|value| {
            value["nests"] =
                serde_json::from_str(&nest("expand_in_candidate", false)).expect("nest");
        });
        let deferred = document(|value| {
            value["nests"] = serde_json::from_str(&nest("defer_until_apply", false)).expect("nest");
        });

        assert_ne!(hash_of(&expanded), hash_of(&deferred));
        assert_eq!(
            content_hash(&missing),
            Err(CanonicalHashError::MissingMemberHash("nests[n1]".into()))
        );
        // A deferred child is unresolved by contract: Apply records its hash later.
        assert!(content_hash(&deferred).is_ok());
    }
}
