//! Create one isolated Nexus world for the Trpg-legend standalone probe.
//!
//! Production `nexus-runtime` deliberately refuses to invent narrative
//! worlds. The probe therefore bootstraps its disposable workspace through
//! Nexus' own schema and narrative-write API before opening the runtime.

use anyhow::{anyhow, Context};
use nexus42::config::{resolve_state_db_path_from_home, CliConfig};
use nexus42::db::Schema;
use serde_json::json;
use std::path::PathBuf;

const CREATOR_ID: &str = "ctr_trpg_probe";

fn parse_home() -> anyhow::Result<PathBuf> {
    let mut home = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => home = args.next().map(PathBuf::from),
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    home.ok_or_else(|| anyhow!("--home is required"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let home = parse_home()?;
    let config = CliConfig::load_from_home(&home).context("load explicit-home config")?;
    let db_path = resolve_state_db_path_from_home(&config, &home)
        .context("resolve explicit-home workspace DB")?;
    let pool = Schema::init(&db_path).await.context("initialize workspace DB")?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES (?, 'Trpg Probe', 'active', ?, '{}') \
         ON CONFLICT(creator_id) DO NOTHING",
    )
    .bind(CREATOR_ID)
    .bind(now)
    .execute(&pool)
    .await
    .context("seed probe creator")?;
    let world = nexus_local_db::create_world(
        &pool,
        CREATOR_ID,
        "Trpg Context Probe",
        "trpg-context-probe",
        "private",
        "manual",
    )
    .await
    .context("create isolated probe world")?;
    println!(
        "{}",
        json!({
            "schema_version": 1,
            "world_id": world.world_id,
        })
    );
    Ok(())
}
