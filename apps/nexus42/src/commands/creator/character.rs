//! `creator character` — thin `DaemonClient` surface for Character identity and bindings.

use crate::api::DaemonClient;
use crate::commands::creator::work_utils::query_path;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::actor_knowledge::{
    add_knowledge_entry_request::AddKnowledgeEntryRequest,
    add_knowledge_entry_response::AddKnowledgeEntryResponse,
    list_character_knowledge_response::ListCharacterKnowledgeResponse, view_request::ViewRequest,
    view_response::ViewResponse,
};
use nexus_contracts::daemon_api::agent_host::{
    create_session_request::CreateSessionRequest,
    execute_operation_request::ExecuteOperationRequest, operation_response::OperationResponse,
    session_response::SessionResponse,
};
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_response::CaptureCharacterPendingReviewResponse;
use nexus_contracts::daemon_api::characters::memory::count_character_pending_reviews_response::CountCharacterPendingReviewsResponse;
use nexus_contracts::daemon_api::characters::memory::delete_character_pending_review_response::DeleteCharacterPendingReviewResponse;
use nexus_contracts::daemon_api::characters::memory::list_character_memory_fragments_response::ListCharacterMemoryFragmentsResponse;
use nexus_contracts::daemon_api::characters::memory::list_character_pending_reviews_response::ListCharacterPendingReviewsResponse;
use nexus_contracts::daemon_api::characters::memory::promote_character_fragment_request::PromoteCharacterFragmentRequest;
use nexus_contracts::daemon_api::characters::memory::promote_character_fragment_response::PromoteCharacterFragmentResponse;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_response::ReviewCharacterMemoryResponse;
use nexus_contracts::daemon_api::characters::soul::character_soul_narrative_request::CharacterSoulNarrativeRequest;
use nexus_contracts::daemon_api::characters::soul::character_soul_narrative_response::CharacterSoulNarrativeResponse;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_response::ListCharacterTomResponse;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_response::NexusCharacterTomBeliefItem;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_response::RecordCharacterTomResponse;
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    add_character_binding_response::AddCharacterBindingResponse, character_detail::CharacterDetail,
    create_character_request::CreateCharacterRequest,
    create_character_response::CreateCharacterResponse,
    list_character_bindings_response::ListCharacterBindingsResponse,
    list_characters_response::ListCharactersResponse,
};

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
    /// Character SOUL narrative operations (v1.184 P3)
    Soul {
        #[command(subcommand)]
        command: CharacterSoulCommand,
    },
    /// Character `ToM` L1/L2 record and show (v1.184 P4)
    Tom {
        #[command(subcommand)]
        command: CharacterTomCommand,
    },
    /// Run a Character prompt through the existing Agent Host
    Run {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        world_id: String,
        #[arg(long)]
        binding_id: String,
        /// User prompt submitted as one `HostOperation::Prompt`
        #[arg(long)]
        prompt: String,
        /// Provider id (deterministic mock in tests)
        #[arg(long, default_value = "mock-provider")]
        provider_id: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        branch_id: Option<String>,
        #[arg(long)]
        event_id: Option<String>,
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
        #[arg(long, default_value_t = false)]
        creator_only: bool,
        #[arg(long)]
        block_type: String,
        #[arg(long)]
        canonical_name: String,
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

/// `creator character soul` subcommands (v1.184 P3).
#[derive(Debug, Subcommand)]
pub enum CharacterSoulCommand {
    /// Read or regenerate the Character SOUL narrative
    Reflect {
        #[arg(long)]
        character_id: String,
        #[arg(long)]
        binding_id: Option<String>,
        /// Force on-demand synthesis (registers a synthesizer)
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run `creator character`.
///
/// # Errors
///
/// Returns daemon/network errors from [`DaemonClient`].
#[allow(clippy::too_many_lines)] // single CLI command dispatcher
pub async fn run(cmd: CharacterCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
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
                &client,
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
        } => list(&client, limit, cursor, json).await,
        CharacterCommand::Show { character_id, json } => show(&client, &character_id, json).await,
        CharacterCommand::Binding { command } => match command {
            BindingCommand::Add {
                character_id,
                world_id,
                world_sheet_entry_id,
                json,
            } => add_binding(&client, &character_id, world_id, world_sheet_entry_id, json).await,
            BindingCommand::List {
                character_id,
                limit,
                cursor,
                json,
            } => list_bindings(&client, &character_id, limit, cursor, json).await,
            BindingCommand::Remove {
                character_id,
                binding_id,
                json,
            } => remove_binding(&client, &character_id, &binding_id, json).await,
        },
        CharacterCommand::Knowledge { command } => match command {
            KnowledgeCommand::Add {
                owner,
                world_id,
                character_id,
                binding_id,
                creator_only,
                block_type,
                canonical_name,
                json,
            } => {
                add_knowledge(
                    &client,
                    &owner,
                    world_id,
                    character_id,
                    binding_id,
                    creator_only,
                    block_type,
                    canonical_name,
                    json,
                )
                .await
            }
            KnowledgeCommand::List {
                character_id,
                limit,
                cursor,
                json,
            } => list_knowledge(&client, &character_id, limit, cursor, json).await,
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
                    &client,
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
                    &client,
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
            } => memory_pending_list(&client, &character_id, binding_id, limit, cursor, json).await,
            CharacterMemoryCommand::PendingCount {
                character_id,
                binding_id,
                json,
            } => memory_pending_count(&client, &character_id, binding_id, json).await,
            CharacterMemoryCommand::PendingDismiss {
                character_id,
                pending_id,
                json,
            } => memory_pending_dismiss(&client, &character_id, &pending_id, json).await,
            CharacterMemoryCommand::Review {
                character_id,
                binding_id,
                json,
            } => memory_review(&client, &character_id, binding_id, json).await,
            CharacterMemoryCommand::Fragments {
                character_id,
                binding_id,
                limit,
                cursor,
                json,
            } => memory_fragments(&client, &character_id, binding_id, limit, cursor, json).await,
            CharacterMemoryCommand::Promote {
                character_id,
                fragment_id,
                expected_revision,
                json,
            } => {
                memory_promote(
                    &client,
                    &character_id,
                    &fragment_id,
                    expected_revision,
                    json,
                )
                .await
            }
        },
        CharacterCommand::Soul { command } => match command {
            CharacterSoulCommand::Reflect {
                character_id,
                binding_id,
                force,
                json,
            } => soul_reflect(&client, &character_id, binding_id, force, json).await,
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
                    &client,
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
                    &client,
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
        CharacterCommand::Run {
            character_id,
            world_id,
            binding_id,
            prompt,
            provider_id,
            cwd,
            model,
            mode,
            branch_id,
            event_id,
            json,
        } => {
            run_character(
                &client,
                character_id,
                world_id,
                binding_id,
                prompt,
                provider_id,
                cwd,
                model,
                mode,
                branch_id,
                event_id,
                json,
            )
            .await
        }
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

async fn create(
    client: &DaemonClient,
    display_name: String,
    world_id: String,
    image_uri: Option<String>,
    persona: Option<String>,
    world_sheet_entry_id: Option<String>,
    json: bool,
) -> Result<()> {
    let mut body = serde_json::json!({
        "display_name": display_name,
        "world_id": world_id,
        "persona": parse_persona(persona)?,
    });
    if let Some(uri) = image_uri {
        body["image_uri"] = serde_json::Value::String(uri);
    }
    if let Some(sheet) = world_sheet_entry_id {
        body["world_sheet_entry_id"] = serde_json::Value::String(sheet);
    }
    let req: CreateCharacterRequest = serde_json::from_value(body)?;
    let resp: CreateCharacterResponse = client.post("/v1/daemon/characters", &req).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Character created:");
        println!("  character_id: {}", *resp.character.character_id);
        println!("  display_name:  {}", *resp.character.display_name);
        println!("  binding_id:    {}", *resp.binding.binding_id);
        println!("  world_id:       {}", *resp.binding.world_id);
    }
    Ok(())
}

async fn list(
    client: &DaemonClient,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
    let mut pairs = Vec::new();
    let limit_owned = limit.map(|n| n.to_string());
    if let Some(ref n) = limit_owned {
        pairs.push(("limit", n.as_str()));
    }
    if let Some(ref c) = cursor {
        pairs.push(("cursor", c.as_str()));
    }
    let path = query_path("/v1/daemon/characters", &pairs);
    let resp: ListCharactersResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No characters.");
    } else {
        for item in &resp.items {
            println!(
                "{}  {}  {}",
                *item.character_id, *item.display_name, item.status
            );
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
}

async fn show(client: &DaemonClient, character_id: &str, json: bool) -> Result<()> {
    let resp: CharacterDetail = client
        .get(&format!("/v1/daemon/characters/{character_id}"))
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        let c = &resp.character;
        println!("character_id: {}", *c.character_id);
        println!("display_name: {}", *c.display_name);
        println!("status:       {}", c.status);
        println!("owner:        {}", *c.owner_creator_id);
    }
    Ok(())
}

async fn add_binding(
    client: &DaemonClient,
    character_id: &str,
    world_id: String,
    world_sheet_entry_id: Option<String>,
    json: bool,
) -> Result<()> {
    let mut body = serde_json::json!({ "world_id": world_id });
    if let Some(sheet) = world_sheet_entry_id {
        body["world_sheet_entry_id"] = serde_json::Value::String(sheet);
    }
    let req: AddCharacterBindingRequest = serde_json::from_value(body)?;
    let resp: AddCharacterBindingResponse = client
        .post(
            &format!("/v1/daemon/characters/{character_id}/bindings"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Binding added:");
        println!("  binding_id: {}", *resp.binding.binding_id);
        println!("  world_id:    {}", *resp.binding.world_id);
    }
    Ok(())
}

async fn list_bindings(
    client: &DaemonClient,
    character_id: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
    let mut pairs = Vec::new();
    let limit_owned = limit.map(|n| n.to_string());
    if let Some(ref n) = limit_owned {
        pairs.push(("limit", n.as_str()));
    }
    if let Some(ref c) = cursor {
        pairs.push(("cursor", c.as_str()));
    }
    let path = query_path(
        &format!("/v1/daemon/characters/{character_id}/bindings"),
        &pairs,
    );
    let resp: ListCharacterBindingsResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No bindings.");
    } else {
        for item in &resp.items {
            println!("{}  {}  {}", *item.binding_id, *item.world_id, item.status);
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
}

async fn remove_binding(
    client: &DaemonClient,
    character_id: &str,
    binding_id: &str,
    json: bool,
) -> Result<()> {
    client
        .delete_no_content(&format!(
            "/v1/daemon/characters/{character_id}/bindings/{binding_id}"
        ))
        .await?;
    if json {
        println!("{{}}");
    } else {
        println!("Binding {binding_id} removed.");
    }
    Ok(())
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
    client: &DaemonClient,
    owner: &str,
    world_id: Option<String>,
    character_id: Option<String>,
    binding_id: Option<String>,
    creator_only: bool,
    block_type: String,
    canonical_name: String,
    json: bool,
) -> Result<()> {
    let mut body = serde_json::json!({
        "owner_kind": owner_kind_wire(owner)?,
        "block_type": block_type,
        "canonical_name": canonical_name,
        "creator_only": creator_only,
    });
    if let Some(id) = world_id {
        body["world_id"] = serde_json::Value::String(id);
    }
    if let Some(id) = character_id {
        body["character_id"] = serde_json::Value::String(id);
    }
    if let Some(id) = binding_id {
        body["binding_id"] = serde_json::Value::String(id);
    }
    let req: AddKnowledgeEntryRequest = serde_json::from_value(body)?;
    let resp: AddKnowledgeEntryResponse = client
        .post("/v1/daemon/actor-knowledge/entries", &req)
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("KnowledgeEntry added:");
        println!("  entry_id: {}", *resp.item.entry_id);
        println!("  owner:    {}", serde_json::to_string(&resp.item.owner)?);
    }
    Ok(())
}

async fn list_knowledge(
    client: &DaemonClient,
    character_id: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
    let mut pairs = Vec::new();
    let limit_owned = limit.map(|n| n.to_string());
    if let Some(ref n) = limit_owned {
        pairs.push(("limit", n.as_str()));
    }
    if let Some(ref c) = cursor {
        pairs.push(("cursor", c.as_str()));
    }
    let path = query_path(
        &format!("/v1/daemon/characters/{character_id}/knowledge"),
        &pairs,
    );
    let resp: ListCharacterKnowledgeResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No knowledge entries.");
    } else {
        for item in &resp.items {
            println!(
                "{}  {}  {}",
                *item.entry_id,
                *item.canonical_name,
                serde_json::to_string(&item.owner)?
            );
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn view_knowledge(
    client: &DaemonClient,
    actor: &str,
    creator_id: Option<String>,
    character_id: Option<String>,
    world_id: String,
    binding_id: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
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
    if let Some(n) = limit {
        body["limit"] = serde_json::Value::Number(n.into());
    }
    if let Some(c) = cursor {
        body["cursor"] = serde_json::Value::String(c);
    }
    let req: ViewRequest = serde_json::from_value(body)?;
    let resp: ViewResponse = client.post("/v1/daemon/actor-knowledge/view", &req).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No knowledge entries.");
    } else {
        for item in &resp.items {
            println!(
                "{}  {}  {}  creator_only={}",
                *item.entry_id,
                *item.canonical_name,
                serde_json::to_string(&item.owner)?,
                item.creator_only
            );
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn run_character(
    client: &DaemonClient,
    character_id: String,
    world_id: String,
    binding_id: String,
    prompt: String,
    provider_id: String,
    cwd: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    branch_id: Option<String>,
    event_id: Option<String>,
    json: bool,
) -> Result<()> {
    let mut body = serde_json::json!({
        "provider_id": provider_id,
        "actor_ref": {
            "actor_kind": "character",
            "character_id": character_id,
        },
        "viewpoint": {
            "world_id": world_id,
            "binding_id": binding_id,
        },
    });
    if let Some(cwd) = cwd {
        body["cwd"] = serde_json::Value::String(cwd);
    }
    if let Some(model) = model {
        body["model"] = serde_json::Value::String(model);
    }
    if let Some(mode) = mode {
        body["mode"] = serde_json::Value::String(mode);
    }
    if let Some(branch_id) = branch_id {
        body["viewpoint"]["branch_id"] = serde_json::Value::String(branch_id);
    }
    if let Some(event_id) = event_id {
        body["viewpoint"]["event_id"] = serde_json::Value::String(event_id);
    }
    let req: CreateSessionRequest = serde_json::from_value(body)?;
    let session: SessionResponse = client.post("/v1/daemon/agent-host/sessions", &req).await?;

    let mut events = client
        .stream_get(&format!(
            "/v1/daemon/agent-host/sessions/{}/events",
            session.session_id
        ))
        .await?;

    let op_req = ExecuteOperationRequest::Prompt { content: prompt };
    let operation: OperationResponse = client
        .post(
            &format!(
                "/v1/daemon/agent-host/sessions/{}/operations",
                session.session_id
            ),
            &op_req,
        )
        .await?;

    let (result, event_values) = consume_terminal_events(&mut events).await?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "session": session,
                "operation": operation,
                "result": result,
                "events": event_values,
            }))?
        );
    } else {
        println!("session_id:   {}", session.session_id);
        println!("provider_id:  {}", session.provider_id);
        if let Some(actor) = session.actor_ref.as_ref() {
            println!(
                "actor_ref:    {}",
                serde_json::to_string(actor).unwrap_or_default()
            );
        }
        if let Some(viewpoint) = session.viewpoint.as_ref() {
            println!(
                "viewpoint:    {}",
                serde_json::to_string(viewpoint).unwrap_or_default()
            );
        }
        println!("operation_id: {}", operation.operation_id);
        println!("result:");
        println!("{result}");
    }
    Ok(())
}

// ─── Character SOUL/Memory helpers (v1.184 P3) ─────────────────────────────

fn character_memory_base(character_id: &str) -> String {
    format!("/v1/daemon/characters/{character_id}/memory")
}

fn binding_pairs(
    binding_id: Option<&str>,
    limit: Option<i64>,
    cursor: Option<&str>,
) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if let Some(b) = binding_id {
        pairs.push(("binding_id".to_string(), b.to_string()));
    }
    if let Some(n) = limit {
        pairs.push(("limit".to_string(), n.to_string()));
    }
    if let Some(c) = cursor {
        pairs.push(("cursor".to_string(), c.to_string()));
    }
    pairs
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn memory_capture(
    client: &DaemonClient,
    character_id: &str,
    pending_id: &str,
    session_id: &str,
    binding_id: Option<String>,
    task_kind: Option<String>,
    digest: String,
    created_at: Option<String>,
    json: bool,
) -> Result<()> {
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
    let resp: CaptureCharacterPendingReviewResponse = client
        .post(
            &format!("{}/pending-review", character_memory_base(character_id)),
            &req,
        )
        .await?;
    print_character_capture(&resp, json);
    Ok(())
}

fn print_character_capture(resp: &CaptureCharacterPendingReviewResponse, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(resp).unwrap_or_default());
    } else {
        println!("Captured pending review:");
        println!("  pending_id: {}", *resp.pending_id);
    }
}

