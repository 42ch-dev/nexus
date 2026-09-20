//! `creator character` — Character identity, bindings, knowledge, memory and
//! `ToM`.
//!
//! Identity, binding (v1.193 P0-T9) and knowledge (P0-T10) authority authors
//! through the shared direct-core seam ([`crate::core`]): one owner-scoped
//! `CoreService` is opened, the typed core call is issued behind its
//! stored-actor fences and explicit `--expected-revision` CAS, and the writer
//! is released before anything is rendered. No `DaemonClient` is constructed
//! for those arms.
//!
//! The knowledge arms read and write the stored `KnowledgeEntryRecord` and
//! project it onto the same generated DTO family the daemon adapter served
//! ([`knowledge_item_from_record`]), so the authored nullable summary, the
//! revision CAS and the native governance pair (`holder_entry_id` /
//! `disclosure`) stay observable through the CLI. A Character viewpoint stays
//! the core's holder-filtered admitted view; the Creator viewpoint stays the
//! management review — this leaf never unions owners itself and never
//! substitutes one for the other.
//!
//! The memory and `ToM` family (P0-T11) authors through the same seam:
//! capture, pending-review list/count/dismiss, the bounded review drain,
//! fragments and the revision-checked promote are the core's own
//! bearer-lease operations, and `ToM` record/show keep the core's carrier
//! admission and keyset paging. The retired `creator character run` and
//! `creator character soul reflect` entrances are gone together with the
//! daemon transport they needed: core host capture, session and soul
//! synthesis behavior stay library/core operations, not CLI commands.

use crate::commands::creator::work_utils::read_file_bounded;
use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::actor_knowledge::{
    add_knowledge_entry_request::AddKnowledgeEntryRequest,
    add_knowledge_entry_response::AddKnowledgeEntryResponse,
    knowledge_entry_detail::KnowledgeEntryDetail, knowledge_view_item::KnowledgeViewItem,
    list_character_knowledge_response::ListCharacterKnowledgeResponse,
    update_knowledge_entry_request::UpdateKnowledgeEntryRequestAudience,
    view_request::{NexusActorRef, ViewRequest}, view_response::ViewResponse,
};
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_response::CaptureCharacterPendingReviewResponse;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_query::ListCharacterTomQuery;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_response::NexusCharacterTomBeliefItem;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_response::RecordCharacterTomResponse;
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    character_binding_detail::CharacterBindingDetail,
    character_detail::{CharacterDetail, NexusCharacter as DetailCharacter},
    create_character_request::CreateCharacterRequest,
};
use nexus_contracts::generated::core::{
    CoreCharacterTransitionRequest, CoreCharacterTransitionRequestTargetStatus,
};
use nexus_core::{
    ActorKnowledgePage, ActorKnowledgeViewQuery, AdmittedActor, CoreService, Principal,
};
use nexus_knowledge::world_kb::knowledge_entry::{
    parse_stored_created_at, KnowledgeEntryRecord, LEGACY_CREATOR_ONLY_UNSUPPORTED,
};
use nexus_local_db::{CharacterPatch, FieldPatch, ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES};
use std::path::PathBuf;

