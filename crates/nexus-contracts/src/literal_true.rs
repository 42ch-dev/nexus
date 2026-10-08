//! Serde support for schema-marked literal `true` wire fields.

use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

/// Deserialize a literal-`true` wire field, refusing any other value.
///
/// # Errors
///
/// Returns the deserializer's own error when the wire value is not a bool, and
/// a custom error when it is a bool but not the literal `true` (a field marked
/// `x-nexus-literal-true` is a wire invariant, so `false` is a hard refusal).
pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    if bool::deserialize(deserializer)? {
        Ok(true)
    } else {
        Err(D::Error::custom("expected literal true"))
    }
}

/// Serialize a literal-`true` wire field, refusing any other value.
///
/// # Errors
///
/// Returns a custom error when the value is `false`: emitting a value for a
/// literal-`true` field would produce a contract-violating payload, so the
/// refusal happens at serialization rather than at the peer.
pub fn serialize<S>(value: &bool, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *value {
        serializer.serialize_bool(true)
    } else {
        Err(S::Error::custom("expected literal true"))
    }
}