async fn memory_pending_list(
    client: &DaemonClient,
    character_id: &str,
    binding_id: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
    let pairs = binding_pairs(binding_id.as_deref(), limit, cursor.as_deref());
    let path = query_path(
        &format!("{}/pending-review", character_memory_base(character_id)),
        &pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    );
    let resp: ListCharacterPendingReviewsResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No pending reviews.");
    } else {
        for r in &resp.items {
            if let Some(b) = r.binding_id.as_deref() {
                println!("{}  {}  binding={}", *r.pending_id, *r.task_kind, b);
            } else {
                println!("{}  {}  shared", *r.pending_id, *r.task_kind);
            }
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
}

async fn memory_pending_count(
    client: &DaemonClient,
    character_id: &str,
    binding_id: Option<String>,
    json: bool,
) -> Result<()> {
    let mut path = format!(
        "{}/pending-review/count",
        character_memory_base(character_id)
    );
    if let Some(b) = binding_id {
        path = format!("{path}?binding_id={b}");
    }
    let resp: CountCharacterPendingReviewsResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("{} pending review(s).", resp.count);
    }
    Ok(())
}

async fn memory_pending_dismiss(
    client: &DaemonClient,
    character_id: &str,
    pending_id: &str,
    json: bool,
) -> Result<()> {
    let resp: DeleteCharacterPendingReviewResponse = client
        .delete(&format!(
            "{}/pending-review/{pending_id}",
            character_memory_base(character_id)
        ))
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.success {
        println!("Pending review '{pending_id}' dismissed.");
    } else {
        println!("Dismiss did not succeed for '{pending_id}'.");
    }
    Ok(())
}

