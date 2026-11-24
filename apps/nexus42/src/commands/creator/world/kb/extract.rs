//! World KB extract-queue operator surface — `creator world kb queue-extract`
//! and `creator world kb extract-status`.
//!
//! Rebound from the retired work-scope `kb` group (v231 retire-file-index
//! Task 1). The job is a `kb_extract_jobs` row owned by the core KB extract
//! authority ([`nexus_core::CoreService::queue_kb_extract`],
//! `list_kb_extract_jobs`, `get_kb_extract_job`); the source the extractor
//! reads is the Work artifact named by `--work-id` / `--chapter`, so neither
//! leaf consults — or needs — the retired work-scope file index. The `--chapter`
//! sugar tells the core which chapter body the run reads; the positional
//! label is the job's idempotency key.
//!
//! The enqueue leaf takes the workspace's engine-owned admission
//! ([`crate::core::open_engine_owned_core`]) because `kb_extract_jobs` is
//! guarded to a migration/engine writer at the schema level; the status leaf
//! reads over the direct-writer one.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core, open_engine_owned_core};
use crate::errors::{CliError, Result};
use nexus_core::QueueKbExtractParams;

/// Default maximum number of extract jobs shown when listing without `--job-id`.
const DEFAULT_EXTRACT_STATUS_LIMIT: u32 = 100;

/// `creator world kb queue-extract` — idempotent enqueue of a Work artifact
/// for extraction into a target world.
///
/// Creates a row in `kb_extract_jobs` with status `queued` through the core
/// KB extract authority ([`nexus_core::CoreService::queue_kb_extract`]); the
/// core owns the label sanitization, the chapter-locator sugar and the
/// idempotency key, so this leaf only maps its flags onto the typed request
/// and renders the job. The actual extraction is performed by the
/// `kb.extract_work` capability (triggered via preset or daemon
/// orchestration). No LLM calls here.
///
/// When `--chapter N` is provided, the core sets `source_kind=work_chapter`,
/// `profile_hint=novel`, and records the chapter body locator.
///
/// # Errors
///
/// Returns [`CliError::Other`] for a chapter below 1 (refused before the core
/// is opened, so the legacy wording is the one the caller reads), and the seam
/// / core failures otherwise.
// CLI helper — runs on single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
pub async fn queue_extract(
    config: &CliConfig,
    work_entry_id: &str,
    world_id: &str,
    work_id: Option<&str>,
    chapter: Option<i32>,
) -> Result<()> {
    if let Some(ch) = chapter {
        if ch < 1 {
            return Err(CliError::Other("Chapter number must be >= 1".to_string()));
        }
    }

    let core = open_engine_owned_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.queue_kb_extract(
            &principal,
            QueueKbExtractParams {
                work_entry_id: work_entry_id.to_string(),
                world_id: world_id.to_string(),
                work_id: work_id.map(std::string::ToString::to_string),
                chapter,
            },
        )
        .await
        .map_err(map_core_error)
    }
    .await;
    let job = finish_direct(&core, outcome).await?;

    if job.status == "queued" {
        println!("✓ Extract job queued: {}", job.job_id);
    } else {
        println!("ℹ Extract job already exists: {}", job.job_id);
    }
    println!("  Work entry:  {}", job.work_entry_id);
    println!("  Target world: {world_id}");
    if let Some(sk) = &job.source_kind {
        println!("  Source kind:  {sk}");
    }
    if let Some(sl) = &job.source_locator {
        println!("  Source loc:   {sl}");
    }
    if let Some(ph) = &job.profile_hint {
        println!("  Profile:      {ph}");
    }
    if let Some(wid) = &job.work_id {
        println!("  Work ID:      {wid}");
    }
    println!("  Status:       {}", job.status);
    println!("  Created:      {}", job.created_at);
    Ok(())
}

/// `creator world kb extract-status` — show extract job(s) for the active creator.
///
/// With `--job-id`, shows a specific job. Without it, lists up to
/// `DEFAULT_EXTRACT_STATUS_LIMIT` (100) most recent jobs. Each arm reads
/// through the core and renders only after [`finish_direct`] released the
/// writer.
///
/// # Errors
///
/// Returns [`CliError::Other`] when the requested job does not exist, and the
/// seam / core failures otherwise.
pub async fn extract_status(config: &CliConfig, job_id: Option<&str>) -> Result<()> {
    if let Some(jid) = job_id {
        return show_extract_job(config, jid).await;
    }
    list_extract_jobs(config).await
}