/// `creator character` subcommands.
#[derive(Debug, Subcommand)]
pub enum CharacterCommand {
    /// Mint a Character with its first World binding
    Create {
        /// Character display name
        #[arg(long)]
        display_name: String,
        /// Owned World for the initial binding
        #[arg(long)]
        world_id: String,
        /// Optional image URI
        #[arg(long)]
        image_uri: Option<String>,
        /// Optional persona JSON object
        #[arg(long)]
        persona: Option<String>,
        /// Optional `WorldSheet` `KnowledgeEntry` id
        #[arg(long)]
        world_sheet_entry_id: Option<String>,
        /// Emit the generated `CreateCharacterResponse` DTO
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List Characters owned by the active Creator
    List {
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show one Character
    Show {
        character_id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Binding mutations
    Binding {
        #[command(subcommand)]
        command: BindingCommand,
    },
    /// Character `KnowledgeEntry` add/list/view
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommand,
    },
    /// Character SOUL/Memory bearer operations (v1.184 P3)
    Memory {
        #[command(subcommand)]
        command: CharacterMemoryCommand,
    },
    /// Character `ToM` L1/L2 record and show (v1.184 P4)
    Tom {
        #[command(subcommand)]
        command: CharacterTomCommand,
    },
    /// Edit Character identity metadata (explicit revision CAS)
    Edit {
        character_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long)]
        image_uri: Option<String>,
        #[arg(long)]
        clear_image_uri: bool,
        #[arg(long)]
        persona: Option<String>,
        #[arg(long)]
        clear_persona: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Archive (freeze) a Character
    Archive {
        character_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Restore an archived Character to active
    Restore {
        character_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator character binding` subcommands.
#[derive(Debug, Subcommand)]
pub enum BindingCommand {
    /// Add an active World binding
    Add {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        world_id: String,
        #[arg(long)]
        world_sheet_entry_id: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List bindings for a Character
    List {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show one binding detail
    Show {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Patch a binding `WorldSheet` link
    Edit {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        world_sheet_entry_id: Option<String>,
        #[arg(long, default_value_t = false)]
        clear_world_sheet: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Remove a non-last active binding
    Remove {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator character knowledge` subcommands.
#[derive(Debug, Subcommand)]
pub enum KnowledgeCommand {
    /// Add a `KnowledgeEntry` under a stored owner
    Add {
        /// Owner kind: world | character | binding
        #[arg(long)]
        owner: String,
        #[arg(long)]
        world_id: Option<String>,
        #[arg(long)]
        character_id: Option<String>,
        #[arg(long)]
        binding_id: Option<String>,
        /// Author audience: `shared`, `author-only`, or `character-private`.
        ///
        /// Omitted means in-scope shared (no holder, no disclosure). The author
        /// never supplies a holder id or a management flag: `author-only`
        /// resolves to the admitted controlling Creator's own holder and
        /// `character-private` to a Character this Creator owns.
        #[arg(long, value_name = "AUDIENCE")]
        audience: Option<String>,
        /// Character a `character-private` audience resolves to.
        #[arg(long, value_name = "CHARACTER_ID", requires = "audience")]
        audience_character: Option<String>,
        /// Retired World-only visibility flag. Refused on presence — including
        /// `--creator-only=false`: use `--audience author-only` instead.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        creator_only: Option<bool>,
        #[arg(long)]
        block_type: String,
        #[arg(long)]
        canonical_name: String,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        summary_file: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List Character-owned `KnowledgeEntry` rows (no World union)
    List {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Compose `KnowledgeView` for a Creator or Character `actor_ref`
    View {
        /// `actor_kind`: creator | character
        #[arg(long)]
        actor: String,
        #[arg(long)]
        creator_id: Option<String>,
        #[arg(long)]
        character_id: Option<String>,
        #[arg(long)]
        world_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show one Character-scoped `KnowledgeEntry` detail (summary + metadata)
    Show {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        entry_id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Edit `canonical_name` and/or summary with revision CAS
    Edit {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        entry_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        canonical_name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        summary_file: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        clear_summary: bool,
        /// Move the governance pair under the same `--expected-revision` CAS:
        /// `shared` clears holder+disclosure, `author-only` resolves the
        /// Creator's holder, `character-private` a permitted Character's.
        /// Omitted preserves the stored pair.
        #[arg(long, value_name = "AUDIENCE")]
        audience: Option<String>,
        /// Character a `character-private` audience resolves to.
        #[arg(long, value_name = "CHARACTER_ID", requires = "audience")]
        audience_character: Option<String>,
        /// Retired World-only visibility flag. Refused on presence — including
        /// `--creator-only=false`: use `--audience author-only` instead.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        creator_only: Option<bool>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Delete an unreferenced `KnowledgeEntry` with revision CAS
    Remove {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        entry_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator character memory` subcommands (v1.184 P3).
#[derive(Debug, Subcommand)]
pub enum CharacterMemoryCommand {
    /// Capture a session-end digest into the Character review queue
    Capture {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        pending_id: String,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        #[arg(long)]
        task_kind: Option<String>,
        /// Session digest (raw text)
        #[arg(long)]
        digest: String,
        #[arg(long)]
        created_at: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List pending-review entries for a Character scope
    PendingList {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Count pending-review entries for a Character scope
    PendingCount {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Dismiss a pending-review entry without promoting
    PendingDismiss {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        pending_id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Drain one bounded batch of the Character review queue
    Review {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List Character memory fragments for a scope
    Fragments {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Explicit, revision-checked local→shared promotion
    Promote {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        fragment_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator character tom` subcommands (v1.184 P4).
#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)] // clap derive requires non-boxed subcommands
pub enum CharacterTomCommand {
    /// Record one L1 or L2 belief on an authorized carrier
    Record {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        world_id: String,
        #[arg(long)]
        binding_id: String,
        #[arg(long)]
        carrier_entry_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        holder: String,
        #[arg(long)]
        proposition: String,
        #[arg(long)]
        order: i64,
        #[arg(long)]
        truth: Option<String>,
        #[arg(long)]
        access: Option<String>,
        #[arg(long)]
        representation: Option<String>,
        #[arg(long)]
        content_type: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        occurred_at: Option<String>,
        #[arg(long)]
        sort_key: Option<String>,
        #[arg(long)]
        event_id: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List bounded `ToM` rows (L1 before L2 in keyset order)
    Show {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        world_id: String,
        #[arg(long)]
        binding_id: String,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run `creator character`.
///
/// Every arm authors through the shared direct-core seam and the writer is
/// released by [`finish_direct`] before any line is printed, so a command
/// never reports an outcome its core could not settle — on success and on
/// refusal alike.
///
/// # Errors
///
/// Returns the mapped core refusal (admission, CAS conflict, storage) and any
/// cleanup refusal from [`finish_direct`].
pub async fn run(cmd: CharacterCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        run_arm(&core, &principal, cmd).await
    }
    .await;
    // `None` is a settled verb that prints nothing (`--json` on a remove).
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

/// Dispatch one arm.
///
/// Every mutation carries the caller's explicit `--expected-revision` into the
/// core's CAS, and every lifecycle write goes through the core's exclusive
/// per-Character transition lease — never a bare status write.
async fn run_arm(
    core: &CoreService,
    principal: &Principal,
    cmd: CharacterCommand,
) -> Result<Option<String>> {
    match cmd {
        CharacterCommand::Create {
            display_name,
            world_id,
            image_uri,
            persona,
            world_sheet_entry_id,
            json,
        } => {
            create(
                core,
                principal,
                display_name,
                world_id,
                image_uri,
                persona,
                world_sheet_entry_id,
                json,
            )
            .await
        }
        CharacterCommand::List {
            limit,
            cursor,
            json,
        } => list(core, principal, limit, cursor, json).await,
        CharacterCommand::Show { character_id, json } => {
            show(core, principal, &character_id, json).await
        }
        CharacterCommand::Binding { command } => match command {
            BindingCommand::Add {
                character_id,
                world_id,
                world_sheet_entry_id,
                json,
            } => {
                add_binding(
                    core,
                    principal,
                    &character_id,
                    world_id,
                    world_sheet_entry_id,
                    json,
                )
                .await
            }
            BindingCommand::List {
                character_id,
                limit,
                cursor,
                json,
            } => list_bindings(core, principal, &character_id, limit, cursor, json).await,
            BindingCommand::Show {
                character_id,
                binding_id,
                json,
            } => show_binding(core, principal, &character_id, &binding_id, json).await,
            BindingCommand::Edit {
                character_id,
                binding_id,
                expected_revision,
                world_sheet_entry_id,
                clear_world_sheet,
                json,
            } => {
                edit_binding(
                    core,
                    principal,
                    &character_id,
                    &binding_id,
                    expected_revision,
                    world_sheet_entry_id,
                    clear_world_sheet,
                    json,
                )
                .await
            }
            BindingCommand::Remove {
                character_id,
                binding_id,
                json,
            } => remove_binding(core, principal, &character_id, &binding_id, json).await,
        },
        CharacterCommand::Knowledge { command } => match command {
            KnowledgeCommand::Add {
                owner,
                world_id,
                character_id,
                binding_id,
                audience,
                audience_character,
                creator_only,
                block_type,
                canonical_name,
                summary,
                summary_file,
                json,
            } => {
                refuse_legacy_creator_only_flag(creator_only)?;
                let audience = audience_wire(audience.as_deref(), audience_character.as_deref())?;
                add_knowledge(
                    core,
                    principal,
                    &owner,
                    world_id,
                    character_id,
                    binding_id,
                    audience,
                    block_type,
                    canonical_name,
                    summary,
                    summary_file,
                    json,
                )
                .await
            }
            KnowledgeCommand::Show {
                character_id,
                entry_id,
                json,
            } => show_knowledge(core, principal, &character_id, &entry_id, json).await,
            KnowledgeCommand::Edit {
                character_id,
                entry_id,
                expected_revision,
                canonical_name,
                summary,
                summary_file,
                clear_summary,
                audience,
                audience_character,
                creator_only,
                json,
            } => {
                refuse_legacy_creator_only_flag(creator_only)?;
                let audience = audience_wire(audience.as_deref(), audience_character.as_deref())?;
                edit_knowledge(
                    core,
                    principal,
                    &character_id,
                    &entry_id,
                    expected_revision,
                    canonical_name,
                    summary,
                    summary_file,
                    clear_summary,
                    audience,
                    json,
                )
                .await
            }
            KnowledgeCommand::Remove {
                character_id,
                entry_id,
                expected_revision,
                json,
            } => {
                remove_knowledge(
                    core,
                    principal,
                    &character_id,
                    &entry_id,
                    expected_revision,
                    json,
                )
                .await
            }
            KnowledgeCommand::List {
                character_id,
                limit,
                cursor,
                json,
            } => list_knowledge(core, principal, &character_id, limit, cursor, json).await,
            KnowledgeCommand::View {
                actor,
                creator_id,
                character_id,
                world_id,
                binding_id,
                limit,
                cursor,
                json,
            } => {
                view_knowledge(
                    core,
                    principal,
                    &actor,
                    creator_id,
                    character_id,
                    world_id,
                    binding_id,
                    limit,
                    cursor,
                    json,
                )
                .await
            }
        },
        CharacterCommand::Edit {
            character_id,
            expected_revision,
            display_name,
            image_uri,
            clear_image_uri,
            persona,
            clear_persona,
            json,
        } => {
            edit_character(
                core,
                principal,
                &character_id,
                expected_revision,
                display_name,
                image_uri,
                clear_image_uri,
                persona,
                clear_persona,
                json,
            )
            .await
        }
        CharacterCommand::Archive {
            character_id,
            expected_revision,
            json,
        } => archive_character(core, principal, &character_id, expected_revision, json).await,
        CharacterCommand::Restore {
            character_id,
            expected_revision,
            json,
        } => restore_character(core, principal, &character_id, expected_revision, json).await,
        CharacterCommand::Memory { command } => match command {
            CharacterMemoryCommand::Capture {
                character_id,
                pending_id,
                session_id,
                binding_id,
                task_kind,
                digest,
                created_at,
                json,
            } => {
                memory_capture(
                    core,
                    principal,
                    &character_id,
                    &pending_id,
                    &session_id,
                    binding_id,
                    task_kind,
                    digest,
                    created_at,
                    json,
                )
                .await
            }
            CharacterMemoryCommand::PendingList {
                character_id,
                binding_id,
                limit,
                cursor,
                json,
            } => memory_pending_list(core, principal, &character_id, binding_id, limit, cursor, json).await,
            CharacterMemoryCommand::PendingCount {
                character_id,
                binding_id,
                json,
            } => memory_pending_count(core, principal, &character_id, binding_id, json).await,
            CharacterMemoryCommand::PendingDismiss {
                character_id,
                pending_id,
                json,
            } => {
                memory_pending_dismiss(core, principal, &character_id, &pending_id, json).await
            }
            CharacterMemoryCommand::Review {
                character_id,
                binding_id,
                json,
            } => memory_review(core, principal, &character_id, binding_id, json).await,
            CharacterMemoryCommand::Fragments {
                character_id,
                binding_id,
                limit,
                cursor,
                json,
            } => memory_fragments(core, principal, &character_id, binding_id, limit, cursor, json).await,
            CharacterMemoryCommand::Promote {
                character_id,
                fragment_id,
                expected_revision,
                json,
            } => {
                memory_promote(
                    core,
                    principal,
                    &character_id,
                    &fragment_id,
                    expected_revision,
                    json,
                )
                .await
            }
        },
        CharacterCommand::Tom { command } => match command {
            CharacterTomCommand::Record {
                character_id,
                world_id,
                binding_id,
                carrier_entry_id,
                expected_revision,
                holder,
                proposition,
                order,
                truth,
                access,
                representation,
                content_type,
                source,
                context,
                occurred_at,
                sort_key,
                event_id,
                json,
            } => {
                tom_record(
                    core,
                    principal,
                    &character_id,
                    world_id,
                    binding_id,
                    carrier_entry_id,
                    expected_revision,
                    holder,
                    proposition,
                    order,
                    truth,
                    access,
                    representation,
                    content_type,
                    source,
                    context,
                    occurred_at,
                    sort_key,
                    event_id,
                    json,
                )
                .await
            }
            CharacterTomCommand::Show {
                character_id,
                world_id,
                binding_id,
                limit,
                cursor,
                json,
            } => {
                tom_show(
                    core,
                    principal,
                    &character_id,
                    &world_id,
                    &binding_id,
                    limit,
                    cursor,
                    json,
                )
                .await
            }
        },
    }
}

fn parse_persona(raw: Option<String>) -> Result<serde_json::Map<String, serde_json::Value>> {
    let Some(text) = raw else {
        return Ok(serde_json::Map::new());
    };
    let value: serde_json::Value = serde_json::from_str(&text)?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| CliError::Other("--persona must be a JSON object".into()))
}

/// Default `--limit` page size for the Character/binding list leaves (the
/// retired daemon adapter's page size).
const DEFAULT_LIST_LIMIT: u32 = 50;
/// Largest `--limit` the list leaves accept (the retired adapter's ceiling).
const MAX_LIST_LIMIT: u32 = 100;

/// Convert a CLI-supplied string into one generated request newtype, surfacing
/// the generated constraint message as the command's own error.
fn wire_field<T: TryFrom<String>>(value: String) -> Result<T>
where
    T::Error: std::fmt::Display,
{
    value.try_into().map_err(wire_error)
}

/// Surface a generated wire-conversion failure as a CLI input error.
fn wire_error(err: impl std::fmt::Display) -> CliError {
    CliError::Other(err.to_string())
}

/// Convert a CLI `--expected-revision` (`u64`) into the stored `i64` revision.
///
/// The flag is wider than the stored revision: a value the core's `i64` cannot
/// represent is refused instead of wrapping into a negative revision, which
/// would silently address a different row revision.
fn revision_i64(raw: u64) -> Result<i64> {
    i64::try_from(raw)
        .map_err(|_| CliError::Other("expected_revision is out of range for i64".into()))
}

/// Resolve `--limit` into the core's page size.
fn resolve_list_limit(raw: Option<i64>) -> Result<u32> {
    let Some(limit) = raw else {
        return Ok(DEFAULT_LIST_LIMIT);
    };
    u32::try_from(limit)
        .ok()
        .filter(|limit| (1..=MAX_LIST_LIMIT).contains(limit))
        .ok_or_else(|| CliError::Other(format!("limit must be between 1 and {MAX_LIST_LIMIT}")))
}

/// Decode the opaque `v1:<offset>` cursor the list responses mint.
///
/// The token is client-facing and opaque: only the retained `v1:` encoding is
/// understood, and anything else is refused rather than silently restarted
/// from the first page.
fn decode_list_cursor(cursor: Option<&str>) -> Result<u32> {
    let Some(raw) = cursor else {
        return Ok(0);
    };
    raw.strip_prefix("v1:")
        .and_then(|offset| offset.parse::<u32>().ok())
        .ok_or_else(|| {
            CliError::Other(
                "invalid pagination cursor; pass the `next_cursor` value returned by the previous \
                 response unchanged"
                    .to_string(),
            )
        })
}

/// Create a Character with its initial active World binding.
///
/// # Errors
///
/// Returns [`CliError`] when a supplied value fails its generated constraint
/// (`--world-id` shape, display-name bounds, `--persona` shape) and the mapped
/// core refusal otherwise (foreign/unknown World,
/// `duplicate_character_display_name`, `invalid_world_sheet`).
async fn create(
    core: &CoreService,
    principal: &Principal,
    display_name: String,
    world_id: String,
    image_uri: Option<String>,
    persona: Option<String>,
    world_sheet_entry_id: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let request = CreateCharacterRequest {
        display_name: wire_field(display_name)?,
        image_uri: image_uri.map(wire_field).transpose()?,
        persona: parse_persona(persona)?,
        world_id: wire_field(world_id)?,
        world_sheet_entry_id: world_sheet_entry_id.map(wire_field).transpose()?,
    };
    let resp = core
        .create_character(principal, request)
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!(
        "Character created:\n  character_id: {}\n  display_name:  {}\n  binding_id:    {}\n  world_id:       {}",
        *resp.character.character_id,
        *resp.character.display_name,
        *resp.binding.binding_id,
        *resp.binding.world_id
    )))
}

/// List the active Creator's Characters, oldest first.
///
/// # Errors
///
/// Returns [`CliError`] for an out-of-range `--limit` or a malformed
/// `--cursor`, and the mapped core refusal otherwise.
async fn list(
    core: &CoreService,
    principal: &Principal,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let page = resolve_list_limit(limit)?;
    let offset = decode_list_cursor(cursor.as_deref())?;
    let resp = core
        .list_characters(principal, page, offset)
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No characters.".to_string()));
    }
    let mut lines: Vec<String> = resp
        .items
        .iter()
        .map(|item| {
            format!(
                "{}  {}  {}",
                *item.character_id, *item.display_name, item.status
            )
        })
        .collect();
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

/// Show one owned Character.
///
/// # Errors
///
/// Returns the mapped core refusal (unknown/foreign id is `[not_found]`).
async fn show(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .character(principal, character_id.to_string())
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    let c = &resp.character;
    Ok(Some(format!(
        "character_id: {}\ndisplay_name: {}\nstatus:       {}\nowner:        {}",
        *c.character_id, *c.display_name, c.status, *c.owner_creator_id
    )))
}

/// Render one Character detail as the `--json` DTO or the human block.
fn render_character_detail(resp: &CharacterDetail, json: bool) -> Result<String> {
    if json {
        return Ok(serde_json::to_string_pretty(resp)?);
    }
    let c = &resp.character;
    Ok(format!(
        "character_id: {}\ndisplay_name: {}\nstatus:       {}\nrevision:     {}\nowner:        {}",
        *c.character_id, *c.display_name, c.status, c.revision, *c.owner_creator_id
    ))
}

/// Edit Character identity metadata under the caller's explicit revision CAS.
///
/// # Errors
///
/// Returns [`CliError`] for a mutually exclusive flag pair, an empty patch or
/// an unrepresentable `--expected-revision`, and the mapped core refusal
/// otherwise (`character_revision_conflict`, `character_inactive`).
#[allow(clippy::too_many_arguments)]
async fn edit_character(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    expected_revision: u64,
    display_name: Option<String>,
    image_uri: Option<String>,
    clear_image_uri: bool,
    persona: Option<String>,
    clear_persona: bool,
    json: bool,
) -> Result<Option<String>> {
    if clear_image_uri && image_uri.is_some() {
        return Err(CliError::Other(
            "use either --image-uri or --clear-image-uri, not both".into(),
        ));
    }
    if clear_persona && persona.is_some() {
        return Err(CliError::Other(
            "use either --persona or --clear-persona, not both".into(),
        ));
    }
    if display_name.is_none()
        && image_uri.is_none()
        && !clear_image_uri
        && persona.is_none()
        && !clear_persona
    {
        return Err(CliError::Other(
            "edit requires at least one mutable field (--display-name, --image-uri, --clear-image-uri, --persona, or --clear-persona)".into(),
        ));
    }
    // The tri-state `FieldPatch` is the storage contract: an omitted member
    // keeps the stored value, an explicit `--clear-*` clears it, and a supplied
    // value sets it. Persona is stored as a JSON object string.
    let persona_json = match persona {
        Some(raw) => Some(serde_json::Value::Object(parse_persona(Some(raw))?).to_string()),
        None => None,
    };
    let patch = CharacterPatch {
        display_name: display_name.as_deref(),
        image_uri: if clear_image_uri {
            FieldPatch::Clear
        } else if let Some(uri) = image_uri.as_deref() {
            FieldPatch::Set(uri)
        } else {
            FieldPatch::Keep
        },
        persona_json: if clear_persona {
            FieldPatch::Clear
        } else if let Some(encoded) = persona_json.as_deref() {
            FieldPatch::Set(encoded)
        } else {
            FieldPatch::Keep
        },
    };
    let resp = core
        .patch_character(
            principal,
            character_id.to_string(),
            revision_i64(expected_revision)?,
            patch,
        )
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_character_detail(&resp, json)?))
}

/// Archive one owned Character through the core's lifecycle transition.
///
/// # Errors
///
/// As [`transition_character`].
async fn archive_character(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    expected_revision: u64,
    json: bool,
) -> Result<Option<String>> {
    transition_character(
        core,
        principal,
        character_id,
        expected_revision,
        CoreCharacterTransitionRequestTargetStatus::Archived,
        json,
    )
    .await
}

/// Restore one owned Character through the core's lifecycle transition.
///
/// # Errors
///
/// As [`transition_character`].
async fn restore_character(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    expected_revision: u64,
    json: bool,
) -> Result<Option<String>> {
    transition_character(
        core,
        principal,
        character_id,
        expected_revision,
        CoreCharacterTransitionRequestTargetStatus::Active,
        json,
    )
    .await
}

/// Commit one Character lifecycle transition.
///
/// The one-call core transition takes the exclusive per-Character lease
/// (busy refusal, ownership and pre-transition epoch re-read under the fence)
/// **and** commits the revision-checked write, so this leaf can never bypass
/// the stored lifecycle epoch the way a bare status patch would.
///
/// # Errors
///
/// Returns [`CliError`] for an unrepresentable `--expected-revision` and the
/// mapped core refusal otherwise (`character_busy`, `character_revision_conflict`,
/// `character_restore_requires_active_binding`, `holder_state_invalid`).
async fn transition_character(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    expected_revision: u64,
    target_status: CoreCharacterTransitionRequestTargetStatus,
    json: bool,
) -> Result<Option<String>> {
    let request: CoreCharacterTransitionRequest = CoreCharacterTransitionRequest::builder()
        .character_id(character_id.to_string())
        .expected_revision(revision_i64(expected_revision)?)
        .target_status(target_status)
        .try_into()
        .map_err(wire_error)?;
    let resp = core
        .transition_character(principal, request)
        .await
        .map_err(map_core_error)?;
    // The transition response carries the core's own Character wire; the
    // detail envelope is the retained CLI projection of the same fields.
    let character = serde_json::to_value(resp.character)?;
    let detail = CharacterDetail {
        character: serde_json::from_value::<DetailCharacter>(character)?,
    };
    Ok(Some(render_character_detail(&detail, json)?))
}

/// Render one binding detail as the `--json` DTO or the human line.
fn render_binding_detail(resp: &CharacterBindingDetail, json: bool) -> Result<String> {
    if json {
        return Ok(serde_json::to_string_pretty(resp)?);
    }
    let b = &resp.binding;
    Ok(format!(
        "{}  {}  rev={}  sheet={}",
        *b.binding_id,
        *b.world_id,
        b.revision,
        b.world_sheet_entry_id
            .as_ref()
            .map_or("-", |id| id.as_str())
    ))
}

/// Show one owned binding (retained reads tolerate an archived Character).
///
/// # Errors
///
/// Returns the mapped core refusal (unknown/foreign binding is `[not_found]`).
async fn show_binding(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .binding(principal, character_id.to_string(), binding_id.to_string())
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_binding_detail(&resp, json)?))
}

/// Patch the optional WorldSheet link under the caller's explicit revision CAS.
///
/// # Errors
///
/// Returns [`CliError`] for a mutually exclusive flag pair, a missing patch
/// value or an unrepresentable `--expected-revision`, and the mapped core
/// refusal otherwise (`binding_revision_conflict`, `character_inactive`,
/// `invalid_world_sheet`).
#[allow(clippy::too_many_arguments)]
async fn edit_binding(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
    expected_revision: u64,
    world_sheet_entry_id: Option<String>,
    clear_world_sheet: bool,
    json: bool,
) -> Result<Option<String>> {
    if clear_world_sheet && world_sheet_entry_id.is_some() {
        return Err(CliError::Other(
            "use either --world-sheet-entry-id or --clear-world-sheet, not both".into(),
        ));
    }
    // Tri-state `FieldPatch`: `--clear-world-sheet` clears the nullable link,
    // a supplied entry id sets it, and an omitted member keeps it.
    let sheet_patch = if clear_world_sheet {
        FieldPatch::Clear
    } else {
        FieldPatch::Set(world_sheet_entry_id.as_deref().ok_or_else(|| {
            CliError::Other("edit requires --world-sheet-entry-id or --clear-world-sheet".into())
        })?)
    };
    let resp = core
        .patch_binding(
            principal,
            character_id.to_string(),
            binding_id.to_string(),
            revision_i64(expected_revision)?,
            sheet_patch,
        )
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_binding_detail(&resp, json)?))
}

/// Add one active World binding to an owned Character.
///
/// # Errors
///
/// Returns [`CliError`] when `--world-id` fails its generated shape constraint
/// and the mapped core refusal otherwise (`duplicate_active_actor_world_binding`,
/// `invalid_world_sheet`, foreign/unknown ids).
async fn add_binding(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    world_id: String,
    world_sheet_entry_id: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let request = AddCharacterBindingRequest {
        world_id: wire_field(world_id)?,
        world_sheet_entry_id: world_sheet_entry_id.map(wire_field).transpose()?,
    };
    let resp = core
        .add_binding(
            principal,
            character_id.to_string(),
            request.world_id.to_string(),
            request
                .world_sheet_entry_id
                .as_ref()
                .map(|id| id.as_str().to_string()),
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!(
        "Binding added:\n  binding_id: {}\n  world_id:    {}",
        *resp.binding.binding_id, *resp.binding.world_id
    )))
}

/// List one owned Character's bindings.
///
/// # Errors
///
/// Returns [`CliError`] for an out-of-range `--limit` or a malformed
/// `--cursor`, and the mapped core refusal otherwise (foreign Character).
async fn list_bindings(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let page = resolve_list_limit(limit)?;
    let offset = decode_list_cursor(cursor.as_deref())?;
    let resp = core
        .list_bindings(principal, character_id.to_string(), page, offset)
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No bindings.".to_string()));
    }
    let mut lines: Vec<String> = resp
        .items
        .iter()
        .map(|item| format!("{}  {}  {}", *item.binding_id, *item.world_id, item.status))
        .collect();
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

/// Remove one active binding (the last active binding is a zero-mutation 409).
///
/// # Errors
///
/// Returns the mapped core refusal (`last_active_actor_world_binding`, a
/// binding that still owns knowledge or local memory, foreign/unknown ids).
async fn remove_binding(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
    json: bool,
) -> Result<Option<String>> {
    core.remove_binding(principal, character_id.to_string(), binding_id.to_string())
        .await
        .map_err(map_core_error)?;
    Ok(json.then(|| "{}".to_string()))
}

fn load_bounded_summary_text(
    summary: Option<String>,
    summary_file: Option<PathBuf>,
) -> Result<Option<String>> {
    if summary.is_some() && summary_file.is_some() {
        return Err(CliError::Other(
            "use either --summary or --summary-file, not both".into(),
        ));
    }
    if let Some(path) = summary_file {
        let text = read_file_bounded(
            &path.to_string_lossy(),
            ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES,
            "--summary-file",
        )?;
        return Ok(Some(text));
    }
    Ok(summary)
}

/// Round-trip one projected value into a generated response DTO.
///
/// The generated response families each own an inline item type with the same
/// JSON shape, exactly like the daemon adapter's own handlers: the projection
/// is built once ([`knowledge_item_from_record`]) and re-read into whichever
/// response envelope the arm returns.
fn wire_map<T: serde::de::DeserializeOwned>(value: impl serde::Serialize) -> Result<T> {
    let value = serde_json::to_value(value)?;
    Ok(serde_json::from_value(value)?)
}

/// The retained item projection of one stored knowledge record.
///
/// Mirrors the native bridge's `knowledge_item_from_record`
/// (`crates/nexus-core-node/src/actors.rs`): the native governance pair
/// (`holder_entry_id` / `disclosure`) is projected, the retired `creator_only`
/// boolean is not, and `owner` stays the narrative owner container.
fn knowledge_item_from_record(record: &KnowledgeEntryRecord) -> Result<KnowledgeViewItem> {
    let created_at = parse_stored_created_at(&record.created_at).map_err(CliError::Other)?;
    let mut value = serde_json::Map::new();
    value.insert("entry_id".into(), serde_json::json!(record.entry_id));
    value.insert(
        "owner".into(),
        serde_json::json!({ "kind": record.owner.kind(), "id": record.owner.id() }),
    );
    value.insert(
        "block_type".into(),
        serde_json::to_value(record.block_type)?,
    );
    value.insert(
        "canonical_name".into(),
        serde_json::json!(record.canonical_name),
    );
    value.insert("status".into(), serde_json::json!(record.status));
    value.insert(
        "revision".into(),
        serde_json::json!(record.revision.unwrap_or(0)),
    );
    value.insert("created_at".into(), serde_json::json!(created_at));
    if let Some(holder) = record.holder_entry_id.as_deref() {
        value.insert("holder_entry_id".into(), serde_json::json!(holder));
    }
    if let Some(disclosure) = record.disclosure.as_deref() {
        value.insert("disclosure".into(), serde_json::json!(disclosure));
    }
    wire_map(serde_json::Value::Object(value))
}

/// The detail envelope's summary: the canonical authored body summary, `null`
/// when the stored body carries no summary member.
fn knowledge_summary_wire_value(record: &KnowledgeEntryRecord) -> Option<&str> {
    record
        .body
        .as_ref()
        .and_then(|body| body.summary.as_deref())
}

/// The retained detail projection of one stored knowledge record.
fn knowledge_detail_from_record(record: &KnowledgeEntryRecord) -> Result<KnowledgeEntryDetail> {
    let item = knowledge_item_from_record(record)?;
    wire_map(serde_json::json!({
        "item": item,
        "summary": knowledge_summary_wire_value(record),
    }))
}

/// The retained page projection of one core knowledge page.
fn knowledge_page_wire_value(page: &ActorKnowledgePage) -> Result<serde_json::Value> {
    let mut items = Vec::with_capacity(page.items.len());
    for record in &page.items {
        items.push(serde_json::to_value(knowledge_item_from_record(record)?)?);
    }
    Ok(serde_json::json!({
        "items": items,
        "pagination": {
            "limit": i64::from(page.limit),
            "has_more": page.has_more,
            "next_cursor": page.next_cursor,
        },
    }))
}

/// Derive the opaque [`AdmittedActor`] token from the wire `actor_ref`.
///
/// Projection only: the core re-validates stored ownership inside the call, so
/// a payload claim never widens the view, and the Creator arm never inherits
/// the Character's holder-filtered selection.
fn admitted_from_ref(actor_ref: &NexusActorRef) -> AdmittedActor {
    match actor_ref {
        NexusActorRef::CreatorActorRef { creator_id, .. } => AdmittedActor::Creator {
            creator_id: creator_id.to_string(),
        },
        NexusActorRef::CharacterActorRef { character_id, .. } => AdmittedActor::Character {
            character_id: character_id.to_string(),
        },
    }
}

fn render_knowledge_detail(resp: &KnowledgeEntryDetail, json: bool) -> Result<String> {
    if json {
        return Ok(serde_json::to_string_pretty(resp)?);
    }
    let summary = match resp.summary.as_ref() {
        Some(text) => format!("summary:\n{}", text.as_str()),
        None => "summary: (none)".to_string(),
    };
    Ok(format!(
        "entry_id: {}\ncanonical_name: {}\nrevision: {}\n{summary}",
        *resp.item.entry_id, *resp.item.canonical_name, resp.item.revision
    ))
}

/// Durable §5: the retired World-only `--creator-only` flag is refused by
/// **presence**, `--creator-only=false` included — it is the compatibility
/// input to refuse, never a permanent alias.
fn refuse_legacy_creator_only_flag(present: Option<bool>) -> Result<()> {
    if present.is_some() {
        return Err(CliError::Other(format!(
            "{LEGACY_CREATOR_ONLY_UNSUPPORTED}: --creator-only is not accepted; use              --audience shared|author-only|character-private"
        )));
    }
    Ok(())
}

/// Map the closed `--audience` / `--audience-character` pair onto the frozen
/// wire object (durable §3). The author supplies intent only: the permitted
/// identity and its holder are resolved by core admission against stored state.
fn audience_wire(
    audience: Option<&str>,
    character_id: Option<&str>,
) -> Result<Option<serde_json::Value>> {
    let Some(audience) = audience else {
        if character_id.is_some() {
            return Err(CliError::Other(
                "--audience-character requires --audience character-private".into(),
            ));
        }
        return Ok(None);
    };
    if audience != "character-private" && character_id.is_some() {
        return Err(CliError::Other(format!(
            "--audience-character is only meaningful with --audience character-private, \
             not '{audience}'"
        )));
    }
    match audience {
        "shared" => Ok(Some(serde_json::json!({ "kind": "shared" }))),
        "author-only" => Ok(Some(serde_json::json!({ "kind": "author-only" }))),
        "character-private" => {
            let character_id = character_id.ok_or_else(|| {
                CliError::Other(
                    "--audience character-private requires --audience-character <CHARACTER_ID>"
                        .into(),
                )
            })?;
            Ok(Some(serde_json::json!({
                "kind": "character-private",
                "character_id": character_id,
            })))
        }
        other => Err(CliError::Other(format!(
            "unknown --audience {other}; expected shared, author-only, or character-private"
        ))),
    }
}

fn owner_kind_wire(owner: &str) -> Result<&'static str> {
    match owner {
        "world" => Ok("world"),
        "character" => Ok("character"),
        "binding" | "actor_world_binding" => Ok("actor_world_binding"),
        other => Err(CliError::Other(format!(
            "unknown --owner {other}; expected world, character, or binding"
        ))),
    }
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn add_knowledge(
    core: &CoreService,
    principal: &Principal,
    owner: &str,
    world_id: Option<String>,
    character_id: Option<String>,
    binding_id: Option<String>,
    audience: Option<serde_json::Value>,
    block_type: String,
    canonical_name: String,
    summary: Option<String>,
    summary_file: Option<PathBuf>,
    json: bool,
) -> Result<Option<String>> {
    let mut body = serde_json::json!({
        "owner_kind": owner_kind_wire(owner)?,
        "block_type": block_type,
        "canonical_name": canonical_name,
    });
    if let Some(audience) = audience {
        body["audience"] = audience;
    }
    if let Some(id) = world_id {
        body["world_id"] = serde_json::Value::String(id);
    }
    if let Some(id) = character_id {
        body["character_id"] = serde_json::Value::String(id);
    }
    if let Some(id) = binding_id {
        body["binding_id"] = serde_json::Value::String(id);
    }
    if let Some(text) = load_bounded_summary_text(summary, summary_file)? {
        body["summary"] = serde_json::Value::String(text);
    }
    // The retained absent/null distinction is a request-shape concern owned by
    // this leaf: the raw member presence drives the core's `summary_present`
    // bit, so a World-owned create carrying the member is refused by the core
    // exactly as the daemon adapter refused it.
    let summary_present = body.get("summary").is_some();
    let req: AddKnowledgeEntryRequest = serde_json::from_value(body)?;
    let stored = core
        .add_actor_knowledge_entry(principal, req, summary_present)
        .await
        .map_err(map_core_error)?;
    let resp: AddKnowledgeEntryResponse =
        wire_map(serde_json::json!({ "item": knowledge_item_from_record(&stored)? }))?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!(
        "KnowledgeEntry added:\n  entry_id: {}\n  owner:    {}",
        *resp.item.entry_id,
        serde_json::to_string(&resp.item.owner)?
    )))
}

async fn show_knowledge(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    entry_id: &str,
    json: bool,
) -> Result<Option<String>> {
    let stored = core
        .actor_knowledge_entry(principal, character_id.to_string(), entry_id.to_string())
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_knowledge_detail(
        &knowledge_detail_from_record(&stored)?,
        json,
    )?))
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn edit_knowledge(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    entry_id: &str,
    expected_revision: u64,
    canonical_name: Option<String>,
    summary: Option<String>,
    summary_file: Option<PathBuf>,
    clear_summary: bool,
    audience: Option<serde_json::Value>,
    json: bool,
) -> Result<Option<String>> {
    if clear_summary && (summary.is_some() || summary_file.is_some()) {
        return Err(CliError::Other(
            "use either --clear-summary or --summary/--summary-file, not both".into(),
        ));
    }
    if canonical_name.is_none()
        && !clear_summary
        && summary.is_none()
        && summary_file.is_none()
        && audience.is_none()
    {
        return Err(CliError::Other(
            "edit requires --canonical-name, --summary, --summary-file, --clear-summary, or \
             --audience"
                .into(),
        ));
    }
    // Tri-state `FieldPatch` is the storage contract: an omitted member keeps
    // the stored summary, `--clear-summary` clears it, a supplied text sets it.
    let summary_text = load_bounded_summary_text(summary, summary_file)?;
    let summary_patch = if clear_summary {
        FieldPatch::Clear
    } else {
        match summary_text.as_deref() {
            Some(text) => FieldPatch::Set(text),
            None => FieldPatch::Keep,
        }
    };
    // The closed `--audience` pair is admitted from the wire member exactly
    // like the native bridge: the CLI never supplies a holder id, and the core
    // resolves the permitted identity against stored state under the CAS.
    let audience_wire: Option<UpdateKnowledgeEntryRequestAudience> = audience
        .map(serde_json::from_value)
        .transpose()
        .map_err(|err| CliError::Other(err.to_string()))?;
    let audience = nexus_core::authored_patch_audience(audience_wire.as_ref())
        .map_err(map_core_error)?;
    let stored = core
        .patch_actor_knowledge_entry(
            principal,
            character_id.to_string(),
            entry_id.to_string(),
            revision_i64(expected_revision)?,
            canonical_name.as_deref(),
            summary_patch,
            audience,
        )
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_knowledge_detail(
        &knowledge_detail_from_record(&stored)?,
        json,
    )?))
}