/// Drain one bounded batch; loops while `has_more` (cap 100 calls, stops on
/// zero-progress) mirroring `creator memory review`.
const CHARACTER_REVIEW_DRAIN_MAX_CALLS: u32 = 100;

async fn memory_review(
    client: &DaemonClient,
    character_id: &str,
    binding_id: Option<String>,
    json: bool,
) -> Result<()> {
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
        let resp: ReviewCharacterMemoryResponse = client
            .post(
                &format!("{}/review", character_memory_base(character_id)),
                &req,
            )
            .await?;
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
            // Call cap reached with more rows reported by the server.
            has_more = true;
            cap_exhausted = true;
            break;
        }
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "promoted": promoted,
                "fragmented": fragmented,
                "dropped": dropped,
                "processed": processed,
                "has_more": has_more,
                "stopped_zero_progress": stopped_zero_progress,
                "cap_exhausted": cap_exhausted,
            }))?
        );
    } else if processed == 0 && !has_more {
        println!("No pending memories to review.");
    } else {
        println!(
            "Review completed: promoted={promoted}, fragmented={fragmented}, dropped={dropped}"
        );
        if stopped_zero_progress {
            println!(
                "Note: a review call made zero progress but the daemon still reported \
                 `has_more`; the queue may contain an unprocessable head row. Re-run \
                 `creator character memory review` to retry."
            );
        } else if has_more {
            println!(
                "Note: the queue was not fully drained within {CHARACTER_REVIEW_DRAIN_MAX_CALLS} calls; \
                 re-run `creator character memory review` to continue."
            );
        }
    }
    Ok(())
}

