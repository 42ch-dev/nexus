//! `game_bible.project_scaffold` capability.
//!
//! V1.54 P1: second non-novel profile scaffold.
//! Creates `Works/<work_ref>/Design/` with 12 template files, a README.md,
//! `Logs/design/` and `Logs/review/` directories, and `PATCH`es the works row
//! to set `work_profile = 'game_bible'` and `work_ref`.
//!
//! # Concurrency note
//!
//! This capability runs in the single-user daemon process. We assume:
//! 1. Only one `game_bible.project_scaffold` invocation per `(creator_id, work_id)`
//!    is in flight at any time.
//! 2. No external process is mutating `Works/<work_ref>/` while this runs.
//!
//! # Deferred (W-005)
//!
//! The current implementation creates FS artifacts and updates the DB row
//! in separate steps (TOCTOU window). The same issue applies to essay scaffold.
//! Should adopt `ScaffoldTransaction` with Drop-based FS rollback before
//! production use with concurrent presets.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::info;

use crate::capability::{Capability, CapabilityError};

/// Input for the game-bible scaffold capability.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
struct ScaffoldInput {
    creator_id: String,
    work_id: String,
    work_ref: String,
    title: String,
    #[serde(default)]
    world_id: Option<String>,
}

/// Output from the game-bible scaffold capability.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ScaffoldOutput {
    scaffold_root: String,
    files_created: Vec<String>,
    dirs_created: Vec<String>,
}

/// Predefined Design template content with YAML frontmatter.
struct DesignTemplate {
    filename: &'static str,
    section_weight: &'static str,
    title: &'static str,
    comment: &'static str,
}

const DESIGN_TEMPLATES: &[DesignTemplate] = &[
    DesignTemplate {
        filename: "overview.md",
        section_weight: "critical",
        title: "Overview",
        comment: "Project vision, core loop summary, target audience",
    },
    DesignTemplate {
        filename: "pillars.md",
        section_weight: "critical",
        title: "Design Pillars",
        comment: "Core constraints, guiding principles, non-goals",
    },
    DesignTemplate {
        filename: "characters.md",
        section_weight: "important",
        title: "Characters",
        comment: "Character roles, archetypes, relationships",
    },
    DesignTemplate {
        filename: "factions.md",
        section_weight: "important",
        title: "Factions",
        comment: "Factions, politics, alignment, conflicts",
    },
    DesignTemplate {
        filename: "species.md",
        section_weight: "important",
        title: "Species",
        comment: "Sapient species, traits, cultures, biology",
    },
    DesignTemplate {
        filename: "locations.md",
        section_weight: "important",
        title: "Locations",
        comment: "World geography, levels, biomes, maps",
    },
    DesignTemplate {
        filename: "mechanics.md",
        section_weight: "critical",
        title: "Mechanics",
        comment: "Core mechanics, gameplay loops, systems",
    },
    DesignTemplate {
        filename: "magic_system.md",
        section_weight: "important",
        title: "Magic System",
        comment: "Magic/superpower rules, constraints, costs",
    },
    DesignTemplate {
        filename: "technology.md",
        section_weight: "important",
        title: "Technology",
        comment: "Tech level, tools, artifacts, research",
    },
    DesignTemplate {
        filename: "economy.md",
        section_weight: "important",
        title: "Economy",
        comment: "Currency, trade, resources, sinks, balance",
    },
    DesignTemplate {
        filename: "progression.md",
        section_weight: "important",
        title: "Progression",
        comment: "Leveling, skill trees, unlocks, player growth",
    },
    DesignTemplate {
        filename: "lore.md",
        section_weight: "nice_to_have",
        title: "Lore",
        comment: "History, mythology, cosmology, legends",
    },
];

fn render_template(tmpl: &DesignTemplate) -> String {
    format!(
        "---\nsection_status: draft\nsection_weight: {}\n---\n\n# {}\n<!-- {} -->\n",
        tmpl.section_weight, tmpl.title, tmpl.comment,
    )
}

/// `game_bible.project_scaffold` capability.
pub struct GameBibleProjectScaffold {
    pool: Option<sqlx::SqlitePool>,
    works_root: PathBuf,
}

impl GameBibleProjectScaffold {
    /// Create a standalone (no-pool) instance for testing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: None,
            works_root: PathBuf::from("Works"),
        }
    }

    /// Create an instance with a DB pool (default Works root).
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(pool),
            works_root: PathBuf::from("Works"),
        }
    }

    /// Create an instance with a DB pool and custom Works root (for e2e tests).
    #[must_use]
    pub const fn new_with_root(pool: sqlx::SqlitePool, works_root: PathBuf) -> Self {
        Self {
            pool: Some(pool),
            works_root,
        }
    }
}

