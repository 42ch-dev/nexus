//! `Nexus` `WorldKbRelationshipKind`
//!
//! `Core` taxonomy values for `World` `KB` typed relationships (`V1`.74). `Use` `custom` with a non-empty `custom_label` for out-of-enum narrative relationships.
//!
//! `@schema_version` 1
//! `@source` world-kb-relationship-kind.schema.json

use serde::{Deserialize, Serialize};

/// `Core` taxonomy values for `World` `KB` typed relationships (`V1`.74). `Use` `custom` with a non-empty `custom_label` for out-of-enum narrative relationships.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorldKbRelationshipKind {
    #[default]
    #[serde(rename = "allied_with")]
    AlliedWith,
    #[serde(rename = "opposes")]
    Opposes,
    #[serde(rename = "parent_of")]
    ParentOf,
    #[serde(rename = "child_of")]
    ChildOf,
    #[serde(rename = "member_of")]
    MemberOf,
    #[serde(rename = "located_in")]
    LocatedIn,
    #[serde(rename = "rules_over")]
    RulesOver,
    #[serde(rename = "references")]
    References,
    #[serde(rename = "serves")]
    Serves,
    #[serde(rename = "rival_of")]
    RivalOf,
    #[serde(rename = "mentor_of")]
    MentorOf,
    #[serde(rename = "custom")]
    Custom,
}
