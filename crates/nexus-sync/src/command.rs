//! Sync Command Types
//!
//! Domain-level command variants built on top of the generated `SyncCommand` type.
//! These commands represent user-initiated operations that produce deltas for bundles.

use nexus_contracts::generated::SyncCommand;
use nexus_contracts::{CommandOrigin as ContractCommandOrigin, CommandStatus, CommandType};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::errors::{SyncError, SyncResult};

/// Extended command types for sync operations.
///
/// These wrap the generated `SyncCommand` with domain-specific command variants
/// that map to specific delta operations in bundles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncCommandVariant {
    /// Advance world state with new deltas.
    AdvanceWorld {
        world_id: String,
        creator_id: String,
    },
    /// Inject a future event into the world timeline.
    InjectFutureEvent {
        world_id: String,
        creator_id: String,
    },
    /// Extract knowledge base summary from world state.
    ExtractKb {
        world_id: String,
        creator_id: String,
    },
    /// Push local changes to platform.
    SyncPush {
        world_id: String,
        creator_id: String,
    },
    /// Pull remote state from platform.
    SyncPull {
        world_id: String,
        creator_id: String,
    },
    /// Fork world to a new branch.
    ForkWorld {
        world_id: String,
        creator_id: String,
        target_world_id: String,
    },
    /// Publish story to output.
    PublishStory {
        world_id: String,
        creator_id: String,
    },
}

impl SyncCommandVariant {
    /// Get the `command_type` string matching the generated `SyncCommand` schema.
    #[must_use] 
    pub const fn command_type_str(&self) -> &str {
        match self {
            Self::AdvanceWorld { .. } => "advance_world",
            Self::InjectFutureEvent { .. } => "inject_future_event",
            Self::ExtractKb { .. } => "extract_kb",
            Self::SyncPush { .. } => "sync_push",
            Self::SyncPull { .. } => "sync_pull",
            Self::ForkWorld { .. } => "fork_world",
            Self::PublishStory { .. } => "publish_story",
        }
    }

    /// Get the `world_id` from this command variant.
    #[must_use] 
    pub fn world_id(&self) -> &str {
        match self {
            Self::AdvanceWorld { world_id, .. }
            | Self::InjectFutureEvent { world_id, .. }
            | Self::ExtractKb { world_id, .. }
            | Self::SyncPush { world_id, .. }
            | Self::SyncPull { world_id, .. }
            | Self::ForkWorld { world_id, .. }
            | Self::PublishStory { world_id, .. } => world_id,
        }
    }

    /// Get the `creator_id` from this command variant.
    #[must_use] 
    pub fn creator_id(&self) -> &str {
        match self {
            Self::AdvanceWorld { creator_id, .. }
            | Self::InjectFutureEvent { creator_id, .. }
            | Self::ExtractKb { creator_id, .. }
            | Self::SyncPush { creator_id, .. }
            | Self::SyncPull { creator_id, .. }
            | Self::ForkWorld { creator_id, .. }
            | Self::PublishStory { creator_id, .. } => creator_id,
        }
    }

    /// Convert a generated `SyncCommand` into a `SyncCommandVariant`.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub fn from_sync_command(cmd: &SyncCommand) -> SyncResult<Self> {
        let _workspace_id = cmd.workspace_id.clone();
        let creator_id = cmd.creator_id.clone();
        let world_id = cmd.world_id.clone();