async fn memory_fragments(
    client: &DaemonClient,
    character_id: &str,
    binding_id: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
    let pairs = binding_pairs(binding_id.as_deref(), limit, cursor.as_deref());
    let path = query_path(
        &format!("{}/fragments", character_memory_base(character_id)),
        &pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    );
    let resp: ListCharacterMemoryFragmentsResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.fragments.is_empty() {
        println!("No memory fragments found.");
    } else {
        for f in &resp.fragments {
            let scope = f
                .binding_id
                .as_deref()
                .map_or("shared", std::string::String::as_str);
            println!("{}  {}  {}", *f.fragment_id, scope, &*f.summary);
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
}

async fn memory_promote(
    client: &DaemonClient,
    character_id: &str,
    fragment_id: &str,
    expected_revision: u64,
    json: bool,
) -> Result<()> {
    let req: PromoteCharacterFragmentRequest = serde_json::from_value(serde_json::json!({
        "expected_revision": expected_revision,
    }))?;
    let resp: PromoteCharacterFragmentResponse = client
        .post(
            &format!(
                "{}/fragments/{fragment_id}:promote",
                character_memory_base(character_id)
            ),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "Promoted fragment {} to shared (revision {}).",
            *resp.fragment.fragment_id, resp.fragment.revision
        );
    }
    Ok(())
}

async fn soul_reflect(
    client: &DaemonClient,
    character_id: &str,
    binding_id: Option<String>,
    force: bool,
    json: bool,
) -> Result<()> {
    let mut body = serde_json::json!({ "force_regenerate": force });
    if let Some(b) = binding_id {
        body["binding_id"] = serde_json::Value::String(b);
    }
    let req: CharacterSoulNarrativeRequest = serde_json::from_value(body)?;
    let resp: CharacterSoulNarrativeResponse = client
        .post(
            &format!("/v1/daemon/characters/{character_id}/soul/reflect"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("character_id: {}", *resp.character_id);
        println!("state:        {}", resp.state);
        if let Some(n) = resp.narrative.as_deref() {
            println!("narrative:");
            println!("{n}");
        }
    }
    Ok(())
}

// ─── Character ToM helpers (v1.184 P4) ─────────────────────────────────────

fn character_tom_base(character_id: &str) -> String {
    format!("/v1/daemon/characters/{character_id}/tom")
}

fn merge_tom_json_field(body: &mut serde_json::Value, key: &str, value: Option<String>) {
    if let Some(v) = value {
        body[key] = serde_json::Value::String(v);
    }
}

#[allow(clippy::too_many_arguments)] // CLI arg mapping
async fn tom_record(
    client: &DaemonClient,
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
) -> Result<()> {
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
    let resp: RecordCharacterTomResponse =
        client.post(&character_tom_base(character_id), &req).await?;
    print_tom_record(&resp, json);
    Ok(())
}

fn print_tom_record(resp: &RecordCharacterTomResponse, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(resp).unwrap_or_default());
    } else {
        println!("Recorded ToM belief:");
        println!("  carrier_entry_id: {}", &*resp.carrier_entry_id);
        println!("  revision: {}", resp.revision);
        println!("  mind_state_id: {}", &*resp.mind_state_id);
    }
}

