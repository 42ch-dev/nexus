//! Non-executable capability metadata shared with the runtime registry.

/// Input-schema lookup used by preset validation. `None` means unknown.
pub trait CapabilityCatalog {
    fn input_schema(&self, name: &str) -> Option<&str>;
}

impl<T: CapabilityCatalog + ?Sized> CapabilityCatalog for std::sync::Arc<T> {
    fn input_schema(&self, name: &str) -> Option<&str> {
        self.as_ref().input_schema(name)
    }
}

/// Static builtin metadata; never constructs an executable capability.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinCapabilityCatalog;

impl CapabilityCatalog for BuiltinCapabilityCatalog {
    fn input_schema(&self, name: &str) -> Option<&str> {
        builtin_input_schema(name)
    }
}

/// The canonical builtin capability declaration — THE single table (C3):
/// one `(name, schema)` entry per builtin, from which BOTH public accessors
/// derive (`builtin_names` iterates the first column, `builtin_input_schema`
/// searches the table by name). A name added to one accessor but not the
/// other is impossible by construction: there is no second place to declare
/// it. Sorted by name, so [`builtin_input_schema`] may binary-search.
///
/// A builtin may be declared here while no handler object is constructed —
/// it stays describable and reserved purely by being in this table.
static BUILTIN_CAPABILITIES: &[(&str, &str)] = &[
    ("acp.prompt", ACP_PROMPT_INPUT_SCHEMA),
    ("context.summarize", CONTEXT_SUMMARIZE_INPUT_SCHEMA),
    ("creator.inject_prompt", CREATOR_INJECT_PROMPT_INPUT_SCHEMA),
    ("creator.read_memory", CREATOR_READ_MEMORY_INPUT_SCHEMA),
    ("creator.write_brief", CREATOR_WRITE_BRIEF_INPUT_SCHEMA),
    ("creator.write_memory", CREATOR_WRITE_MEMORY_INPUT_SCHEMA),
    (
        "essay.draft_status.finalize",
        ESSAY_DRAFT_STATUS_FINALIZE_INPUT_SCHEMA,
    ),
    (
        "essay.project_scaffold",
        ESSAY_PROJECT_SCAFFOLD_INPUT_SCHEMA,
    ),
    (
        "game_bible.project_scaffold",
        GAME_BIBLE_PROJECT_SCAFFOLD_INPUT_SCHEMA,
    ),
    (
        "game_bible.section_status.update",
        GAME_BIBLE_SECTION_STATUS_UPDATE_INPUT_SCHEMA,
    ),
    ("judge.llm", JUDGE_LLM_INPUT_SCHEMA),
    ("judge.rule", JUDGE_RULE_INPUT_SCHEMA),
    ("kb.extract_work", KB_EXTRACT_WORK_INPUT_SCHEMA),
    ("narrative.compute", NARRATIVE_COMPUTE_INPUT_SCHEMA),
    ("nexus.fork.create", NEXUS_FORK_CREATE_INPUT_SCHEMA),
    ("nexus.llm.extract", NEXUS_LLM_EXTRACT_INPUT_SCHEMA),
    (
        "nexus.timeline.event.append",
        NEXUS_TIMELINE_EVENT_APPEND_INPUT_SCHEMA,
    ),
    (
        "nexus.world.delta.apply",
        NEXUS_WORLD_DELTA_APPLY_INPUT_SCHEMA,
    ),
    (
        "nexus.world.delta.propose",
        NEXUS_WORLD_DELTA_PROPOSE_INPUT_SCHEMA,
    ),
    (
        "nexus.world.state.query",
        NEXUS_WORLD_STATE_QUERY_INPUT_SCHEMA,
    ),
    (
        "novel.chapter_transition",
        NOVEL_CHAPTER_TRANSITION_INPUT_SCHEMA,
    ),
    (
        "novel.project_scaffold",
        NOVEL_PROJECT_SCAFFOLD_INPUT_SCHEMA,
    ),
    ("outbox.compact", OUTBOX_COMPACT_INPUT_SCHEMA),
    ("outbox.flush", OUTBOX_FLUSH_INPUT_SCHEMA),
    ("registry.refresh", REGISTRY_REFRESH_INPUT_SCHEMA),
    (
        "script.project_scaffold",
        SCRIPT_PROJECT_SCAFFOLD_INPUT_SCHEMA,
    ),
    (
        "script.section_status.update",
        SCRIPT_SECTION_STATUS_UPDATE_INPUT_SCHEMA,
    ),
    (
        "soul.experience.aggregate",
        SOUL_EXPERIENCE_AGGREGATE_INPUT_SCHEMA,
    ),
    ("sync.pull", SYNC_PULL_INPUT_SCHEMA),
    ("sync.push", SYNC_PUSH_INPUT_SCHEMA),
    ("workspace.commit", WORKSPACE_COMMIT_INPUT_SCHEMA),
    ("workspace.open", WORKSPACE_OPEN_INPUT_SCHEMA),
];