        match cmd.command_type.as_str() {
            "advance_world" => Ok(Self::AdvanceWorld {
                world_id,
                creator_id,
            }),
            "inject_future_event" => Ok(Self::InjectFutureEvent {
                world_id,
                creator_id,
            }),
            "extract_kb" => Ok(Self::ExtractKb {
                world_id,
                creator_id,
            }),
            "sync_push" => Ok(Self::SyncPush {
                world_id,
                creator_id,
            }),
            "sync_pull" => Ok(Self::SyncPull {
                world_id,
                creator_id,
            }),
            "fork_world" => {
                // Fork world requires target_world_id — check requested_by for V1.0
                let target_world_id = cmd.requested_by.clone().ok_or_else(|| {
                    SyncError::BundleValidation(
                        "fork_world command missing target_world_id".to_string(),
                    )
                })?;
                Ok(Self::ForkWorld {
                    world_id,
                    creator_id,
                    target_world_id,
                })
            }
            "publish_story" => Ok(Self::PublishStory {
                world_id,
                creator_id,
            }),
            other => Err(SyncError::BundleValidation(format!(
                "unknown command type: {other}"
            ))),
        }
    }

    /// Convert this variant into a generated `SyncCommand`.
    ///
    /// # Panics
    /// Panics if `command_type_str` or `origin` does not parse correctly.
    #[must_use] 
    pub fn to_sync_command(
        &self,
        command_id: &str,
        workspace_id: &str,
        origin: &str,
    ) -> SyncCommand {
        let now = chrono::Utc::now().to_rfc3339();
        SyncCommand {
            schema_version: 1,
            command_id: command_id.to_string(),
            workspace_id: workspace_id.to_string(),
            world_id: self.world_id().to_string(),
            creator_id: self.creator_id().to_string(),
            command_type: CommandType::from_str(self.command_type_str()).unwrap(),
            origin: ContractCommandOrigin::from_str(origin).unwrap(),
            output_manuscript: matches!(self, Self::PublishStory { .. }).then_some(true),
            status: CommandStatus::Pending,
            requested_by: match self {
                Self::ForkWorld {
                    target_world_id, ..
                } => Some(target_world_id.clone()),
                _ => None,
            },
            started_at: None,
            completed_at: None,
            created_at: now,
        }
    }
}

/// Command origin (matches generated SyncCommand.origin).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandOrigin {
    LocalUser,
    LocalAgent,
    OfficialCreator,
    System,
}

impl CommandOrigin {
    #[must_use] 
    pub const fn as_str(&self) -> &str {
        match self {
            Self::LocalUser => "local_user",
            Self::LocalAgent => "local_agent",
            Self::OfficialCreator => "official_creator",
            Self::System => "system",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::{CommandOrigin as WireCommandOrigin, CommandType};

    #[test]
    fn command_variant_roundtrip() {
        let variant = SyncCommandVariant::SyncPush {
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
        };

        assert_eq!(variant.command_type_str(), "sync_push");
        assert_eq!(variant.world_id(), "wld_test");
        assert_eq!(variant.creator_id(), "ctr_test");
    }

    #[test]
    fn variant_to_sync_command_and_back() {
        let variant = SyncCommandVariant::AdvanceWorld {
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
        };

        let cmd = variant.to_sync_command("cmd_001", "wrk_001", "local_user");
        assert_eq!(cmd.command_type, CommandType::AdvanceWorld);
        assert_eq!(cmd.world_id, "wld_test");
        assert_eq!(cmd.creator_id, "ctr_test");
        assert_eq!(cmd.origin, WireCommandOrigin::LocalUser);

        let recovered = SyncCommandVariant::from_sync_command(&cmd).expect("should convert");
        assert_eq!(recovered, variant);
    }

    #[test]
    fn variant_serialization_roundtrip() {
        let variant = SyncCommandVariant::PublishStory {
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
        };

        let json = serde_json::to_string(&variant).expect("serialize");
        let recovered: SyncCommandVariant = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(recovered, variant);
    }

    #[test]
    fn unknown_command_type_rejected_at_wire_deserialize() {
        let json = r#"{
            "schema_version": 1,
            "command_id": "cmd_001",
            "workspace_id": "wrk_001",
            "world_id": "wld_test",
            "creator_id": "ctr_test",
            "command_type": "unknown_type",
            "origin": "local_user",
            "output_manuscript": null,
            "status": "pending",
            "requested_by": null,
            "started_at": null,
            "completed_at": null,
            "created_at": "2025-01-01T00:00:00Z"
        }"#;
        assert!(serde_json::from_str::<SyncCommand>(json).is_err());
    }
}