async fn remove_knowledge(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    entry_id: &str,
    expected_revision: u64,
    json: bool,
) -> Result<Option<String>> {
    core.delete_actor_knowledge_entry(
        principal,
        character_id.to_string(),
        entry_id.to_string(),
        revision_i64(expected_revision)?,
    )
    .await
    .map_err(map_core_error)?;
    Ok(json.then(|| "{}".to_string()))
}

async fn list_knowledge(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let page = core
        .list_character_knowledge(
            principal,
            character_id.to_string(),
            resolve_list_limit(limit)?,
            cursor,
        )
        .await
        .map_err(map_core_error)?;
    let resp: ListCharacterKnowledgeResponse = wire_map(knowledge_page_wire_value(&page)?)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No knowledge entries.".to_string()));
    }
    let mut lines: Vec<String> = Vec::with_capacity(resp.items.len() + 1);
    for item in &resp.items {
        lines.push(format!(
            "{}  {}  {}",
            *item.entry_id,
            *item.canonical_name,
            serde_json::to_string(&item.owner)?
        ));
    }
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn view_knowledge(
    core: &CoreService,
    principal: &Principal,
    actor: &str,
    creator_id: Option<String>,
    character_id: Option<String>,
    world_id: String,
    binding_id: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let actor_ref = match actor {
        "creator" => {
            let creator_id = creator_id.ok_or_else(|| {
                CliError::Other("--creator-id is required for --actor creator".into())
            })?;
            serde_json::json!({ "actor_kind": "creator", "creator_id": creator_id })
        }
        "character" => {
            let character_id = character_id.ok_or_else(|| {
                CliError::Other("--character-id is required for --actor character".into())
            })?;
            serde_json::json!({ "actor_kind": "character", "character_id": character_id })
        }
        other => {
            return Err(CliError::Other(format!(
                "unknown --actor {other}; expected creator or character"
            )));
        }
    };
    let mut body = serde_json::json!({
        "actor_ref": actor_ref,
        "world_id": world_id,
    });
    if let Some(id) = binding_id {
        body["binding_id"] = serde_json::Value::String(id);
    }
    if let Some(c) = cursor {
        body["cursor"] = serde_json::Value::String(c);
    }
    let req: ViewRequest = serde_json::from_value(body)?;
    // The read policy is chosen by the core from the admitted actor kind: a
    // Creator viewpoint is the management review, a Character viewpoint stays
    // the strict holder-filtered selection. This leaf selects neither.
    let query = ActorKnowledgeViewQuery {
        world_id: req.world_id.to_string(),
        binding_id: req.binding_id.as_ref().map(|id| id.as_str().to_string()),
        limit: resolve_list_limit(limit)?,
        cursor: req.cursor.clone(),
    };
    let admitted = admitted_from_ref(&req.actor_ref);
    let page = core
        .actor_knowledge_view(principal, &admitted, query)
        .await
        .map_err(map_core_error)?;
    let resp: ViewResponse = wire_map(knowledge_page_wire_value(&page)?)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No knowledge entries.".to_string()));
    }
    let mut lines: Vec<String> = Vec::with_capacity(resp.items.len() + 1);
    for item in &resp.items {
        // Durable §7: the projection carries the native governance pair, never
        // the retired `creator_only` boolean — shared is the *absence* of
        // disclosure.
        let governance = item
            .disclosure
            .as_ref()
            .map_or_else(String::new, |disclosure| {
                format!(
                    "  disclosure={disclosure} holder={}",
                    item.holder_entry_id
                        .as_deref()
                        .map_or("(none)", std::ops::Deref::deref)
                )
            });
        lines.push(format!(
            "{}  {}  {}{governance}",
            *item.entry_id,
            *item.canonical_name,
            serde_json::to_string(&item.owner)?
        ));
    }
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

