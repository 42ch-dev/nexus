//! `creator character` — thin DaemonClient surface for Character identity and bindings.

use crate::api::DaemonClient;
use crate::commands::creator::work_utils::query_path;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    add_character_binding_response::AddCharacterBindingResponse,
    character_detail::CharacterDetail, create_character_request::CreateCharacterRequest,
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
        /// Optional WorldSheet KnowledgeEntry id
        #[arg(long)]
        world_sheet_entry_id: Option<String>,
        /// Emit the generated CreateCharacterResponse DTO
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

/// Run `creator character`.
///
/// # Errors
///
/// Returns daemon/network errors from [`DaemonClient`].
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
        CharacterCommand::Show {
            character_id,
            json,
        } => show(&client, &character_id, json).await,
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
        println!("  character_id: {}", &*resp.character.character_id);
        println!("  display_name:  {}", &*resp.character.display_name);
        println!("  binding_id:    {}", &*resp.binding.binding_id);
        println!("  world_id:       {}", &*resp.binding.world_id);
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
                &*item.character_id, &*item.display_name, item.status
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
        println!("character_id: {}", &*c.character_id);
        println!("display_name: {}", &*c.display_name);
        println!("status:       {}", c.status);
        println!("owner:        {}", &*c.owner_creator_id);
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
        println!("  binding_id: {}", &*resp.binding.binding_id);
        println!("  world_id:    {}", &*resp.binding.world_id);
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
            println!("{}  {}  {}", &*item.binding_id, &*item.world_id, item.status);
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
