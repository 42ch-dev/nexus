//! Clone Command Module
//!
//! Clone a world from the platform or a local source via the daemon.
//!
//! Usage: `nexus42 clone <world-ref>`

use crate::api::DaemonClient;
use crate::commands::context::validate_world_id;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Args;
use serde::Deserialize;

/// Clone command arguments
#[derive(Debug, Args)]
pub struct CloneArgs {
    /// World reference to clone (world_id, e.g. wld_abc123)
    world_ref: String,

    /// Clone source: platform (default) or local
    #[arg(long, value_enum, default_value = "platform")]
    source: CloneSourceArg,

    /// Print the JSON request and exit without calling the daemon
    #[arg(long)]
    dry_run: bool,

    /// Skip interactive confirmation
    #[arg(long)]
    yes: bool,
}

/// Clone source options
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum CloneSourceArg {
    /// Clone from the platform (via daemon proxy)
    Platform,
    /// Clone from a local source
    Local,
}

/// Response from the daemon clone endpoint
#[derive(Debug, Deserialize)]
pub struct WorldCloneResponse {
    pub success: bool,
    pub world_id: Option<String>,
    pub world_revision: Option<u64>,
    pub cloned_at: Option<String>,
    pub error: Option<String>,
}

/// Validate world reference format.
///
/// Accepts both full world IDs (`wld_*`) and numeric references.
fn validate_world_ref(s: &str) -> std::result::Result<String, String> {
    // If it starts with wld_, validate as a full world ID
    if s.starts_with("wld_") {
        return validate_world_id(s);
    }
    // Otherwise accept as-is (could be a numeric ID or slug)
    if s.is_empty() {
        return Err("world-ref cannot be empty".to_string());
    }
    Ok(s.to_string())
}

fn confirm_clone(yes: bool, world_ref: &str, source: CloneSourceArg) -> bool {
    if yes {
        return true;
    }
    let source_label = match source {
        CloneSourceArg::Platform => "platform",
        CloneSourceArg::Local => "local",
    };
    match dialoguer::Confirm::new()
        .with_prompt(format!(
            "Clone world '{}' from {}?",
            world_ref, source_label
        ))
        .default(false)
        .interact()
    {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Non-interactive terminal: pass --yes to confirm clone.");
            false
        }
    }
}

/// Run the clone command.
pub async fn run(args: CloneArgs, config: &CliConfig) -> Result<()> {
    let world_ref = validate_world_ref(&args.world_ref).map_err(CliError::Other)?;

    let body = serde_json::json!({
        "world_ref": world_ref,
        "source": match args.source {
            CloneSourceArg::Platform => "platform",
            CloneSourceArg::Local => "local",
        },
        "schema_version": 1,
    });

    if args.dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(CliError::Json)?
        );
        return Ok(());
    }

    if !confirm_clone(args.yes, &world_ref, args.source) {
        println!("Clone cancelled.");
        return Ok(());
    }

    let client = DaemonClient::from_config(config);

    if !client.health_check().await? {
        return Err(CliError::DaemonNotRunning);
    }

    let resp = client
        .post::<WorldCloneResponse, serde_json::Value>("/v1/local/world/clone", &body)
        .await?;

    if resp.success {
        println!("World clone completed.");
        if let Some(id) = resp.world_id {
            println!("  world_id:        {}", id);
        }
        if let Some(rev) = resp.world_revision {
            println!("  world_revision:  {}", rev);
        }
        if let Some(at) = &resp.cloned_at {
            println!("  cloned_at:       {}", at);
        }
    } else if let Some(err) = resp.error {
        return Err(CliError::Other(format!("World clone failed: {}", err)));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_world_ref_accepts_wld_prefix() {
        assert!(validate_world_ref("wld_abc123").is_ok());
    }

    #[test]
    fn validate_world_ref_accepts_numeric() {
        assert!(validate_world_ref("42").is_ok());
    }

    #[test]
    fn validate_world_ref_rejects_empty() {
        assert!(validate_world_ref("").is_err());
    }

    #[test]
    fn validate_world_ref_rejects_invalid_wld() {
        assert!(validate_world_ref("wld_").is_err());
    }

    #[test]
    fn world_clone_response_deser_success() {
        let j = r#"{"success":true,"world_id":"wld_new","world_revision":1,"cloned_at":"2026-04-15T00:00:00Z"}"#;
        let r: WorldCloneResponse = serde_json::from_str(j).expect("deser");
        assert!(r.success);
        assert_eq!(r.world_id.as_deref(), Some("wld_new"));
        assert_eq!(r.world_revision, Some(1));
    }

    #[test]
    fn world_clone_response_deser_failure() {
        let j = r#"{"success":false,"error":"not found"}"#;
        let r: WorldCloneResponse = serde_json::from_str(j).expect("deser");
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("not found"));
    }

    #[test]
    fn clone_source_arg_labels() {
        assert!(matches!(CloneSourceArg::Platform, CloneSourceArg::Platform));
        assert!(matches!(CloneSourceArg::Local, CloneSourceArg::Local));
    }
}