/// `extract-status --job-id <id>` — one job in detail.
async fn show_extract_job(config: &CliConfig, job_id: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.get_kb_extract_job(&principal, job_id)
            .await
            .map_err(map_core_error)
    }
    .await;
    let job = finish_direct(&core, outcome).await?;

    let Some(job) = job else {
        return Err(CliError::Other(format!(
            "Extract job '{job_id}' not found."
        )));
    };
    print_job_detail(&job);
    Ok(())
}

/// `extract-status` — the creator's most recent jobs.
async fn list_extract_jobs(config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let creator_id = principal.creator_id().to_string();
        let jobs = core
            .list_kb_extract_jobs(&principal, DEFAULT_EXTRACT_STATUS_LIMIT)
            .await
            .map_err(map_core_error)?;
        Ok((creator_id, jobs))
    }
    .await;
    let (creator_id, jobs) = finish_direct(&core, outcome).await?;

    if jobs.is_empty() {
        println!("No extract jobs for creator {creator_id}.");
        return Ok(());
    }

    println!(
        "Extract jobs for creator {creator_id} (showing up to {DEFAULT_EXTRACT_STATUS_LIMIT}):"
    );
    println!(
        "{:<20} {:<15} {:<20} {:<20} STATUS",
        "JOB_ID", "WORK_ENTRY", "WORLD", "CREATED"
    );
    for job in &jobs {
        println!(
            "{:<20} {:<15} {:<20} {:<20} {}",
            job.job_id,
            truncate_str(&job.work_entry_id, 15),
            truncate_str(&job.world_id, 20),
            job.created_at,
            job.status,
        );
    }
    Ok(())
}

/// Print a single job in detail.
fn print_job_detail(job: &nexus_local_db::KbExtractJob) {
    println!("Job:           {}", job.job_id);
    println!("  Creator:     {}", job.creator_id);
    println!("  Workspace:   {}", job.workspace_id);
    println!("  Work entry:  {}", job.work_entry_id);
    println!("  World:       {}", job.world_id);
    println!("  Status:      {}", job.status);
    println!("  Created:     {}", job.created_at);
    if let Some(started) = &job.started_at {
        println!("  Started:     {started}");
    }
    if let Some(finished) = &job.finished_at {
        println!("  Finished:    {finished}");
    }
    if let Some(err) = &job.error_text {
        println!("  Error:       {err}");
    }
}

/// Truncate a string for tabular display.
///
/// `max_len` counts characters, and the cut lands on a character boundary: an
/// input of at most `max_len` characters is returned unchanged, otherwise the
/// kept prefix is exactly `max_len - 1` characters plus `…`, so a multi-byte
/// label never slices mid-character and never gains a spurious ellipsis. ASCII
/// output is unchanged (`chars().count() == len()` for ASCII).
fn truncate_str(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let cut = max_len - 1;
    let end = s.char_indices().nth(cut).map_or(s.len(), |(idx, _)| idx);
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod truncate_str_tests {
    use super::truncate_str;

    #[test]
    fn ascii_short_input_is_unchanged() {
        assert_eq!(truncate_str("abc", 5), "abc");
    }

    #[test]
    fn ascii_truncation_replaces_last_unit_with_ellipsis() {
        assert_eq!(truncate_str("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn multi_byte_label_cuts_on_char_boundary() {
        // cut == max_len - 1 == 2 keeps 2 characters; a byte slice at 2 would
        // split the first 3-byte char.
        let label = "知识抽取失败原因很长";
        let out = truncate_str(label, 3);
        assert_eq!(out, "知识…");
        assert!(out.ends_with('…'));
        assert!(out.len() < label.len());
    }

    #[test]
    fn multi_byte_label_within_char_budget_is_unchanged() {
        // 8 characters but 24 bytes: the character budget fits, so no ellipsis.
        let label = "知识抽取失败原因";
        assert!(label.len() > 15);
        assert_eq!(truncate_str(label, 15), label);
    }

    #[test]
    fn zero_max_len_returns_empty() {
        assert_eq!(truncate_str("abcdefghij", 0), "");
    }
}