async fn tom_show(
    client: &DaemonClient,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    json: bool,
) -> Result<()> {
    let mut pairs: Vec<(&str, String)> = vec![
        ("world_id", world_id.to_string()),
        ("binding_id", binding_id.to_string()),
    ];
    if let Some(n) = limit {
        pairs.push(("limit", n.to_string()));
    }
    if let Some(c) = cursor {
        pairs.push(("cursor", c));
    }
    let path = query_path(
        &character_tom_base(character_id),
        &pairs
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>(),
    );
    let resp: ListCharacterTomResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No ToM beliefs.");
    } else {
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
        println!("## Character ToM — L1");
        if l1.is_empty() {
            println!();
        } else {
            for line in l1 {
                println!("{line}");
            }
        }
        println!("## Character ToM — L2");
        if l2.is_empty() {
            println!();
        } else {
            for line in l2 {
                println!("{line}");
            }
        }
        if resp.pagination.has_more {
            if let Some(next) = &resp.pagination.next_cursor {
                println!("next_cursor: {next}");
            }
        }
    }
    Ok(())
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

async fn consume_terminal_events(
    resp: &mut reqwest::Response,
) -> Result<(String, Vec<serde_json::Value>)> {
    let mut buf = String::new();
    let mut result = String::new();
    let mut events = Vec::new();
    loop {
        let chunk = resp.chunk().await?.ok_or_else(|| {
            CliError::Other("agent-host event stream closed before a terminal event".into())
        })?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(idx) = buf.find("\n\n") {
            let frame = buf[..idx].to_string();
            buf = buf[idx + 2..].to_string();
            let mut data = String::new();
            for line in frame.lines() {
                if let Some(rest) = line.strip_prefix("data:") {
                    data.push_str(rest.trim_start());
                }
            }
            if data.is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(&data)?;
            if let Some(text) = value
                .get("MessageDelta")
                .and_then(|v| v.get("text"))
                .and_then(serde_json::Value::as_str)
            {
                result.push_str(text);
            }
            let terminal_fail = value.get("OpFailed").cloned();
            let finished = value.get("OpFinished").is_some();
            events.push(value);
            if let Some(fail) = terminal_fail {
                let message = fail
                    .get("error_message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("operation failed");
                return Err(CliError::Other(format!(
                    "agent-host operation failed: {message}"
                )));
            }
            if finished {
                return Ok((result, events));
            }
        }
    }
}