impl Default for GameBibleProjectScaffold {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for GameBibleProjectScaffold {
    fn name(&self) -> &'static str {
        "game_bible.project_scaffold"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","files_created","dirs_created"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("game_bible.project_scaffold input: {e}"))
        })?;

        info!(
            work_id = %inp.work_id,
            work_ref = %inp.work_ref,
            world_id = ?inp.world_id,
            "game_bible.project_scaffold: start"
        );

        let work_dir = self.works_root.join(&inp.work_ref);
        let design_dir = work_dir.join("Design");
        let logs_dir = work_dir.join("Logs");
        let logs_design_dir = logs_dir.join("design");
        let logs_review_dir = logs_dir.join("review");

        let mut files_created = Vec::new();
        let mut dirs_created = Vec::new();

        // Create directory structure
        for dir in [
            &work_dir,
            &design_dir,
            &logs_dir,
            &logs_design_dir,
            &logs_review_dir,
        ] {
            if !dir.exists() {
                tokio::fs::create_dir_all(dir).await.map_err(|e| {
                    CapabilityError::Internal(format!("mkdir {}: {e}", dir.display()))
                })?;
                dirs_created.push(
                    dir.strip_prefix(&self.works_root)
                        .unwrap_or(dir.as_path())
                        .display()
                        .to_string(),
                );
            }
        }

        // Write README.md
        let readme_path = work_dir.join("README.md");
        let readme_content = format!(
            "# {title}\n\nGame design bible.\n\n- **Work ID**: {work_id}\n- **Profile**: game_bible\n\n## Core Pillars\n\n<!-- Genre, tone, target audience, and key design constraints -->\n",
            title = inp.title,
            work_id = inp.work_id,
        );
        tokio::fs::write(&readme_path, &readme_content)
            .await
            .map_err(|e| CapabilityError::Internal(format!("write README.md: {e}")))?;
        files_created.push("README.md".to_string());

        // Write 12 Design/*.md template files
        for tmpl in DESIGN_TEMPLATES {
            let content = render_template(tmpl);
            let path = design_dir.join(tmpl.filename);
            tokio::fs::write(&path, &content).await.map_err(|e| {
                CapabilityError::Internal(format!("write Design/{}: {e}", tmpl.filename))
            })?;
            files_created.push(format!("Design/{}", tmpl.filename));
        }

        // PATCH works row: set work_profile and work_ref
        if let Some(ref pool) = self.pool {
            sqlx::query(
                "UPDATE works SET work_profile = 'game_bible', work_ref = ? WHERE work_id = ?",
            )
            .bind(&inp.work_ref)
            .bind(&inp.work_id)
            .execute(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("patch works row: {e}")))?;
        }

        let output = ScaffoldOutput {
            scaffold_root: work_dir.display().to_string(),
            files_created,
            dirs_created,
        };

        info!(
            work_id = %inp.work_id,
            files = ?output.files_created,
            "game_bible.project_scaffold: done"
        );

        serde_json::to_value(output).map_err(|e| {
            CapabilityError::Internal(format!("game_bible.project_scaffold output: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_design_templates_render_non_empty() {
        for tmpl in DESIGN_TEMPLATES {
            let content = render_template(tmpl);
            assert!(!content.is_empty(), "template {} is empty", tmpl.filename);
            assert!(
                content.contains("section_status: draft"),
                "template {} missing section_status frontmatter",
                tmpl.filename
            );
            assert!(
                content.contains("section_weight:"),
                "template {} missing section_weight",
                tmpl.filename
            );
        }
    }

    #[test]
    fn design_templates_count_is_twelve() {
        assert_eq!(DESIGN_TEMPLATES.len(), 12);
    }

    #[test]
    fn critical_sections_are_overview_pillars_mechanics() {
        let critical: Vec<&str> = DESIGN_TEMPLATES
            .iter()
            .filter(|t| t.section_weight == "critical")
            .map(|t| t.filename)
            .collect();
        assert!(critical.contains(&"overview.md"));
        assert!(critical.contains(&"pillars.md"));
        assert!(critical.contains(&"mechanics.md"));
    }

    #[test]
    fn game_bible_capability_name() {
        let cap = GameBibleProjectScaffold::new();
        assert_eq!(cap.name(), "game_bible.project_scaffold");
    }
}
