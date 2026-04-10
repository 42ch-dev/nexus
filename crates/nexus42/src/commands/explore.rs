//! Explore browse/search — daemon-mediated read-only platform calls.
//!
//! Requires a running nexus42d and `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN`
//! on the daemon process (same pattern as `world`).

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{ExploreBrowseRequest, ExploreFeedResponse, ExploreSearchRequest};
use serde::Deserialize;

const BROWSE_SCOPES: &[&str] = &["all", "worlds", "creators", "manuscripts"];

fn validate_scope(s: &str) -> std::result::Result<String, String> {
    if BROWSE_SCOPES.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "scope must be one of: {}",
            BROWSE_SCOPES.join(", ")
        ))
    }
}

fn validate_limit(s: &str) -> std::result::Result<i64, String> {
    let n: i64 = s
        .parse()
        .map_err(|_| "limit must be an integer".to_string())?;
    if !(1..=100).contains(&n) {
        return Err("limit must be between 1 and 100".into());
    }
    Ok(n)
}

#[derive(Debug, Subcommand)]
pub enum ExploreCommand {
    /// Directory-style listing (POST /v1/explore/browse)
    Browse {
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, value_parser = validate_limit)]
        limit: Option<i64>,
        #[arg(long, value_parser = validate_scope)]
        scope: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Full-text style query (POST /v1/explore/search)
    Search {
        /// Search query (non-empty)
        query: String,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, value_parser = validate_limit)]
        limit: Option<i64>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct ExploreLocalResponse {
    pub success: bool,
    pub feed: Option<ExploreFeedResponse>,
    pub error: Option<String>,
}

fn is_json_output(output_format: &str) -> bool {
    output_format.eq_ignore_ascii_case("json")
}

fn print_feed_text(feed: &ExploreFeedResponse) -> Result<()> {
    println!(
        "entries: {}, has_more: {}",
        feed.entries.len(),
        feed.has_more
    );
    if let Some(c) = &feed.next_cursor {
        println!("next_cursor: {c}");
    }
    for (i, e) in feed.entries.iter().enumerate() {
        let line = serde_json::to_string(e).map_err(CliError::Json)?;
        println!("  [{i}] {line}");
    }
    Ok(())
}

/// Run explore subcommands
pub async fn run(cmd: ExploreCommand, config: &CliConfig, output_format: &str) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let json_out = is_json_output(output_format);

    match cmd {
        ExploreCommand::Browse {
            cursor,
            limit,
            scope,
            dry_run,
        } => {
            let req = ExploreBrowseRequest {
                schema_version: 1,
                cursor,
                limit,
                scope,
            };

            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&req).map_err(CliError::Json)?
                );
                return Ok(());
            }

            if !client.health_check().await? {
                return Err(CliError::DaemonNotRunning);
            }

            match client
                .post::<ExploreLocalResponse, ExploreBrowseRequest>(
                    "/v1/local/explore/browse",
                    &req,
                )
                .await
            {
                Ok(resp) => {
                    if resp.success {
                        if let Some(feed) = resp.feed {
                            if json_out {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&feed).map_err(CliError::Json)?
                                );
                            } else {
                                print_feed_text(&feed)?;
                            }
                        }
                    } else if let Some(err) = resp.error {
                        eprintln!("Explore browse failed: {err}");
                    }
                }
                Err(e) => {
                    eprintln!("Explore browse request failed: {e}");
                    return Err(e);
                }
            }
        }
        ExploreCommand::Search {
            query,
            cursor,
            limit,
            dry_run,
        } => {
            if query.trim().is_empty() {
                return Err(CliError::Config("search query must not be empty".into()));
            }

            let req = ExploreSearchRequest {
                schema_version: 1,
                query: query.clone(),
                cursor,
                limit,
            };

            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&req).map_err(CliError::Json)?
                );
                return Ok(());
            }

            if !client.health_check().await? {
                return Err(CliError::DaemonNotRunning);
            }

            match client
                .post::<ExploreLocalResponse, ExploreSearchRequest>(
                    "/v1/local/explore/search",
                    &req,
                )
                .await
            {
                Ok(resp) => {
                    if resp.success {
                        if let Some(feed) = resp.feed {
                            if json_out {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&feed).map_err(CliError::Json)?
                                );
                            } else {
                                print_feed_text(&feed)?;
                            }
                        }
                    } else if let Some(err) = resp.error {
                        eprintln!("Explore search failed: {err}");
                    }
                }
                Err(e) => {
                    eprintln!("Explore search request failed: {e}");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_local_response_deser() {
        let j = r#"{"success":true,"feed":{"schema_version":1,"entries":[],"has_more":false},"error":null}"#;
        let r: ExploreLocalResponse = serde_json::from_str(j).unwrap();
        assert!(r.success);
        assert!(r.feed.is_some());
    }

    #[test]
    fn validate_limit_bounds() {
        assert!(validate_limit("0").is_err());
        assert!(validate_limit("101").is_err());
        assert_eq!(validate_limit("50").unwrap(), 50);
    }
}
