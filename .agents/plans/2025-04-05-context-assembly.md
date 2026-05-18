# Context Assembly Basic — CLI-Side Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Scope**: CLI-side only. Platform-side Context Assembly service belongs in private nexus-platform repo.
> **Supersedes**: Original plan which contained Neo4j/pgvector scope violations.
> **Spec**: `.agents/knowledge/restructured-context-assembly.md`

**Goal:** Implement CLI-side context assembly — summary generation from local manuscript files, Local API client for `POST /v1/local/context/assemble`, bundle metadata wiring, CLI command, and JSON Schema registration.

**Architecture:** The CLI generates `StoryManifest.summary_text` from local manuscript files using basic extraction (title + chapter list + word count + opening excerpt). It calls the platform's Context Assembly service via the existing `DaemonClient` (loopback HTTP to nexus42d, which proxies to platform). Summaries are attached to sync bundles via the existing `story_manifest_delta()` helper and `BundleBuilder` fluent API.

**Tech Stack:** Rust (nexus42 crate), JSON Schema, reqwest (existing DaemonClient), serde, clap, tempfile (dev-dep)

**Working Branch:** `feature/v1.0-context-assembly` from `main`

**Dependencies:** cli-daemon-foundation (Done), sync-contract (Done), acp-client (Done) — all unblocked.

---

## Design Constraints

1. **CLI-side ONLY** — no Neo4j, Postgres, pgvector, HybridRAG, embedding, reranking. These are platform-side TypeScript concerns in the private `nexus-platform` repo.
2. **No new crate** — all code lives in `crates/nexus42/src/context/` module. `crates/nexus-context/` is explicitly prohibited.
3. **No Docker Compose** — CLI uses SQLite for local state only.
4. **Summary is basic extraction (V1.0)** — title + chapter list + word count + opening excerpt. No LLM call required.
5. **Local API is the only integration point** — `POST /v1/local/context/assemble` goes through nexus42d loopback to platform.
6. **Schema file goes to `schemas/platform/`** — follows the existing `schemas/` convention.
7. **Follow existing patterns** — clap derive macros, DaemonClient, BundleBuilder, thiserror errors.

---

## Task Breakdown

### Task 1: JSON Schema — Context Assembly Request/Response

**Goal:** Register the wire contract for `POST /v1/local/context/assemble` request/response shapes in `schemas/platform/`.

**Files:**
- Create: `schemas/platform/context-assembly-v1.schema.json`

- [ ] **Step 1: Create the schema file with ContextAssembleRequestV1**

Create `schemas/platform/context-assembly-v1.schema.json`:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/platform/context-assembly-v1.schema.json",
  "schema_version": 1,
  "title": "ContextAssembleRequestV1",
  "description": "Request shape for POST /v1/local/context/assemble. CLI sends this to request a stable read-only context snapshot from the platform.",
  "type": "object",
  "required": ["request_id", "workspace_id", "creator_id", "world_id"],
  "properties": {
    "request_id": {
      "type": "string",
      "minLength": 1,
      "description": "Caller-generated traceable ID (Local API envelope)"
    },
    "workspace_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorkspaceId"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId"
    },
    "include_memory": {
      "type": "boolean",
      "default": true,
      "description": "Include memory items in assembled context"
    },
    "include_timeline": {
      "type": "boolean",
      "default": true,
      "description": "Include timeline events in assembled context"
    },
    "include_story_summaries": {
      "type": "boolean",
      "default": true,
      "description": "Include story summaries in assembled context"
    },
    "memory_kinds": {
      "type": "array",
      "items": {
        "type": "string",
        "enum": ["story_summary", "research_material", "review_note"]
      },
      "default": ["story_summary", "research_material", "review_note"],
      "description": "Filter memory items by kind"
    },
    "max_timeline_events": {
      "type": ["integer", "null"],
      "minimum": 1,
      "maximum": 100,
      "description": "Maximum number of recent timeline events (null = platform default)"
    },
    "max_story_summaries": {
      "type": ["integer", "null"],
      "minimum": 1,
      "maximum": 50,
      "description": "Maximum number of story summaries (null = platform default)"
    }
  },
  "additionalProperties": false
}
```

- [ ] **Step 2: Run JSON validity check**

```bash
python3 -m json.tool schemas/platform/context-assembly-v1.schema.json > /dev/null
```
Expected: No output (valid JSON).

- [ ] **Step 3: Run schema validator**

```bash
node tooling/validation/schema-validator.js
```
Expected: Schema validation passes.

- [ ] **Step 4: Commit**

```bash
git add schemas/platform/context-assembly-v1.schema.json
git commit -m "feat(schemas): add context assembly request/response schema"
```

---

### Task 2: Context Module — Rust Types

**Goal:** Define serde-compatible Rust structs for the request/response shapes and module root.

**Files:**
- Create: `crates/nexus42/src/context/mod.rs`
- Create: `crates/nexus42/src/context/types.rs`
- Modify: `crates/nexus42/src/lib.rs` (add `pub mod context;`)

- [ ] **Step 1: Write failing test for types serialization**

Create `crates/nexus42/src/context/types.rs`:

```rust
//! Context Assembly — request/response types for POST /v1/local/context/assemble.