// ─── Character SOUL/Memory helpers (v1.184 P3) ─────────────────────────────
//
// Every memory arm is one core bearer operation behind the Character's
// activity lease: capture, the retained pending reads/dismiss, the bounded
// review drain, fragments and the revision-checked promote. This leaf only
// resolves the caller's page inputs and renders the returned DTO — it never
// re-implements queue semantics, and the writer was released before any line
// is printed.

/// The retained capture projection (the `CaptureCharacterPendingReviewResponse`).
fn render_character_capture(
    resp: &CaptureCharacterPendingReviewResponse,
    json: bool,
) -> Result<String> {
    if json {
        return Ok(serde_json::to_string_pretty(resp)?);
    }
    Ok(format!(
        "Captured pending review:\n  pending_id: {}",
        *resp.pending_id
    ))
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn memory_capture(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    pending_id: &str,
    session_id: &str,
    binding_id: Option<String>,
    task_kind: Option<String>,
    digest: String,
    created_at: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let mut body = serde_json::json!({
        "pending_id": pending_id,
        "session_id": session_id,
        "raw_digest": digest,
    });
    if let Some(b) = binding_id {
        body["binding_id"] = serde_json::Value::String(b);
    }
    if let Some(k) = task_kind {
        body["task_kind"] = serde_json::Value::String(k);
    }
    if let Some(t) = created_at {
        body["created_at"] = serde_json::Value::String(t);
    }
    let req: CaptureCharacterPendingReviewRequest = serde_json::from_value(body)?;
    let resp = core
        .capture_character_pending_review(principal, character_id.to_string(), req)
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_character_capture(&resp, json)?))
}