/// Iterate the canonical builtin declaration (C3) — the name column of
/// [`BUILTIN_CAPABILITIES`]. This is THE projection every builtin-name
/// consumer must read: boot scan collision ownership, watcher rebuild
/// collision ownership, registry reservation and dispatch's known-name
/// discrimination all derive from it, never from constructed handler
/// objects.
#[must_use]
pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_CAPABILITIES.iter().map(|(name, _)| *name)
}

/// Resolve the canonical builtin input schema — the schema column of
/// [`BUILTIN_CAPABILITIES`], searched by name. Unknown names fail closed.
#[must_use]
pub fn builtin_input_schema(name: &str) -> Option<&'static str> {
    // The table is sorted by name (declaration order above).
    BUILTIN_CAPABILITIES
        .binary_search_by(|(declared, _)| (*declared).cmp(name))
        .ok()
        .map(|idx| BUILTIN_CAPABILITIES[idx].1)
}

/// Input schema for `acp.prompt`.
pub const ACP_PROMPT_INPUT_SCHEMA: &str = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string", "description": "The prompt text to send to the ACP agent" },
                "tool_policy": {
                    "type": "string",
                    "enum": ["auto_grant_all", "auto_grant_read_only", "deny_all", "request_policy"],
                    "default": "auto_grant_read_only",
                    "description": "Tool permission policy for this prompt"
                }
                // "_creator_id" and "_session_id" are injected by orchestration context,
                // NOT accepted from user input (security: prevents cross-creator routing).
            }
        }"#;

/// Input schema for `context.summarize`.
pub const CONTEXT_SUMMARIZE_INPUT_SCHEMA: &str = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Current core_context text to summarize"
                },
                "trace": {
                    "type": "string",
                    "description": "Optional state execution trace for context"
                },
                "template": {
                    "type": "string",
                    "description": "Optional summarization template/instructions"
                }
            }
        }"#;

/// Input schema for `creator.inject_prompt`.
pub const CREATOR_INJECT_PROMPT_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"prompt":{"type":"string"},"priority":{"type":"integer","default":0},"prompt_file":{"type":"string"},"vars":{"type":"object","additionalProperties":{"type":"string"}}},"required":[],"anyOf":[{"required":["prompt"]},{"required":["prompt_file"]}],"additionalProperties":false}"#;

/// Input schema for `creator.read_memory`.
pub const CREATOR_READ_MEMORY_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"keyword":{"type":"string"},"limit":{"type":"integer","minimum":1,"default":50}},"required":[],"additionalProperties":false}"#;

/// Input schema for `creator.write_brief`.
pub const CREATOR_WRITE_BRIEF_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"workId":{"type":"string"},"briefText":{"type":"string"}},"required":["workId","briefText"],"additionalProperties":false}"#;

/// Input schema for `creator.write_memory`.
pub const CREATOR_WRITE_MEMORY_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"content":{"type":"string"},"keywords":{"type":"array","items":{"type":"string"}},"required":["content","keywords"],"additionalProperties":false}"#;

/// Input schema for `essay.draft_status.finalize`.
pub const ESSAY_DRAFT_STATUS_FINALIZE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"work_ref":{"type":"string"},"works_root":{"type":"string"},"word_count":{"anyOf":[{"type":"string","enum":["auto"]},{"type":"integer"}]}},"required":["work_ref"],"additionalProperties":false}"#;

/// Input schema for `essay.project_scaffold`.
pub const ESSAY_PROJECT_SCAFFOLD_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#;

/// Input schema for `game_bible.project_scaffold`.
pub const GAME_BIBLE_PROJECT_SCAFFOLD_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#;

/// Input schema for `game_bible.section_status.update`.
pub const GAME_BIBLE_SECTION_STATUS_UPDATE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"work_ref":{"type":"string"},"section_path":{"type":"string"},"new_status":{"type":"string","enum":["draft","reviewed","accepted"]},"reason":{"type":"string"},"works_root":{"type":"string"}},"required":["work_ref","section_path","new_status"],"additionalProperties":false}"#;

/// Input schema for `judge.llm`.
pub const JUDGE_LLM_INPUT_SCHEMA: &str = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string", "description": "The evaluation prompt for the judge" }
            }
        }"#;

/// Input schema for `judge.rule`.
pub const JUDGE_RULE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"rule":{"type":"string"},"contextData":{}},"required":["rule","contextData"],"additionalProperties":false}"#;