use serde::{Deserialize, Serialize};

/// Request for context assembly via the Local API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextAssembleRequest {
    /// Caller-generated traceable ID.
    pub request_id: String,
    /// Workspace ID (pattern: `wrk_.*`).
    pub workspace_id: String,
    /// Creator ID (pattern: `ctr_.*`).
    pub creator_id: String,
    /// World ID (pattern: `wld_.*`).
    pub world_id: String,
    /// Include memory items in assembled context.
    #[serde(default = "default_true")]
    pub include_memory: bool,
    /// Include timeline events in assembled context.
    #[serde(default = "default_true")]
    pub include_timeline: bool,
    /// Include story summaries in assembled context.
    #[serde(default = "default_true")]
    pub include_story_summaries: bool,
    /// Filter memory items by kind.
    #[serde(default = "default_memory_kinds")]
    pub memory_kinds: Vec<String>,
    /// Maximum number of recent timeline events (null = platform default).
    pub max_timeline_events: Option<u64>,
    /// Maximum number of story summaries (null = platform default).
    pub max_story_summaries: Option<u64>,
}

fn default_true() -> bool {
    true
}

fn default_memory_kinds() -> Vec<String> {
    vec![
        "story_summary".to_string(),
        "research_material".to_string(),
        "review_note".to_string(),
    ]
}

impl ContextAssembleRequest {
    /// Create a minimal request with required fields and default options.
    pub fn new(
        request_id: String,
        workspace_id: String,
        creator_id: String,
        world_id: String,
    ) -> Self {
        Self {
            request_id,
            workspace_id,
            creator_id,
            world_id,
            include_memory: true,
            include_timeline: true,
            include_story_summaries: true,
            memory_kinds: default_memory_kinds(),
            max_timeline_events: None,
            max_story_summaries: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// A KeyBlock in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBlockSnapshot {
    pub key_block_id: String,
    pub block_type: String,
    pub name: String,
    pub summary: String,
}

/// A timeline event in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineEventSnapshot {
    pub event_id: String,
    pub event_type: String,
    pub description: String,
    pub occurred_at: String,
}

/// A story summary in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorySummarySnapshot {
    pub story_manifest_id: String,
    pub title: String,
    pub summary_text: String,
    pub manifest_type: String,
}

/// A memory item in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryItemSnapshot {
    pub memory_id: String,
    pub memory_kind: String,
    pub content: String,
}

/// Response from POST /v1/local/context/assemble.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextAssembleResponse {
    /// Echo of request_id for correlation.
    pub request_id: String,
    /// Whether the assembly succeeded.
    pub success: bool,
    /// Error code if success=false.
    pub error_code: Option<String>,
    /// Human-readable error message if success=false.
    pub error_message: Option<String>,
    /// World ID this context belongs to.
    pub world_id: String,
    /// ISO 8601 timestamp when this snapshot was assembled.
    pub assembled_at: String,
    /// Freshness indicator (e.g., bundle ID) to detect stale data.
    pub data_freshness_hint: Option<String>,
    /// Confirmed KeyBlocks relevant to the world.
    #[serde(default)]
    pub key_blocks: Vec<KeyBlockSnapshot>,
    /// Recent canon timeline events.
    #[serde(default)]
    pub timeline_events: Vec<TimelineEventSnapshot>,
    /// Story summaries from StoryManifest.summary_text.
    #[serde(default)]
    pub story_summaries: Vec<StorySummarySnapshot>,
    /// Memory slices.
    #[serde(default)]
    pub memory_items: Vec<MemoryItemSnapshot>,
}

impl ContextAssembleResponse {
    /// Check whether the response indicates an error.
    pub fn is_error(&self) -> bool {
        !self.success
    }

    /// Get the error code, if any.
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_valid_json() {
        let req = ContextAssembleRequest::new(
            "req_test".to_string(),
            "wrk_001".to_string(),
            "ctr_001".to_string(),
            "wld_001".to_string(),
        );
        let json = serde_json::to_string(&req).expect("serialization should succeed");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json should be valid");
        assert_eq!(parsed["request_id"], "req_test");
        assert_eq!(parsed["workspace_id"], "wrk_001");
        assert_eq!(parsed["creator_id"], "ctr_001");
        assert_eq!(parsed["world_id"], "wld_001");
        assert_eq!(parsed["include_memory"], true);
        assert_eq!(parsed["include_timeline"], true);
        assert_eq!(parsed["include_story_summaries"], true);
    }