async fn memory_pending_list(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .list_character_pending_reviews(
            principal,
            character_id.to_string(),
            binding_id,
            resolve_list_limit(limit)?,
            decode_list_cursor(cursor.as_deref())?,
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No pending reviews.".to_string()));
    }
    let mut lines: Vec<String> = Vec::with_capacity(resp.items.len() + 1);
    for r in &resp.items {
        if let Some(b) = r.binding_id.as_deref() {
            lines.push(format!("{}  {}  binding={}", *r.pending_id, *r.task_kind, b));
        } else {
            lines.push(format!("{}  {}  shared", *r.pending_id, *r.task_kind));
        }
    }
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

async fn memory_pending_count(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .count_character_pending_reviews(principal, character_id.to_string(), binding_id)
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!("{} pending review(s).", resp.count)))
}

async fn memory_pending_dismiss(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    pending_id: &str,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .delete_character_pending_review(
            principal,
            character_id.to_string(),
            pending_id.to_string(),
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.success {
        Ok(Some(format!("Pending review '{pending_id}' dismissed.")))
    } else {
        Ok(Some(format!("Dismiss did not succeed for '{pending_id}'.")))
    }
}

/// Drain one bounded batch; loops while `has_more` (cap 100 calls, stops on
/// zero-progress) mirroring `creator memory review`.
const CHARACTER_REVIEW_DRAIN_MAX_CALLS: u32 = 100;

async fn memory_review(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let mut promoted: i64 = 0;
    let mut fragmented: i64 = 0;
    let mut dropped: i64 = 0;
    let mut processed: i64 = 0;
    let mut has_more = false;
    let mut stopped_zero_progress = false;
    let mut cap_exhausted = false;
    for call in 0..CHARACTER_REVIEW_DRAIN_MAX_CALLS {
        let mut body = serde_json::json!({});
        if let Some(b) = binding_id.clone() {
            body["binding_id"] = serde_json::Value::String(b);
        }
        let req: ReviewCharacterMemoryRequest = serde_json::from_value(body)?;
        let resp = core
            .review_character_memory(principal, character_id.to_string(), req)
            .await
            .map_err(map_core_error)?;
        promoted += resp.promoted;
        fragmented += resp.fragmented;
        dropped += resp.dropped;
        processed += resp.processed.unwrap_or(0);
        has_more = resp.has_more.unwrap_or(false);
        if !has_more {
            break;
        }
        if resp.processed.unwrap_or(0) == 0 {
            stopped_zero_progress = true;
            break;
        }
        if call + 1 >= CHARACTER_REVIEW_DRAIN_MAX_CALLS {
            // Call cap reached with more rows reported by the core.
            has_more = true;
            cap_exhausted = true;
            break;
        }
    }
    if json {
        return Ok(Some(serde_json::to_string_pretty(&serde_json::json!({
            "promoted": promoted,
            "fragmented": fragmented,
            "dropped": dropped,
            "processed": processed,
            "has_more": has_more,
            "stopped_zero_progress": stopped_zero_progress,
            "cap_exhausted": cap_exhausted,
        }))?));
    }
    if processed == 0 && !has_more {
        return Ok(Some("No pending memories to review.".to_string()));
    }
    let mut lines = vec![format!(
        "Review completed: promoted={promoted}, fragmented={fragmented}, dropped={dropped}"
    )];
    if stopped_zero_progress {
        lines.push(
            "Note: a review call made zero progress but the core still reported \
             `has_more`; the queue may contain an unprocessable head row. Re-run \
             `creator character memory review` to retry."
                .to_string(),
        );
    } else if has_more {
        lines.push(format!(
            "Note: the queue was not fully drained within {CHARACTER_REVIEW_DRAIN_MAX_CALLS} calls; \
             re-run `creator character memory review` to continue."
        ));
    }
    Ok(Some(lines.join("\n")))
}

async fn memory_fragments(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .list_character_memory_fragments(
            principal,
            character_id.to_string(),
            binding_id,
            resolve_list_limit(limit)?,
            decode_list_cursor(cursor.as_deref())?,
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.fragments.is_empty() {
        return Ok(Some("No memory fragments found.".to_string()));
    }
    let mut lines: Vec<String> = Vec::with_capacity(resp.fragments.len() + 1);
    for f in &resp.fragments {
        let scope = f
            .binding_id
            .as_deref()
            .map_or("shared", std::string::String::as_str);
        lines.push(format!("{}  {}  {}", *f.fragment_id, scope, &*f.summary));
    }
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

async fn memory_promote(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    fragment_id: &str,
    expected_revision: u64,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .promote_character_fragment(
            principal,
            character_id.to_string(),
            fragment_id.to_string(),
            revision_i64(expected_revision)?,
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!(
        "Promoted fragment {} to shared (revision {}).",
        *resp.fragment.fragment_id, resp.fragment.revision
    )))
}

// ─── Character ToM helpers (v1.184 P4) ─────────────────────────────────────

fn merge_tom_json_field(body: &mut serde_json::Value, key: &str, value: Option<String>) {
    if let Some(v) = value {
        body[key] = serde_json::Value::String(v);
    }
}

/// The retained record projection (the `RecordCharacterTomResponse`).
fn render_tom_record(resp: &RecordCharacterTomResponse, json: bool) -> Result<String> {
    if json {
        return Ok(serde_json::to_string_pretty(resp)?);
    }
    Ok(format!(
        "Recorded ToM belief:\n  carrier_entry_id: {}\n  revision: {}\n  mind_state_id: {}",
        &*resp.carrier_entry_id, resp.revision, &*resp.mind_state_id
    ))
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn tom_record(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    world_id: String,
    binding_id: String,
    carrier_entry_id: String,
    expected_revision: u64,
    holder: String,
    proposition: String,
    order: i64,
    truth: Option<String>,
    access: Option<String>,
    representation: Option<String>,
    content_type: Option<String>,
    source: Option<String>,
    context: Option<String>,
    occurred_at: Option<String>,
    sort_key: Option<String>,
    event_id: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let mut body = serde_json::json!({
        "world_id": world_id,
        "binding_id": binding_id,
        "carrier_entry_id": carrier_entry_id,
        "expected_revision": expected_revision,
        "holder": holder,
        "proposition": proposition,
        "order": order,
    });
    merge_tom_json_field(&mut body, "truth", truth);
    merge_tom_json_field(&mut body, "access", access);
    merge_tom_json_field(&mut body, "representation", representation);
    merge_tom_json_field(&mut body, "content_type", content_type);
    merge_tom_json_field(&mut body, "source", source);
    merge_tom_json_field(&mut body, "context", context);
    merge_tom_json_field(&mut body, "occurred_at", occurred_at);
    merge_tom_json_field(&mut body, "sort_key", sort_key);
    merge_tom_json_field(&mut body, "event_id", event_id);
    let req: RecordCharacterTomRequest = serde_json::from_value(body)?;
    let resp = core
        .record_character_tom(principal, character_id.to_string(), req)
        .await
        .map_err(map_core_error)?;
    Ok(Some(render_tom_record(&resp, json)?))
}

async fn tom_show(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let mut body = serde_json::json!({
        "world_id": world_id,
        "binding_id": binding_id,
    });
    if let Some(n) = limit {
        body["limit"] = serde_json::json!(n);
    }
    if let Some(c) = cursor {
        body["cursor"] = serde_json::Value::String(c);
    }
    // The core owns the page bounds and the opaque keyset cursor: this leaf
    // forwards them and never re-derives the keyset order.
    let query: ListCharacterTomQuery = serde_json::from_value(body)?;
    let resp = core
        .list_character_tom(principal, character_id.to_string(), query)
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No ToM beliefs.".to_string()));
    }
    let mut l1 = Vec::new();
    let mut l2 = Vec::new();
    for row in &resp.items {
        let line = format_tom_item_human(row);
        match row.order {
            1 => l1.push(line),
            2 => l2.push(line),
            _ => {}
        }
    }
    let mut lines = vec!["## Character ToM — L1".to_string()];
    if l1.is_empty() {
        lines.push(String::new());
    } else {
        lines.extend(l1);
    }
    lines.push("## Character ToM — L2".to_string());
    if l2.is_empty() {
        lines.push(String::new());
    } else {
        lines.extend(l2);
    }
    if resp.pagination.has_more {
        if let Some(next) = &resp.pagination.next_cursor {
            lines.push(format!("next_cursor: {next}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

fn format_tom_item_human(row: &NexusCharacterTomBeliefItem) -> String {
    let holder = row.holder.to_string();
    let proposition = row.proposition.as_deref().unwrap_or("");
    let truth = row
        .truth
        .as_ref()
        .map_or_else(|| "Unknown".to_string(), std::string::ToString::to_string);
    format!(
        "- [{}] holder={} truth={} {} (carrier={})",
        row.order, holder, truth, proposition, &*row.carrier_entry_id
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod holder_public_tests {
    use super::{audience_wire, refuse_legacy_creator_only_flag};

    // v1.191 P1 T9 (durable §5/§6): the CLI request surface for the shipped
    // `creator character knowledge` authoring commands.

    #[test]
    fn v1191_holder_public_audience_flag_maps_the_frozen_wire_object() {
        assert_eq!(audience_wire(None, None).unwrap(), None);
        assert_eq!(
            audience_wire(Some("shared"), None).unwrap(),
            Some(serde_json::json!({ "kind": "shared" }))
        );
        assert_eq!(
            audience_wire(Some("author-only"), None).unwrap(),
            Some(serde_json::json!({ "kind": "author-only" }))
        );
        assert_eq!(
            audience_wire(Some("character-private"), Some("chr_1")).unwrap(),
            Some(serde_json::json!({ "kind": "character-private", "character_id": "chr_1" }))
        );
    }

    #[test]
    fn v1191_holder_public_audience_flag_rejects_incomplete_or_unknown_values() {
        assert!(audience_wire(Some("character-private"), None).is_err());
        assert!(audience_wire(None, Some("chr_1")).is_err());
        assert!(audience_wire(Some("owner-private"), Some("chr_1")).is_err());
        assert!(audience_wire(Some("shared"), Some("chr_1")).is_err());
    }

    #[test]
    fn v1191_holder_public_legacy_creator_only_flag_is_refused_by_presence() {
        // `--creator-only` and `--creator-only=false` are both refused; only
        // an absent flag is accepted.
        for present in [Some(true), Some(false)] {
            let err = refuse_legacy_creator_only_flag(present).unwrap_err();
            assert!(
                err.to_string().contains("legacy_creator_only_unsupported"),
                "{err}"
            );
        }
        assert!(refuse_legacy_creator_only_flag(None).is_ok());
    }
}