/// Input schema for `kb.extract_work`.
pub const KB_EXTRACT_WORK_INPUT_SCHEMA: &str = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["creator_id"],
            "properties": {
                "job_id": { "type": "string", "description": "Existing extract job ID" },
                "work_entry_id": { "type": "string", "description": "Work-scope KB entry ID to extract" },
                "world_id": { "type": "string", "description": "Target world ID for the resulting KnowledgeEntryRecord" },
                "work_id": { "type": "string", "description": "Source work ID (parent of the chapter)" },
                "work_content": { "type": "string", "description": "Pre-loaded work content" },
                "creator_id": { "type": "string", "description": "Creator ID" },
                "llm_response": { "type": "string", "description": "LLM response text from acp.prompt for finalizing" },
                "source_kind": { "type": "string", "description": "Artifact kind (work_chapter, work_section, etc.)" },
                "source_locator": { "type": "string", "description": "Artifact locator (relative path)" },
                "profile_hint": { "type": "string", "description": "Extract profile (novel, screenplay, essay, generic)" }
            }
        }"#;

/// Input schema for `narrative.compute`.
pub const NARRATIVE_COMPUTE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"module_id":{"type":"string"},"invocation_params":{"type":"object"}},"required":["world_id","creator_id"],"additionalProperties":false}"#;

/// Input schema for `nexus.fork.create`.
pub const NEXUS_FORK_CREATE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"parent_branch_id":{"type":"string"},"forked_from_event_id":{"type":"string"},"label":{"type":"string","minLength":1,"maxLength":200}},"required":["world_id","creator_id","parent_branch_id","forked_from_event_id"],"additionalProperties":false}"#;

/// Input schema for `nexus.llm.extract`.
pub const NEXUS_LLM_EXTRACT_INPUT_SCHEMA: &str = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt", "chapter_prose"],
            "properties": {
                "prompt": { "type": "string", "description": "Extraction instruction template (rendered by LlmExtractTask)" },
                "chapter_prose": { "type": "string", "description": "Verbatim chapter body to extract entities from" },
                "_creator_id": { "type": "string" },
                "_session_id": { "type": "string" }
            }
        }"#;

/// Input schema for `nexus.timeline.event.append`.
pub const NEXUS_TIMELINE_EVENT_APPEND_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"branch_id":{"type":"string"},"event_type":{"type":"string"},"title":{"type":"string"},"summary":{"type":"string"},"event_id":{"type":"string"}},"required":["world_id","creator_id","branch_id","event_type"],"additionalProperties":false}"#;

/// Input schema for `nexus.world.delta.apply`.
pub const NEXUS_WORLD_DELTA_APPLY_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"policy_context":{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"source_work_id":{"type":"string"}},"required":["world_id","creator_id"]},"proposed_changes":{"type":"array","items":{"type":"object","properties":{"entity":{"type":"string"},"entity_id":{"type":"string"},"field":{"type":"string"},"old_value":{},"new_value":{},"rationale":{"type":"string"}},"required":["entity","field","new_value","rationale"]}},"atomic":{"type":"boolean"}},"required":["policy_context","proposed_changes"],"additionalProperties":false}"#;

/// Input schema for `nexus.world.delta.propose`.
pub const NEXUS_WORLD_DELTA_PROPOSE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"changeset":{"type":"array","items":{"type":"object","properties":{"entity":{"type":"string"},"entity_id":{"type":"string"},"field":{"type":"string"},"new_value":{},"rationale":{"type":"string"}},"required":["entity","field","new_value","rationale"]}}},"required":["world_id","creator_id","changeset"],"additionalProperties":false}"#;

/// Input schema for `nexus.world.state.query`.
pub const NEXUS_WORLD_STATE_QUERY_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"slice":{"type":"string","enum":["kb","timeline","all"]},"branch_id":{"type":"string"},"limit":{"type":"integer","minimum":0}},"required":["world_id","creator_id"],"additionalProperties":false}"#;

/// Input schema for `novel.chapter_transition`.
pub const NOVEL_CHAPTER_TRANSITION_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"from_status":{"type":"string"},"to_status":{"type":"string"},"actual_word_count":{"type":"integer","minimum":0},"force":{"type":"boolean","default":false},"reason":{"type":"string"},"workspace_root":{"type":"string"},"work_ref":{"type":"string"},"body_path":{"type":"string"}},"required":["work_id","chapter","from_status","to_status"],"additionalProperties":false}"#;

/// Input schema for `novel.project_scaffold`.
pub const NOVEL_PROJECT_SCAFFOLD_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]},"create_world":{"type":"boolean"},"world_title":{"type":"string"},"world_slug":{"type":"string"},"total_planned_chapters":{"type":"integer","minimum":1},"total_volumes":{"type":"integer","minimum":1,"default":1}},"required":["creator_id","work_id","work_ref","title","total_planned_chapters"],"additionalProperties":false}"#;