    #[test]
    fn request_deserializes_with_defaults() {
        let json = r#"{
            "request_id": "req_1",
            "workspace_id": "wrk_1",
            "creator_id": "ctr_1",
            "world_id": "wld_1"
        }"#;
        let req: ContextAssembleRequest =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(req.include_memory);
        assert!(req.include_timeline);
        assert!(req.include_story_summaries);
        assert_eq!(req.memory_kinds.len(), 3);
        assert_eq!(req.max_timeline_events, None);
        assert_eq!(req.max_story_summaries, None);
    }

    #[test]
    fn request_deserializes_with_explicit_options() {
        let json = r#"{
            "request_id": "req_2",
            "workspace_id": "wrk_1",
            "creator_id": "ctr_1",
            "world_id": "wld_1",
            "include_memory": false,
            "max_timeline_events": 10
        }"#;
        let req: ContextAssembleRequest =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(!req.include_memory);
        assert_eq!(req.max_timeline_events, Some(10));
    }

    #[test]
    fn response_success_roundtrip() {
        let resp = ContextAssembleResponse {
            request_id: "req_1".to_string(),
            success: true,
            error_code: None,
            error_message: None,
            world_id: "wld_001".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: Some("bdl_abc123".to_string()),
            key_blocks: vec![],
            timeline_events: vec![],
            story_summaries: vec![],
            memory_items: vec![],
        };
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponse =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized, resp);
        assert!(!deserialized.is_error());
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = ContextAssembleResponse {
            request_id: "req_2".to_string(),
            success: false,
            error_code: Some("world_not_found".to_string()),
            error_message: Some("World does not exist".to_string()),
            world_id: "wld_999".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: None,
            key_blocks: vec![],
            timeline_events: vec![],
            story_summaries: vec![],
            memory_items: vec![],
        };
        assert!(resp.is_error());
        assert_eq!(resp.error_code(), Some("world_not_found"));
    }

    #[test]
    fn response_with_data_roundtrip() {
        let resp = ContextAssembleResponse {
            request_id: "req_3".to_string(),
            success: true,
            error_code: None,
            error_message: None,
            world_id: "wld_001".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: None,
            key_blocks: vec![KeyBlockSnapshot {
                key_block_id: "kb_001".to_string(),
                block_type: "character".to_string(),
                name: "Hero".to_string(),
                summary: "The protagonist".to_string(),
            }],
            timeline_events: vec![TimelineEventSnapshot {
                event_id: "evt_001".to_string(),
                event_type: "plot_point".to_string(),
                description: "Discovery".to_string(),
                occurred_at: "2025-04-01T00:00:00Z".to_string(),
            }],
            story_summaries: vec![StorySummarySnapshot {
                story_manifest_id: "stm_001".to_string(),
                title: "Chapter 1".to_string(),
                summary_text: "The beginning".to_string(),
                manifest_type: "chapter".to_string(),
            }],
            memory_items: vec![MemoryItemSnapshot {
                memory_id: "mem_001".to_string(),
                memory_kind: "story_summary".to_string(),
                content: "Important detail".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponse =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized.key_blocks.len(), 1);
        assert_eq!(deserialized.timeline_events.len(), 1);
        assert_eq!(deserialized.story_summaries.len(), 1);
        assert_eq!(deserialized.memory_items.len(), 1);
    }
}
```

- [ ] **Step 2: Create module root**

Create `crates/nexus42/src/context/mod.rs`:

```rust
//! Context Assembly — CLI-side module.
//!
//! Provides:
//! - Summary generation from local manuscript files
//! - Local API client for POST /v1/local/context/assemble
//! - Request/response types

pub mod client;
pub mod summary;
pub mod types;
```

Note: `client` and `summary` modules will be created in Tasks 3 and 4. For this step, create stub files so the module compiles:

Create `crates/nexus42/src/context/client.rs`:

```rust
//! Context Assembly Local API client (stub — implemented in Task 3).
```

Create `crates/nexus42/src/context/summary.rs`:

```rust
//! Summary generation from manuscript files (stub — implemented in Task 4).
```

- [ ] **Step 3: Register module in lib.rs**

Add `pub mod context;` to `crates/nexus42/src/lib.rs` after the existing `pub mod auth;` line:

```rust
pub mod acp;
pub mod api;
pub mod auth;
pub mod config;
pub mod context;  // <-- add this line
pub mod commands;
pub mod errors;
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p nexus42 context::types
```
Expected: All 6 tests pass.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -p nexus42 -- -D warnings
```
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/nexus42/src/context/ crates/nexus42/src/lib.rs
git commit -m "feat(cli): add context module with request/response types"
```

---

### Task 3: Summary Generation from Manuscript Files

**Goal:** Implement `SummaryGenerator` that extracts basic summary text (title + chapter list + word count + opening excerpt) from local manuscript files under `Stories/<world_ref>/`.

**Files:**
- Modify: `crates/nexus42/src/context/summary.rs`
- Create: `crates/nexus42/src/context/summary_test.rs` (optional — tests inline in summary.rs using `#[cfg(test)]`)

- [ ] **Step 1: Write failing test — scan manuscript directory**

Add to `crates/nexus42/src/context/summary.rs`:

```rust
//! Summary generation from local manuscript files.
//!
//! V1.0 uses basic extraction: title (from front-matter or first heading),
//! chapter list (from heading structure), word count, and opening excerpt.
//! No LLM call required.

use std::fs;
use std::path::{Path, PathBuf};

/// Maximum summary text length (characters).
const MAX_SUMMARY_CHARS: usize = 4096;

/// Maximum opening excerpt length (characters).
const MAX_EXCERPT_CHARS: usize = 500;

/// A manuscript file discovered by scanning.
#[derive(Debug, Clone)]
pub struct ManuscriptFile {
    /// Relative path from the manuscript root.
    pub relative_path: PathBuf,
    /// File content.
    pub content: String,
}

/// Result of summary generation.
#[derive(Debug, Clone)]
pub struct GeneratedSummary {
    /// Title extracted from front-matter or first heading.
    pub title: Option<String>,
    /// List of chapter titles (from heading structure).
    pub chapters: Vec<String>,
    /// Total word count across all manuscript files.
    pub word_count: usize,
    /// Opening excerpt (first N chars of first chapter body).
    pub opening_excerpt: String,
    /// Final summary text suitable for StoryManifest.summary_text.
    pub summary_text: String,
}

/// Summary generator for local manuscript files.
pub struct SummaryGenerator {
    /// Path to the manuscript root (e.g., `Stories/<world_ref>/`).
    manuscript_root: PathBuf,
    /// Maximum summary length in characters.
    max_summary_chars: usize,
    /// Maximum excerpt length in characters.
    max_excerpt_chars: usize,
}

impl SummaryGenerator {
    /// Create a new summary generator for the given manuscript root.
    pub fn new(manuscript_root: PathBuf) -> Self {
        Self {
            manuscript_root,
            max_summary_chars: MAX_SUMMARY_CHARS,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
        }
    }

    /// Scan the manuscript directory for recognized file types.
    ///
    /// Walks `Stories/<world_ref>/` for `.md` and `.txt` files.
    /// Ignores `References/` tree and non-recognized extensions.
    pub fn scan_manuscript_dir(&self) -> std::io::Result<Vec<ManuscriptFile>> {
        let mut files = Vec::new();
        if !self.manuscript_root.exists() {
            return Ok(files);
        }
        self.scan_recursive(&self.manuscript_root, &mut files)?;
        Ok(files)
    }

    fn scan_recursive(
        &self,
        dir: &Path,
        files: &mut Vec<ManuscriptFile>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Skip References/ tree
                if let Some(name) = path.file_name() {
                    if name == "References" {
                        continue;
                    }
                }
                self.scan_recursive(&path, files)?;
            } else {
                let extension = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if matches!(extension, "md" | "txt") {
                    let relative = path
                        .strip_prefix(&self.manuscript_root)
                        .unwrap_or(&path)
                        .to_path_buf();
                    let content = fs::read_to_string(&path)?;
                    files.push(ManuscriptFile {
                        relative_path: relative,
                        content,
                    });
                }
            }
        }
        Ok(())
    }

    /// Generate a basic summary from manuscript files.
    pub fn generate(&self) -> std::io::Result<GeneratedSummary> {
        let files = self.scan_manuscript_dir()?;
        let mut title = None;
        let mut chapters = Vec::new();
        let mut total_words = 0;
        let mut first_body_excerpt: Option<String> = None;

        for file in &files {
            let words = file.content.split_whitespace().count();
            total_words += words;

            // Extract title from first file with a heading
            if title.is_none() {
                title = extract_title(&file.content);
            }

            // Extract chapter headings from all files
            extract_chapters(&file.content, &mut chapters);

            // Extract opening excerpt from first file with body text
            if first_body_excerpt.is_none() {
                first_body_excerpt = extract_opening_excerpt(&file.content, self.max_excerpt_chars);
            }
        }

        // Build summary text
        let mut summary_parts: Vec<String> = Vec::new();
        if let Some(t) = &title {
            summary_parts.push(format!("Title: {}", t));
        }
        if !chapters.is_empty() {
            let chapter_list: Vec<String> = chapters
                .iter()
                .enumerate()
                .map(|(i, ch)| format!("  {}. {}", i + 1, ch))
                .collect();
            summary_parts.push("Chapters:".to_string());
            summary_parts.extend(chapter_list);
        }
        summary_parts.push(format!("Word count: {}", total_words));
        if let Some(excerpt) = &first_body_excerpt {
            summary_parts.push(format!("Opening: {}", excerpt));
        }

        let mut summary_text = summary_parts.join("\n");
        // Truncate to max length
        if summary_text.len() > self.max_summary_chars {
            summary_text.truncate(self.max_summary_chars.saturating_sub(3));
            summary_text.push_str("...");
        }

        Ok(GeneratedSummary {
            title,
            chapters,
            word_count: total_words,
            opening_excerpt: first_body_excerpt.unwrap_or_default(),
            summary_text,
        })
    }
}

/// Extract title from markdown content (front-matter `title:` or first `#` heading).
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Check YAML front-matter title
        if trimmed.starts_with("title:") {
            let value = trimmed["title:".len()..].trim();
            // Remove surrounding quotes if present
            let cleaned = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }
        // Check first Markdown heading (but not inside front-matter)
        if trimmed.starts_with("# ") && !trimmed.starts_with("# #") {
            let value = trimmed[2..].trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Extract chapter titles from Markdown headings (`## ` level).
fn extract_chapters(content: &str, chapters: &mut Vec<String>) {
    let mut in_front_matter = false;
    for line in content.lines() {
        let trimmed = line.trim();
        // Track front-matter boundaries
        if trimmed == "---" {
            in_front_matter = !in_front_matter;
            continue;
        }
        if in_front_matter {
            continue;
        }
        // Extract `## ` headings as chapters
        if trimmed.starts_with("## ") {
            let chapter_title = trimmed[3..].trim().to_string();
            if !chapter_title.is_empty() {
                chapters.push(chapter_title);
            }
        }
    }
}

/// Extract opening excerpt from the first body text (after front-matter and headings).
fn extract_opening_excerpt(content: &str, max_chars: usize) -> Option<String> {
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_front_matter = false;
    let mut past_first_heading = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_front_matter = !in_front_matter;
            continue;
        }
        if in_front_matter {
            continue;
        }
        // Skip heading lines
        if trimmed.starts_with('#') {
            past_first_heading = true;
            continue;
        }
        // Skip empty lines before we have content
        if trimmed.is_empty() && body_lines.is_empty() {
            continue;
        }
        body_lines.push(trimmed);
    }

    if body_lines.is_empty() {
        return None;
    }

    let body_text = body_lines.join(" ");
    if body_text.len() <= max_chars {
        Some(body_text)
    } else {
        // Truncate at word boundary near max_chars
        let mut end = max_chars;
        while end > 0 && !body_text.is_char_boundary(end) {
            end -= 1;
        }
        // Try to break at a space
        if let Some(space_pos) = body_text[..end].rfind(' ') {
            end = space_pos;
        }
        Some(format!("{}...", &body_text[..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper: create a temp manuscript directory with files.
    fn create_test_manuscript(files: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        for (path, content) in files {
            let full = tmp.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            let mut f = fs::File::create(&full).expect("create file");
            f.write_all(content.as_bytes()).expect("write file");
        }
        tmp
    }

    #[test]
    fn scan_finds_markdown_files() {
        let tmp = create_test_manuscript(&[
            ("chapter-01.md", "# Chapter One\n\nHello world."),
            ("chapter-02.md", "# Chapter Two\n\nGoodbye world."),
            ("notes.txt", "Some notes."),
            ("image.png", "not text"),
        ]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert_eq!(files.len(), 3); // 2 md + 1 txt, not png
        assert!(files.iter().any(|f| f.relative_path.ends_with("chapter-01.md")));
        assert!(files.iter().any(|f| f.relative_path.ends_with("chapter-02.md")));
        assert!(files.iter().any(|f| f.relative_path.ends_with("notes.txt")));
    }

    #[test]
    fn scan_skips_references_directory() {
        let tmp = create_test_manuscript(&[
            ("chapter-01.md", "# Chapter One"),
            ("References/research.md", "# Research Notes"),
        ]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("chapter-01.md"));
    }

    #[test]
    fn scan_empty_directory() {
        let tmp = TempDir::new().expect("temp dir");
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert!(files.is_empty());
    }

    #[test]
    fn scan_nonexistent_directory() {
        let gen = SummaryGenerator::new(PathBuf::from("/nonexistent/path"));
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert!(files.is_empty());
    }

    #[test]
    fn extract_title_from_heading() {
        assert_eq!(
            extract_title("# My Story\n\nOnce upon a time..."),
            Some("My Story".to_string())
        );
    }

    #[test]
    fn extract_title_from_front_matter() {
        let content = "---\ntitle: \"The Great Adventure\"\n---\n\n# Chapter 1\n\nHello.";
        assert_eq!(
            extract_title(content),
            Some("The Great Adventure".to_string())
        );
    }

    #[test]
    fn extract_title_none_for_empty() {
        assert_eq!(extract_title(""), None);
        assert_eq!(extract_title("Just plain text."), None);
    }

    #[test]
    fn extract_chapters_from_headings() {
        let content = "# Title\n\n## The Beginning\n\n## The Middle\n\n## The End\n";
        let mut chapters = Vec::new();
        extract_chapters(content, &mut chapters);
        assert_eq!(chapters, vec!["The Beginning", "The Middle", "The End"]);
    }

    #[test]
    fn extract_chapters_skips_front_matter() {
        let content = "---\n## Not a chapter\n---\n\n## Real Chapter\n\n## Another\n";
        let mut chapters = Vec::new();
        extract_chapters(content, &mut chapters);
        assert_eq!(chapters, vec!["Real Chapter", "Another"]);
    }

    #[test]
    fn extract_opening_excerpt() {
        let content = "# Title\n\nThis is the opening paragraph of the story.\n\nIt continues here.";
        let excerpt = extract_opening_excerpt(content, 100).expect("should have excerpt");
        assert!(excerpt.contains("This is the opening paragraph"));
    }

    #[test]
    fn extract_opening_excerpt_truncates() {
        let long_body = "a".repeat(1000);
        let content = format!("# Title\n\n{}", long_body);
        let excerpt = extract_opening_excerpt(&content, 50).expect("should have excerpt");
        assert!(excerpt.len() <= 53); // 50 + "..."
    }

    #[test]
    fn generate_full_summary() {
        let tmp = create_test_manuscript(&[
            (
                "chapter-01.md",
                "---\ntitle: \"My Novel\"\n---\n\n## The Beginning\n\nIn a land far away, there lived a hero.\n\n## The Journey\n\nThe hero set out on a quest.",
            ),
            (
                "chapter-02.md",
                "## The Return\n\nAfter many adventures, the hero came home.\n\nThe end.",
            ),
        ]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert_eq!(summary.title, Some("My Novel".to_string()));
        assert_eq!(summary.chapters.len(), 3);
        assert!(summary.word_count > 0);
        assert!(!summary.summary_text.is_empty());
        assert!(summary.summary_text.contains("My Novel"));
        assert!(summary.summary_text.contains("Word count:"));
        assert!(summary.summary_text.len() <= 4096);
    }

    #[test]
    fn generate_empty_manuscript() {
        let tmp = TempDir::new().expect("temp dir");
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert_eq!(summary.title, None);
        assert!(summary.chapters.is_empty());
        assert_eq!(summary.word_count, 0);
    }

    #[test]
    fn summary_text_within_limit() {
        // Create a large file that would produce a long summary
        let big_content = format!("# Title\n\n{}\n", "word ".repeat(5000));
        let tmp = create_test_manuscript(&[("big.md", &big_content)]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert!(
            summary.summary_text.len() <= 4096,
            "summary text {} exceeds max {}",
            summary.summary_text.len(),
            4096
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
cargo test -p nexus42 context::summary
```
Expected: All 14 tests pass.

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -p nexus42 -- -D warnings
```
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/nexus42/src/context/summary.rs
git commit -m "feat(cli): add summary generation from manuscript files"
```

---

### Task 4: Context Assembly Local API Client

**Goal:** Implement the HTTP client that calls `POST /v1/local/context/assemble` via the existing `DaemonClient`.

**Files:**
- Modify: `crates/nexus42/src/context/client.rs`

- [ ] **Step 1: Write failing tests for client**

Replace the stub in `crates/nexus42/src/context/client.rs`:

```rust
//! Context Assembly Local API client.
//!
//! Calls POST /v1/local/context/assemble through the DaemonClient (nexus42d loopback).

use crate::api::daemon_client::DaemonClient;
use crate::errors::Result;
use super::types::{ContextAssembleRequest, ContextAssembleResponse};

/// Client for the Context Assembly Local API.
pub struct ContextClient {
    daemon: DaemonClient,
}

impl ContextClient {
    /// Create a new context client from a DaemonClient.
    pub fn new(daemon: DaemonClient) -> Self {
        Self { daemon }
    }

    /// Request assembled context from the platform via the Local API.
    ///
    /// Sends `POST /v1/local/context/assemble` through nexus42d.
    /// The daemon proxies this request to the platform's Context Assembly service.
    pub async fn assemble(
        &self,
        request: &ContextAssembleRequest,
    ) -> Result<ContextAssembleResponse> {
        let response: ContextAssembleResponse = self
            .daemon
            .post("/v1/local/context/assemble", request)
            .await
            .map_err(|e| crate::errors::CliError::Other(format!(
                "Context assembly request failed: {}",
                e
            )))?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper: create a DaemonClient pointed at a wiremock server URL.
    /// The actual mock server setup is done in individual tests.
    fn make_request() -> ContextAssembleRequest {
        ContextAssembleRequest::new(
            "req_test_001".to_string(),
            "wrk_001".to_string(),
            "ctr_001".to_string(),
            "wld_001".to_string(),
        )
    }

    #[test]
    fn request_serializes_correctly() {
        let req = make_request();
        let json = serde_json::to_value(&req).expect("serialization should succeed");
        assert_eq!(json["request_id"], "req_test_001");
        assert_eq!(json["workspace_id"], "wrk_001");
        assert_eq!(json["creator_id"], "ctr_001");
        assert_eq!(json["world_id"], "wld_001");
        assert_eq!(json["include_memory"], true);
    }

    #[tokio::test]
    async fn assemble_success_with_mock() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        let success_response = json!({
            "request_id": "req_test_001",
            "success": true,
            "error_code": null,
            "error_message": null,
            "world_id": "wld_001",
            "assembled_at": "2025-04-05T12:00:00Z",
            "data_freshness_hint": "bdl_abc123",
            "key_blocks": [],
            "timeline_events": [],
            "story_summaries": [],
            "memory_items": []
        });

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_response))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let response = client.assemble(&req).await.expect("assemble should succeed");

        assert!(response.success);
        assert_eq!(response.world_id, "wld_001");
        assert_eq!(response.data_freshness_hint, Some("bdl_abc123".to_string()));
    }

    #[tokio::test]
    async fn assemble_error_response() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        let error_response = json!({
            "request_id": "req_test_001",
            "success": false,
            "error_code": "world_not_found",
            "error_message": "World wld_999 does not exist",
            "world_id": "wld_999",
            "assembled_at": "2025-04-05T12:00:00Z",
            "data_freshness_hint": null,
            "key_blocks": [],
            "timeline_events": [],
            "story_summaries": [],
            "memory_items": []
        });

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&error_response))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let response = client.assemble(&req).await.expect("assemble should succeed");

        assert!(!response.success);
        assert_eq!(response.error_code, Some("world_not_found".to_string()));
        assert_eq!(
            response.error_message,
            Some("World wld_999 does not exist".to_string())
        );
    }

    #[tokio::test]
    async fn assemble_http_error() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(503).set_body("Platform unavailable"))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let result = client.assemble(&req).await;

        assert!(result.is_err(), "should return error for HTTP 503");
    }

    #[tokio::test]
    async fn assemble_with_full_data() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        let full_response = json!({
            "request_id": "req_test_001",
            "success": true,
            "error_code": null,
            "error_message": null,
            "world_id": "wld_001",
            "assembled_at": "2025-04-05T12:00:00Z",
            "data_freshness_hint": null,
            "key_blocks": [
                {
                    "key_block_id": "kb_001",
                    "block_type": "character",
                    "name": "Hero",
                    "summary": "The protagonist"
                }
            ],
            "timeline_events": [
                {
                    "event_id": "evt_001",
                    "event_type": "plot_point",
                    "description": "Discovery",
                    "occurred_at": "2025-04-01T00:00:00Z"
                }
            ],
            "story_summaries": [
                {
                    "story_manifest_id": "stm_001",
                    "title": "Chapter 1",
                    "summary_text": "The beginning",
                    "manifest_type": "chapter"
                }
            ],
            "memory_items": [
                {
                    "memory_id": "mem_001",
                    "memory_kind": "story_summary",
                    "content": "Important detail"
                }
            ]
        });

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&full_response))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let response = client.assemble(&req).await.expect("assemble should succeed");

        assert_eq!(response.key_blocks.len(), 1);
        assert_eq!(response.key_blocks[0].name, "Hero");
        assert_eq!(response.timeline_events.len(), 1);
        assert_eq!(response.story_summaries.len(), 1);
        assert_eq!(response.memory_items.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
cargo test -p nexus42 context::client
```
Expected: All 5 tests pass (1 sync + 4 async).

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -p nexus42 -- -D warnings
```
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/nexus42/src/context/client.rs
git commit -m "feat(cli): add context assembly Local API client with wiremock tests"
```

---

### Task 5: CLI Command — `nexus42 context assemble`

**Goal:** Wire the CLI command to the context assembly client, replacing the placeholder in `commands/context.rs`.

**Files:**
- Modify: `crates/nexus42/src/commands/context.rs`

- [ ] **Step 1: Replace the placeholder command implementation**

Replace the contents of `crates/nexus42/src/commands/context.rs`:

```rust
//! Context Command — `nexus42 context assemble`

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::context::client::ContextClient;
use crate::context::types::ContextAssembleRequest;
use crate::errors::Result;
use clap::Subcommand;
use uuid::Uuid;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Assemble context for a world via the Local API
    Assemble {
        /// World ID (required for context assembly)
        #[arg(long)]
        world_id: String,

        /// Workspace ID (defaults to current workspace)
        #[arg(long)]
        workspace_id: Option<String>,

        /// Creator ID (defaults to active creator)
        #[arg(long)]
        creator_id: Option<String>,

        /// Include memory items in assembled context
        #[arg(long, default_value_t = true)]
        include_memory: bool,

        /// Include timeline events in assembled context
        #[arg(long, default_value_t = true)]
        include_timeline: bool,

        /// Include story summaries in assembled context
        #[arg(long, default_value_t = true)]
        include_story_summaries: bool,

        /// Maximum number of recent timeline events (null = platform default)
        #[arg(long)]
        max_timeline_events: Option<u64>,

        /// Maximum number of story summaries (null = platform default)
        #[arg(long)]
        max_story_summaries: Option<u64>,

        /// Output file path (default: stdout as JSON)
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
}

/// Run context command
pub async fn run(cmd: ContextCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        ContextCommand::Assemble {
            world_id,
            workspace_id,
            creator_id,
            include_memory,
            include_timeline,
            include_story_summaries,
            max_timeline_events,
            max_story_summaries,
            output,
        } => {
            // Resolve workspace_id and creator_id from config if not provided
            let workspace_id = workspace_id.unwrap_or_else(|| {
                config
                    .workspace_path
                    .as_ref()
                    .map(|_| "wrk_current".to_string())
                    .unwrap_or_else(|| "wrk_unknown".to_string())
            });

            let creator_id = creator_id.unwrap_or_else(|| {
                config
                    .active_creator_id
                    .clone()
                    .unwrap_or_else(|| "ctr_unknown".to_string())
            });

            // Build the request
            let request = ContextAssembleRequest {
                request_id: format!("req_{}", Uuid::new_v4().simple()),
                workspace_id,
                creator_id,
                world_id,
                include_memory,
                include_timeline,
                include_story_summaries,
                memory_kinds: vec![
                    "story_summary".to_string(),
                    "research_material".to_string(),
                    "review_note".to_string(),
                ],
                max_timeline_events,
                max_story_summaries,
            };

            // Create daemon client and context client
            let daemon = DaemonClient::from_config(config);
            let client = ContextClient::new(daemon);

            // Call the Local API
            let response = client.assemble(&request).await?;

            // Handle error responses
            if response.is_error() {
                let error_code = response.error_code().unwrap_or("unknown");
                let error_message = response
                    .error_message
                    .as_deref()
                    .unwrap_or("No details available");
                eprintln!("Error: Context assembly failed ({})", error_code);
                eprintln!("  {}", error_message);
                if error_code == "auth_expired" {
                    eprintln!("  Run `nexus42 auth login` to re-authenticate.");
                } else if error_code == "world_not_found" {
                    eprintln!("  Check the world ID and ensure the world exists on the platform.");
                } else if error_code == "platform_unavailable" {
                    eprintln!("  The platform may be temporarily unavailable. Try again later.");
                }
                std::process::exit(1);
            }

            // Output the response
            let output_json = serde_json::to_string_pretty(&response)?;
            match output {
                Some(path) => {
                    std::fs::write(&path, &output_json)?;
                    eprintln!("Context assembly written to {}", path);
                }
                None => {
                    println!("{}", output_json);
                }
            }

            Ok(())
        }
    }
}
```

- [ ] **Step 2: Verify help text**

```bash
cargo run -p nexus42 -- context assemble --help
```
Expected: Help text shows all flags (`--world-id`, `--workspace-id`, `--creator-id`, `--include-memory`, `--include-timeline`, `--include-story-summaries`, `--max-timeline-events`, `--max-story-summaries`, `--output`).

- [ ] **Step 3: Run workspace tests**

```bash
cargo test -p nexus42 context
```
Expected: All context module tests pass.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy -p nexus42 -- -D warnings
```
Expected: No warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/nexus42/src/commands/context.rs
git commit -m "feat(cli): implement nexus42 context assemble command"
```

---

## Integration Points

| Component | Location | How Used |
|-----------|----------|----------|
| **DaemonClient** | `crates/nexus42/src/api/daemon_client.rs` | HTTP client for Local API (loopback port 8420) |
| **BundleBuilder** | `crates/nexus-sync/src/delta_bundle.rs` | Fluent API for constructing sync bundles with `story_manifest_delta()` |
| **story_manifest_delta()** | `crates/nexus-sync/src/delta_bundle.rs:315` | Creates a `DeltaType::StoryManifest` delta with `summary_text` payload |
| **StoryManifest.summary_text** | `schemas/domain/story-manifest.schema.json:44` | Already exists in schema as `Option<String>` |
| **DeltaType::StoryManifest** | `crates/nexus-sync/src/delta_bundle.rs:57` | Already in the `DeltaType` enum |
| **CliConfig** | `crates/nexus42/src/config.rs` | Provides `daemon_url`, `workspace_path`, `active_creator_id` |
| **CliError** | `crates/nexus42/src/errors.rs` | Existing error types (Api, Network, Other) |
| **Common schema** | `schemas/common/common.schema.json` | `$ref` targets for WorkspaceId, CreatorId, WorldId, Timestamp |

## Acceptance Criteria

| # | Criterion | Verification |
|---|-----------|-------------|
| 1 | Schema file registered in `schemas/platform/` | `python3 -m json.tool schemas/platform/context-assembly-v1.schema.json` succeeds |
| 2 | Schema validator passes | `node tooling/validation/schema-validator.js` passes |
| 3 | Context module compiles | `cargo build -p nexus42` succeeds |
| 4 | Request/response types serialize/deserialize correctly | `cargo test -p nexus42 context::types` — 6 tests pass |
| 5 | Summary generator scans manuscript dirs and produces text | `cargo test -p nexus42 context::summary` — 14 tests pass |
| 6 | Context client calls Local API and parses responses | `cargo test -p nexus42 context::client` — 5 tests pass |
| 7 | CLI command `nexus42 context assemble --help` shows all flags | Manual verification |
| 8 | CLI command handles success, error, and degraded responses | Tests pass |
| 9 | No Neo4j/Postgres/pgvector dependencies | `grep -r "neo4j\|pgvector\|sqlx\|neo4rs" crates/nexus42/Cargo.toml` returns nothing |
| 10 | No new crate created | `ls crates/` does not contain `nexus-context/` |
| 11 | Clippy clean | `cargo clippy -p nexus42 -- -D warnings` passes |
| 12 | All workspace tests pass | `cargo test --all` passes |

## Effort Estimate (agent-oriented)

- **Complexity**: **M** (medium)
- **Agent session band**: ~1–2 focused agent sessions. Each task is independently S-sized; collectively M due to 5 tasks spanning schema, types, file I/O, HTTP client, and CLI wiring.
- **Assumptions**: Plan is locked, all dependency plans (cli-daemon-foundation, sync-contract, acp-client) are Done and merged to main, wiremock is already in dev-dependencies.

---

*Plan saved to:* `.agents/plans/2025-04-05-context-assembly.md`