/// Input schema for `outbox.compact`.
pub const OUTBOX_COMPACT_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"retentionDays":{"type":"integer","minimum":1,"default":7}},"required":[],"additionalProperties":false}"#;

/// Input schema for `outbox.flush`.
pub const OUTBOX_FLUSH_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"limit":{"type":"integer","minimum":0,"default":0}},"required":[],"additionalProperties":false}"#;

/// Input schema for `registry.refresh`.
pub const REGISTRY_REFRESH_INPUT_SCHEMA: &str =
    r#"{"type":"object","properties":{},"required":[],"additionalProperties":false}"#;

/// Input schema for `script.project_scaffold`.
pub const SCRIPT_PROJECT_SCAFFOLD_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#;

/// Input schema for `script.section_status.update`.
pub const SCRIPT_SECTION_STATUS_UPDATE_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"work_ref":{"type":"string"},"section_path":{"type":"string"},"new_status":{"type":"string","enum":["draft","reviewed","accepted"]},"reason":{"type":"string"},"works_root":{"type":"string"}},"required":["work_ref","section_path","new_status"],"additionalProperties":false}"#;

/// Input schema for `soul.experience.aggregate`.
pub const SOUL_EXPERIENCE_AGGREGATE_INPUT_SCHEMA: &str = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["creator_id", "home_dir"],
            "properties": {
                "creator_id": {
                    "type": "string",
                    "description": "Creator ID to aggregate experience for"
                },
                "home_dir": {
                    "type": "string",
                    "description": "Absolute path to the user home directory"
                }
            },
            "additionalProperties": false
        }"#;

/// Input schema for `sync.pull`.
pub const SYNC_PULL_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"force":{"type":"boolean","default":false}},"required":[],"additionalProperties":false}"#;

/// Input schema for `sync.push`.
pub const SYNC_PUSH_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"force":{"type":"boolean","default":false}},"required":[],"additionalProperties":false}"#;

/// Input schema for `workspace.commit`.
pub const WORKSPACE_COMMIT_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"sessionId":{"type":"string","minLength":1},"changes":{"type":"array","minItems":1,"maxItems":128,"items":{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096,"pattern":"^(?!/)(?!.*\.\.)[^/]+(?:/[^/]+)*$"},"op":{"type":"string","enum":["create","modify","delete"]},"expectedHash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"contentBase64":{"type":"string","maxLength":1398104}},"required":["path","op"],"additionalProperties":false,"allOf":[{"if":{"properties":{"op":{"const":"create"}},"required":["op"]},"then":{"required":["contentBase64"],"not":{"required":["expectedHash"]}},"else":{"required":["expectedHash"]}},{"if":{"properties":{"op":{"const":"delete"}},"required":["op"]},"then":{"not":{"required":["contentBase64"]}},"else":{"required":["contentBase64"]}}]}},"required":["sessionId","changes"],"additionalProperties":false}"#;

/// Input schema for `workspace.open`.
pub const WORKSPACE_OPEN_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096,"pattern":"^(?!/)(?!.*\.\.)[^/]+(?:/[^/]+)*$"}},"required":["path"],"additionalProperties":false}"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// THE property (C3, single table): the roster and the schema lookup are
    /// ONE declaration — every schema-table key appears in the roster and
    /// vice versa, in the same order, and every entry resolves through the
    /// public accessor.
    ///
    /// Under the previous two-handwritten-declarations arrangement this case
    /// was FAILABLE: a key added only to the schema `match` made
    /// `builtin_input_schema` answer while `builtin_names` — the reservation,
    /// collision-ownership and dispatch-discriminator source — lacked it.
    /// Collapsed into one table the equality holds by construction; the test
    /// stays as the tripwire against a second handwritten list reappearing.
    #[test]
    fn roster_and_schema_table_are_one_declaration() {
        let roster: Vec<&str> = builtin_names().collect();
        let schema_keys: Vec<&str> = BUILTIN_CAPABILITIES.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            roster, schema_keys,
            "the roster and the schema table's keys are the same declaration, in the same order"
        );
        for (name, schema) in BUILTIN_CAPABILITIES {
            assert_eq!(
                builtin_input_schema(name),
                Some(*schema),
                "table entry '{name}' resolves through the public accessor"
            );
        }
        // Unknown names fail closed through the same accessor.
        assert_eq!(builtin_input_schema("not.a.declared.builtin"), None);
    }

    /// Count anchor shared with the capability registry: exactly 32 canonical
    /// builtins — `registry_has_32_builtins` and its declaration-subset
    /// assertion read the same table through `builtin_names`.
    #[test]
    fn declaration_has_thirty_two_canonical_names() {
        assert_eq!(builtin_names().count(), 32);
    }
}
