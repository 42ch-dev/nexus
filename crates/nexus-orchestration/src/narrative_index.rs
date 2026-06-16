//! Narrative indexes runtime — F### foreshadowing + E### event index (V1.49 P1).
//!
//! Implements the minimum viable index runtime described in
//! `.mstar/knowledge/specs/novel-writing/narrative-indexes.md` (Draft V1.49):
//! parse / serialize the `Works/<work_ref>/Outlines/foreshadowing.md` table,
//! allocate sequential `F###` ids, promote inline declarations authored in a
//! chapter outline's `## Foreshadowing Touched (F###)` section, and expose a
//! compact summary for prompt injection.
//!
//! ## Row schema (ground truth = scaffolded template)
//!
//! The scaffolded `Outlines/foreshadowing.md` (template
//! `embedded-presets/novel-{writing,project-init}/templates/foreshadowing.md`,
//! identical) ships this canonical 5-column table header:
//!
//! ```markdown
//! | ID | Description | Planted | Paid off | Status |
//! | --- | --- | --- | --- | --- |
//! ```
//!
//! The Draft overlay §3 summarizes the schema as `id | description | status |
//! chapters` and claims alignment with the embedded template. The template is
//! the on-disk ground truth (it is what [`crate::capability::builtins::NovelProjectScaffold`]
//! writes and what the round-trip tests must reproduce), so this module
//! implements the **template's 5-column** shape and maps the overlay semantics
//! onto it: `Planted` + `Paid off` realize the overlay's single `chapters`
//! column more precisely; `Status` honours the overlay's
//! `planned | buried | paid_off` vocabulary. The overlay §3 table is to be
//! reconciled at the P5 fold-into-`workflow-profile.md` step (see residual
//! `R-V149P1-01`).
//!
//! ## Concurrency
//!
//! Promotion performs an atomic temp-file + rename write so a crash never
//! leaves a torn `foreshadowing.md`. Concurrent promotion safety relies on the
//! same single-writer daemon assumption documented on
//! [`crate::capability::builtins::NovelProjectScaffold`] (pre-1.0 single-user):
//! only one `novel-writing` schedule terminates at a time per Work. A hard
//! advisory lock is intentionally not added in this slice to avoid lock-file
//! leak hazards on panic (see completion report follow-ups).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Canonical table geometry
// ---------------------------------------------------------------------------

/// Canonical header row for `foreshadowing.md` (matches the scaffolded template).
const FORESHADOWING_HEADER: &str = "| ID | Description | Planted | Paid off | Status |";
/// Canonical separator row for `foreshadowing.md`.
const FORESHADOWING_SEPARATOR: &str = "| --- | --- | --- | --- | --- |";

/// Canonical header row for `event-index.md` (matches the scaffolded template).
const EVENT_HEADER: &str = "| ID | Description | Chapter | Impact |";
/// Canonical separator row for `event-index.md`.
const EVENT_SEPARATOR: &str = "| --- | --- | --- | --- | --- |";

/// Expected number of columns in a foreshadowing data row.
const FORESHADOWING_COL_COUNT: usize = 5;

// ---------------------------------------------------------------------------
// Foreshadowing status (typed closed vocabulary — overlay §3)
// ---------------------------------------------------------------------------

/// Closed status vocabulary for a foreshadowing row (overlay §3).
///
/// Canonical wire strings (emitted by [`Display`], parsed by [`FromStr`]):
/// `"planned"` | `"buried"` | `"paid_off"`.
///
/// `FromStr` is **case-insensitive** to tolerate author typos in hand-edited
/// `foreshadowing.md` files (e.g. `Planned`, `PAID_OFF`); `Display` always
/// emits canonical lowercase + underscore so the serialize/parse round-trip is
/// stable regardless of how the value was originally cased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeshadowingStatus {
    /// Seed is declared but not yet planted in the narrative.
    Planned,
    /// Seed is planted but will intentionally never pay off.
    Buried,
    /// Seed has resolved / paid off.
    PaidOff,
}

impl ForeshadowingStatus {
    /// Canonical lowercase wire string for the variant.
    #[must_use]
    pub const fn as_canonical_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Buried => "buried",
            Self::PaidOff => "paid_off",
        }
    }
}

impl std::fmt::Display for ForeshadowingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_canonical_str())
    }
}

impl FromStr for ForeshadowingStatus {
    type Err = ForeshadowingStatusError;

    /// Parse a status cell into a typed variant.
    ///
    /// Tolerates surrounding whitespace and mixed casing. Only the three
    /// canonical underscore forms are recognised (e.g. `paid_off`, not
    /// `paidoff` / `paid-off`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "planned" => Ok(Self::Planned),
            "buried" => Ok(Self::Buried),
            "paid_off" => Ok(Self::PaidOff),
            _ => Err(ForeshadowingStatusError {
                input: s.to_string(),
            }),
        }
    }
}

/// Error returned when a status cell does not match the closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeshadowingStatusError {
    /// The rejected input value (as found in the source document).
    pub input: String,
}

impl std::fmt::Display for ForeshadowingStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid foreshadowing status {:?} \
             (expected one of: planned, buried, paid_off)",
            self.input
        )
    }
}

impl std::error::Error for ForeshadowingStatusError {}

/// Structured error from foreshadowing index table parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexParseError {
    /// A status cell did not match the [`ForeshadowingStatus`] vocabulary.
    InvalidStatus {
        /// Zero-based index of the offending data row within the parsed table
        /// (counting every `|`-delimited data row, including placeholder rows
        /// that were skipped due to empty id).
        row_index: usize,
        /// The rejected status value as found in the source document.
        value: String,
    },
}

impl std::fmt::Display for IndexParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStatus { row_index, value } => {
                write!(
                    f,
                    "invalid foreshadowing status {value:?} at data row {row_index} \
                     (expected one of: planned, buried, paid_off)"
                )
            }
        }
    }
}

impl std::error::Error for IndexParseError {}

// ---------------------------------------------------------------------------
// Foreshadowing row
// ---------------------------------------------------------------------------

/// A single `F###` row in `Outlines/foreshadowing.md`.
///
/// Field order matches the canonical table header
/// (`ID | Description | Planted | Paid off | Status`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeshadowingRow {
    /// `F` + three-digit zero-padded integer (e.g. `F001`).
    pub id: String,
    /// Short human label / what is foreshadowed.
    pub description: String,
    /// Chapter number where the seed is planted (empty until planted).
    pub planted: String,
    /// Chapter number where it resolves (empty until resolved).
    pub paid_off: String,
    /// Lifecycle status (overlay §3 closed vocabulary, typed).
    pub status: ForeshadowingStatus,
}

impl ForeshadowingRow {
    /// Build a freshly-allocated row with `planned` status and empty chapter
    /// columns (the shape produced by promotion of a new declaration).
    const fn new_allocated(id: String, description: String) -> Self {
        Self {
            id,
            description,
            planted: String::new(),
            paid_off: String::new(),
            status: ForeshadowingStatus::Planned,
        }
    }
}

/// A single `E###` row in `Outlines/event-index.md` (P1 read-only surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRow {
    /// `E` + three-digit zero-padded integer (e.g. `E001`).
    pub id: String,
    /// What happens.
    pub description: String,
    /// Chapter number where the event occurs.
    pub chapter: String,
    /// Downstream consequences.
    pub impact: String,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse the `foreshadowing.md` markdown table into rows.
///
/// Tolerates the surrounding prose (title, bullet list, trailing stub note)
/// emitted by the scaffold template: only lines that look like table data rows
/// (`| ... |`) following the header + separator are decoded. An empty or
/// table-less file yields an empty `Vec`.
///
/// Rows whose `ID` cell is empty (e.g. the scaffold's placeholder
/// `| | | | | |` row) are skipped.
///
/// # Errors
///
/// Returns [`IndexParseError::InvalidStatus`] when a data row's status cell
/// does not parse into [`ForeshadowingStatus`] (the overlay §3 closed
/// vocabulary `planned | buried | paid_off`). The error carries the
/// zero-based data-row index and the rejected value so the caller can point
/// the author at the offending line. Rows decoded before the error are
/// discarded (the whole table is rejected atomically on any invalid status).
pub fn parse_foreshadowing_index(content: &str) -> Result<Vec<ForeshadowingRow>, IndexParseError> {
    let table = parse_table(content, FORESHADOWING_COL_COUNT);
    let mut rows = Vec::with_capacity(table.len());
    for (row_index, cells) in table.into_iter().enumerate() {
        let id = cells.first().map_or("", String::as_str).trim();
        if id.is_empty() {
            continue;
        }
        let status_value = cell(&cells, 4);
        let status = ForeshadowingStatus::from_str(&status_value).map_err(|_| {
            IndexParseError::InvalidStatus {
                row_index,
                value: status_value,
            }
        })?;
        rows.push(ForeshadowingRow {
            id: id.to_string(),
            description: cell(&cells, 1),
            planted: cell(&cells, 2),
            paid_off: cell(&cells, 3),
            status,
        });
    }
    Ok(rows)
}

/// Parse the `event-index.md` markdown table into rows (P1 read stub).
///
/// Full E### CRUD is deferred to V1.50 per overlay §4; this read path lets the
/// prompt surface existing events when the file is populated. An empty or
/// table-less file yields an empty `Vec`.
#[must_use]
pub fn parse_event_index(content: &str) -> Vec<EventRow> {
    parse_table(content, 4)
        .into_iter()
        .filter_map(|cells| {
            let id = cells.first()?.trim();
            if id.is_empty() {
                return None;
            }
            Some(EventRow {
                id: id.to_string(),
                description: cell(&cells, 1),
                chapter: cell(&cells, 2),
                impact: cell(&cells, 3),
            })
        })
        .collect()
}

/// Split a markdown document into table data rows.
///
/// Scans for the first header/separator pair (a `|`-delimited row immediately
/// followed by a `| --- |`-style separator), then collects every subsequent
/// `|`-delimited row until a blank line or a non-table line. Each row is split
/// into trimmed cells; the leading/trailing empty cells produced by the
/// bounding `|` characters are dropped. Rows are padded/truncated to
/// `col_count` so callers can index by position.
fn parse_table(content: &str, col_count: usize) -> Vec<Vec<String>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut rows = Vec::new();

    // Locate the header: the first pipe-row whose following line is a separator.
    let mut i = 0;
    let mut header_at = None;
    while i + 1 < lines.len() {
        if is_table_row(lines[i]) && is_separator_row(lines[i + 1]) {
            header_at = Some(i);
            break;
        }
        i += 1;
    }
    let Some(header_idx) = header_at else {
        return Vec::new();
    };

    // Data rows start after header + separator.
    let mut idx = header_idx + 2;
    while idx < lines.len() {
        let line = lines[idx];
        if !is_table_row(line) {
            // A blank line or prose ends the table block.
            break;
        }
        let cells = split_row(line, col_count);
        rows.push(cells);
        idx += 1;
    }
    rows
}

/// `true` if the line is a pipe-delimited table row (not a separator).
fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && !is_separator_row(line)
}

/// `true` if the line is a markdown table separator (`| --- | --- | …`).
fn is_separator_row(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('|') {
        return false;
    }
    t.trim_matches('|')
        .split('|')
        .all(|cell| cell.trim().trim_matches('-').is_empty() && !cell.trim().is_empty())
}

/// Split a `| a | b | c |` row into exactly `col_count` trimmed cells.
///
/// The bounding pipes and any extra trailing empty cells are collapsed; short
/// rows are padded with empty strings so positional access is safe.
fn split_row(line: &str, col_count: usize) -> Vec<String> {
    let mut cells: Vec<String> = line
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect();
    // Drop trailing empties introduced by a trailing `| ` but keep at least
    // `col_count` slots when the row is well-formed.
    while cells.len() > col_count && cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    cells.resize(col_count, String::new());
    cells
}

/// Safe positional cell accessor.
fn cell(cells: &[String], idx: usize) -> String {
    cells.get(idx).cloned().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

/// Serialize rows back into the canonical `foreshadowing.md` table.
///
/// Emits the canonical header + separator (byte-identical to the scaffolded
/// template) followed by one row per entry, preserving the
/// `ID | Description | Planted | Paid off | Status` column order. Empty input
/// emits just the header + separator + a single empty placeholder row so the
/// file stays a valid table ready for manual editing.
#[must_use]
pub fn serialize_foreshadowing_index(rows: &[ForeshadowingRow]) -> String {
    let mut lines: Vec<String> = vec![
        "# Foreshadowing Index".to_string(),
        String::new(),
        FORESHADOWING_HEADER.to_string(),
        FORESHADOWING_SEPARATOR.to_string(),
    ];
    if rows.is_empty() {
        lines.push("| | | | | |".to_string());
    } else {
        for r in rows {
            lines.push(format!(
                "| {} | {} | {} | {} | {}",
                r.id, r.description, r.planted, r.paid_off, r.status
            ));
        }
    }
    format!("{}\n", lines.join("\n"))
}

/// Serialize rows into the canonical `event-index.md` table (P1 writer stub).
#[must_use]
pub fn serialize_event_index(rows: &[EventRow]) -> String {
    let mut lines: Vec<String> = vec![
        "# Event Index".to_string(),
        String::new(),
        EVENT_HEADER.to_string(),
        EVENT_SEPARATOR.to_string(),
    ];
    if rows.is_empty() {
        lines.push("| | | | |".to_string());
    } else {
        for r in rows {
            lines.push(format!(
                "| {} | {} | {} | {}",
                r.id, r.description, r.chapter, r.impact
            ));
        }
    }
    format!("{}\n", lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Id allocation
// ---------------------------------------------------------------------------

/// Return the next sequential `F###` id (`max(existing numeric suffix) + 1`,
/// per overlay §3.1). With no rows, returns `F001`.
#[must_use]
pub fn next_f_id(rows: &[ForeshadowingRow]) -> String {
    next_seq_id(rows.iter().map(|r| r.id.as_str()), 'F')
}

/// Return the next sequential `E###` id (helper for the deferred E### writer).
#[must_use]
pub fn next_e_id(rows: &[EventRow]) -> String {
    next_seq_id(rows.iter().map(|r| r.id.as_str()), 'E')
}

/// Core sequential allocator shared by F### / E###.
///
/// Parses the numeric suffix of every id matching `<prefix><digits>` and returns
/// `<prefix>{max+1:03}`. Ids that do not match the expected shape are ignored.
fn next_seq_id<'a, I>(ids: I, prefix: char) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let max = ids
        .into_iter()
        .filter_map(|id| {
            let rest = id.strip_prefix(prefix)?;
            // Only well-formed numeric suffixes participate.
            rest.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    // `max + 1`: overlay §3.1 "next id = max(existing numeric suffix) + 1".
    // `checked_add` keeps the nursery `arithmetic_side_effects` lint quiet;
    // overflow is impossible for realistic chapter counts.
    let next = max
        .checked_add(1)
        .expect("foreshadowing id suffix overflow");
    format!("{prefix}{next:03}")
}

// ---------------------------------------------------------------------------
// Outline section extraction + inline declarations
// ---------------------------------------------------------------------------

/// One inline foreshadowing declaration authored in a chapter outline's
/// `## Foreshadowing Touched (F###)` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FDeclaration {
    /// `Some("F001")` when the author wrote an explicit id; `None` means
    /// "declare this title, allocate the next `F###`".
    pub id: Option<String>,
    /// Human label / description text.
    pub description: String,
}

/// Extract the `## Foreshadowing Touched (F###)` section body from a chapter
/// outline document.
///
/// Returns the raw section text (everything after the section header up to the
/// next same-or-higher-level heading or end of document), or `None` when the
/// section is absent. Used by the promotion hook to feed
/// [`promote_outline_to_index`].
#[must_use]
pub fn extract_foreshadowing_section(outline_content: &str) -> Option<String> {
    let mut lines = outline_content.lines();
    // Advance to the section header.
    let header = lines.find(|line| {
        let t = line.trim_start();
        // Match `## Foreshadowing Touched` (level-2 heading per outline-chapter.md).
        t.starts_with("## ") && t.contains("Foreshadowing Touched")
    })?;
    let _ = header; // positioned just past the header
    let mut body = String::new();
    for line in lines {
        let t = line.trim_start();
        // Stop at the next heading of level <= 2 (a sibling/parent section).
        if t.starts_with("# ") || t.starts_with("## ") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    if body.trim().is_empty() {
        None
    } else {
        Some(body)
    }
}

/// Parse inline `F###` declarations from a "Foreshadowing Touched" section body.
///
/// Only bullets that contain an explicit `F###` token (with at least one
/// digit, followed by `:` or whitespace) are yielded as declarations. Bullets
/// without the `F###` marker — notes, TODOs, prose — are **ignored** so the
/// index is not polluted with spurious allocated ids (W-2 policy).
///
/// Recognized forms (the canonical form is `F###: description` per the
/// `outline-chapter.md` prompt; a space delimiter is also tolerated for
/// robustness):
///
/// - `F001: the locket`            → `(Some("F001"), "the locket")`
/// - `- F001: the locket`          → `(Some("F001"), "the locket")` (bullet)
/// - `F001 the locket`             → `(Some("F001"), "the locket")`
/// - `- F001 already planted`      → `(Some("F001"), "already planted")`
/// - `- This is just a note`       → ignored (no `F###` token)
/// - `- TODO: resolve the locket`  → ignored (no `F###` token)
///
/// The "No foreshadowing items touched …" sentinel emitted by the prompt when
/// nothing is touched is skipped (never treated as a declaration).
#[must_use]
pub fn extract_inline_f_declarations(outline_section: &str) -> Vec<FDeclaration> {
    outline_section
        .lines()
        .map(str::trim_start)
        .filter_map(parse_declaration_line)
        .collect()
}

/// Parse a single line into a declaration, or `None` if it is not one.
///
/// A declaration requires an explicit `F###` token (at least one digit after
/// `F`, followed by `:` or whitespace, with non-empty description text). Lines
/// without such a token — including non-`F###` bullets — return `None`.
fn parse_declaration_line(line: &str) -> Option<FDeclaration> {
    let body = line.trim();
    if body.is_empty() {
        return None;
    }
    // Skip the explicit "nothing touched" sentinel from the prompt.
    if body.to_ascii_lowercase().contains("no foreshadowing") {
        return None;
    }
    // Strip a leading bullet (`-` / `*`).
    let stripped = body
        .strip_prefix("- ")
        .or_else(|| body.strip_prefix("* "))
        .unwrap_or(body)
        .trim();
    if stripped.is_empty() {
        return None;
    }
    // `F###` with at least one digit, followed by `:` or whitespace.
    if let Some(rest) = stripped.strip_prefix('F') {
        if let Some(digits_end) = rest.find(|c: char| !c.is_ascii_digit()) {
            let (digits, tail) = rest.split_at(digits_end);
            if !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit())
                && (tail.starts_with(':') || tail.starts_with(char::is_whitespace))
            {
                let desc = tail.trim_start_matches(':').trim().to_string();
                if desc.is_empty() {
                    return None;
                }
                return Some(FDeclaration {
                    id: Some(format!("F{digits}")),
                    description: desc,
                });
            }
        } else if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            // `F001` with no description on the line — treat as id-only ref,
            // not a declaration (nothing to promote). Skip.
            return None;
        }
    }
    // W-2: bullets without an explicit `F###` token are NOT declarations.
    // Notes, TODOs, and prose are ignored to avoid polluting the index with
    // spurious allocated ids.
    None
}

// ---------------------------------------------------------------------------
// Promotion (read-modify-write `foreshadowing.md`)
// ---------------------------------------------------------------------------

/// Path to a Work's `foreshadowing.md` index under its Work directory.
fn foreshadowing_index_path(work_dir: &Path) -> PathBuf {
    work_dir.join("Outlines").join("foreshadowing.md")
}

/// Promote inline `F###` declarations from a chapter outline section into the
/// Work's `foreshadowing.md` index.
///
/// Reads the current index (creating it fresh if absent), then for each
/// declaration carrying an explicit `F###` token:
///
/// - `F###` not already in the index → append a new `planned` row.
/// - `F###` already present with the **same** description → no-op
///   (idempotent re-promotion).
/// - `F###` already present with a **conflicting** description → error
///   (overlay §3.1: "Duplicate id with conflicting description → fail …").
///
/// **W-2 policy**: only bullets containing an explicit `F###` token are
/// eligible for promotion (see [`extract_inline_f_declarations`]). Notes,
/// TODOs, and prose without the `F###` marker are ignored — they are NOT
/// allocated a new id. This keeps the index free of spurious ids from
/// non-declaration bullets.
///
/// The file is rewritten atomically (temp file + rename) so a crash never
/// leaves a torn index. Returns the list of **newly appended** `F###` ids
/// (explicit ids that were not previously indexed).
///
/// `work_dir` is the Work directory `Works/<work_ref>/` (the parent of
/// `Outlines/`).
///
/// # Errors
///
/// Returns an error if the index file exists but cannot be read, the temp file
/// cannot be written, the atomic rename fails, a data row has an invalid
/// status cell (see [`IndexParseError`]), or a conflicting-description
/// duplicate is detected.
pub fn promote_outline_to_index(work_dir: &Path, outline_section: &str) -> Result<Vec<String>> {
    let index_path = foreshadowing_index_path(work_dir);
    let existing_content = match std::fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("read foreshadowing index {}", index_path.display()))
        }
    };
    let mut rows = parse_foreshadowing_index(&existing_content)?;

    let declarations = extract_inline_f_declarations(outline_section);
    let mut allocated = Vec::new();
    for decl in declarations {
        // W-2: `extract_inline_f_declarations` only yields declarations that
        // carry an explicit `F###` token, so `decl.id` is always `Some`.
        // Defensive guard: if a future change reintroduces id-less
        // declarations, skip them rather than allocating silently.
        let Some(id) = decl.id else {
            continue;
        };
        if let Some(row) = rows.iter().find(|r| r.id == id) {
            if row.description == decl.description {
                // Idempotent: same id + same description already indexed.
                continue;
            }
            anyhow::bail!(
                "foreshadowing id {id} already exists with a different \
                 description (existing: {:?}, new: {:?}); reconcile the \
                 outline or edit foreshadowing.md manually",
                row.description,
                decl.description
            );
        }
        allocated.push(id.clone());
        rows.push(ForeshadowingRow::new_allocated(id, decl.description));
    }

    if allocated.is_empty() {
        // Nothing to do — avoid touching the file's mtime.
        return Ok(Vec::new());
    }

    let serialized = serialize_foreshadowing_index(&rows);
    atomic_write(&index_path, &serialized)?;
    Ok(allocated)
}

/// Write `contents` to `path` atomically: write to `<path>.tmp` then rename.
fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let mut tmp = path.to_path_buf();
    tmp.set_extension("md.tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("write temp {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Summary for prompt injection
// ---------------------------------------------------------------------------

/// Read `Works/<work_ref>/Outlines/foreshadowing.md` and return a compact
/// one-line-per-row Markdown summary for outline/draft prompt injection.
///
/// Returns `None` when the file is missing, empty, or contains no rows, so the
/// caller's `{{#if foreshadowing_summary}}` template guard omits the section
/// (no empty-sentinel noise — mirrors the `open_findings_block` contract).
/// A parse error (e.g. an invalid status cell) is logged at `warn!` and the
/// summary is omitted (`None`) so prompt injection degrades gracefully rather
/// than surfacing a corrupt index to the model.
///
/// `work_dir` is the Work directory `Works/<work_ref>/`.
#[must_use]
pub fn read_foreshadowing_summary(work_dir: &Path) -> Option<String> {
    let index_path = foreshadowing_index_path(work_dir);
    let content = std::fs::read_to_string(&index_path).ok()?;
    let rows = match parse_foreshadowing_index(&content) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                path = %index_path.display(),
                error = %e,
                "foreshadowing-summary: index parse failed; summary omitted"
            );
            return None;
        }
    };
    if rows.is_empty() {
        return None;
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| format!("- {} | {} | {}", r.id, r.description, r.status))
        .collect();
    Some(lines.join("\n"))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── parse_foreshadowing_index ──────────────────────────────────────────

    #[test]
    fn parse_foreshadowing_index_handles_empty_file() {
        assert!(parse_foreshadowing_index("").unwrap().is_empty());
        assert!(parse_foreshadowing_index("# just a title\n\nno table here")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn parse_foreshadowing_index_handles_full_table() {
        // Content mirrors the scaffolded template shape + real rows.
        let content = "# Foreshadowing Index\n\nintro prose\n\n\
            | ID | Description | Planted | Paid off | Status |\n\
            | --- | --- | --- | --- | --- |\n\
            | F001 | the locket | 1 |  | planned |\n\
            | F002 | the prophecy | 3 | 7 | paid_off |\n\
            \n---\n\nstub note\n";
        let rows = parse_foreshadowing_index(content).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "F001");
        assert_eq!(rows[0].description, "the locket");
        assert_eq!(rows[0].planted, "1");
        assert_eq!(rows[0].paid_off, "");
        assert_eq!(rows[0].status, ForeshadowingStatus::Planned);
        assert_eq!(rows[1].id, "F002");
        assert_eq!(rows[1].paid_off, "7");
        assert_eq!(rows[1].status, ForeshadowingStatus::PaidOff);
    }

    #[test]
    fn parse_foreshadowing_index_skips_placeholder_row() {
        // The scaffolded stub row `| | | | | |` must not yield a phantom row.
        let content = "\
            | ID | Description | Planted | Paid off | Status |\n\
            | --- | --- | --- | --- | --- |\n\
            | | | | | |\n";
        assert!(parse_foreshadowing_index(content).unwrap().is_empty());
    }

    #[test]
    fn parse_foreshadowing_index_parses_scaffolded_template_verbatim() {
        // The actual scaffolded file (prose + table + trailing stub).
        let content = include_str!("../embedded-presets/novel-writing/templates/foreshadowing.md");
        let rows = parse_foreshadowing_index(content).unwrap();
        assert!(rows.is_empty(), "empty scaffold must parse to zero rows");
    }

    // ── serialize round-trip ───────────────────────────────────────────────

    #[test]
    fn serialize_then_parse_roundtrip_is_stable() {
        let rows = vec![
            ForeshadowingRow {
                id: "F001".to_string(),
                description: "the locket".to_string(),
                planted: "1".to_string(),
                paid_off: String::new(),
                status: ForeshadowingStatus::Planned,
            },
            ForeshadowingRow {
                id: "F002".to_string(),
                description: "the prophecy".to_string(),
                planted: "3".to_string(),
                paid_off: "7".to_string(),
                status: ForeshadowingStatus::PaidOff,
            },
        ];
        let serialized = serialize_foreshadowing_index(&rows);
        let reparsed = parse_foreshadowing_index(&serialized).unwrap();
        assert_eq!(reparsed, rows);
        // Header is byte-identical to the scaffolded template.
        assert!(serialized.contains(FORESHADOWING_HEADER));
        assert!(serialized.contains(FORESHADOWING_SEPARATOR));
    }

    #[test]
    fn serialize_empty_emits_valid_table() {
        let s = serialize_foreshadowing_index(&[]);
        assert!(s.contains(FORESHADOWING_HEADER));
        assert!(parse_foreshadowing_index(&s).unwrap().is_empty());
    }

    // ── next_f_id ──────────────────────────────────────────────────────────

    #[test]
    fn next_f_id_allocates_sequentially() {
        assert_eq!(next_f_id(&[]), "F001");
        let rows = vec![row("F001"), row("F002")];
        assert_eq!(next_f_id(&rows), "F003");
        // max+1 (not count+1) per overlay §3.1 — a gap is preserved.
        let rows = vec![row("F001"), row("F003")];
        assert_eq!(next_f_id(&rows), "F004");
    }

    fn row(id: &str) -> ForeshadowingRow {
        ForeshadowingRow {
            id: id.to_string(),
            description: String::new(),
            planted: String::new(),
            paid_off: String::new(),
            status: ForeshadowingStatus::Planned,
        }
    }

    // ── event-index read stub ──────────────────────────────────────────────

    #[test]
    fn parse_event_index_reads_populated_table() {
        let content = "\
            | ID | Description | Chapter | Impact |\n\
            | --- | --- | --- | --- |\n\
            | E001 | the coronation | 2 | sets up the coup |\n";
        let rows = parse_event_index(content);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "E001");
        assert_eq!(rows[0].chapter, "2");
        assert_eq!(next_e_id(&rows), "E002");
    }

    #[test]
    fn parse_event_index_handles_empty_file() {
        assert!(parse_event_index("").is_empty());
    }

    // ── extract_inline_f_declarations ──────────────────────────────────────

    #[test]
    fn extract_inline_f_declarations_handles_dash_form() {
        let section = "\
            - F001: the locket\n\
            - F002: the prophecy\n";
        let decls = extract_inline_f_declarations(section);
        assert_eq!(decls.len(), 2);
        assert_eq!(
            decls[0],
            FDeclaration {
                id: Some("F001".to_string()),
                description: "the locket".to_string()
            }
        );
        assert_eq!(decls[1].id.as_deref(), Some("F002"));
    }

    #[test]
    fn extract_inline_f_declarations_handles_inline_form() {
        // No bullet, colon form.
        let section = "F001: the locket\n";
        let decls = extract_inline_f_declarations(section);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].id.as_deref(), Some("F001"));
        assert_eq!(decls[0].description, "the locket");
    }

    #[test]
    fn extract_inline_f_declarations_handles_space_form() {
        // Tolerated space delimiter.
        let decls = extract_inline_f_declarations("- F007 the scar");
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].id.as_deref(), Some("F007"));
        assert_eq!(decls[0].description, "the scar");
    }

    #[test]
    fn extract_inline_f_declarations_skips_nothing_touched_sentinel() {
        let section = "No foreshadowing items touched in this chapter.\n";
        assert!(extract_inline_f_declarations(section).is_empty());
    }

    #[test]
    fn extract_inline_f_declarations_ignores_bullets_without_f_token() {
        // W-2: bullets without an explicit `F###` token are NOT declarations.
        let section = "\
            - This is a note\n\
            - TODO: resolve the locket payoff next chapter\n\
            - (no new items, just touching F001)\n\
            - F001: the dagger\n";
        let decls = extract_inline_f_declarations(section);
        assert_eq!(
            decls.len(),
            1,
            "only the F### bullet should be a declaration"
        );
        assert_eq!(decls[0].id.as_deref(), Some("F001"));
        assert_eq!(decls[0].description, "the dagger");
    }

    #[test]
    fn extract_inline_f_declarations_handles_bullet_with_existing_f_id() {
        // A bullet that references an existing F### with a short note.
        let decls = extract_inline_f_declarations("- F001 already planted");
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].id.as_deref(), Some("F001"));
        assert_eq!(decls[0].description, "already planted");
    }

    // ── extract_foreshadowing_section ──────────────────────────────────────

    #[test]
    fn extract_foreshadowing_section_finds_body() {
        let outline = "\
            # Chapter 1 Outline\n\n\
            ## Opening Scene\nstuff\n\n\
            ## Foreshadowing Touched (F###)\n\n\
            - F001: the locket\n\n\
            ## Ending Hook\nmore\n";
        let body = extract_foreshadowing_section(outline).expect("section present");
        assert!(body.contains("F001: the locket"));
        assert!(!body.contains("Ending Hook"));
    }

    #[test]
    fn extract_foreshadowing_section_absent_returns_none() {
        assert!(extract_foreshadowing_section("# Chapter 1\n\nno section\n").is_none());
    }

    // ── promote_outline_to_index ───────────────────────────────────────────

    fn work_dir() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let work = tmp.path().to_path_buf();
        std::fs::create_dir_all(work.join("Outlines")).unwrap();
        (tmp, work)
    }

    #[test]
    fn promote_outline_to_index_appends_new_f_ids() {
        let (_tmp, work) = work_dir();
        let section = "- F001: the locket\n- F002: the prophecy\n";
        let allocated = promote_outline_to_index(&work, section).unwrap();
        assert_eq!(allocated, vec!["F001".to_string(), "F002".to_string()]);

        let written = fs::read_to_string(work.join("Outlines/foreshadowing.md")).unwrap();
        let rows = parse_foreshadowing_index(&written).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "F001");
        assert_eq!(rows[0].status, ForeshadowingStatus::Planned);
    }

    #[test]
    fn promote_outline_to_index_creates_file_when_absent() {
        let (_tmp, work) = work_dir();
        let allocated = promote_outline_to_index(&work, "F001: the locket").unwrap();
        assert_eq!(allocated, vec!["F001".to_string()]);
        assert!(work.join("Outlines/foreshadowing.md").is_file());
    }

    #[test]
    fn promote_outline_to_index_does_not_duplicate_existing_f_id() {
        let (_tmp, work) = work_dir();
        // First promotion.
        promote_outline_to_index(&work, "- F001: the locket").unwrap();
        // Re-promote the SAME id + description → no-op, no duplicate.
        let allocated = promote_outline_to_index(&work, "- F001: the locket").unwrap();
        assert!(allocated.is_empty());

        let rows = parse_foreshadowing_index(
            &fs::read_to_string(work.join("Outlines/foreshadowing.md")).unwrap(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn promote_outline_to_index_errors_on_conflicting_description() {
        let (_tmp, work) = work_dir();
        promote_outline_to_index(&work, "- F001: the locket").unwrap();
        let err = promote_outline_to_index(&work, "- F001: a different thing").unwrap_err();
        assert!(
            err.to_string().contains("different description"),
            "expected conflict error, got: {err}"
        );
    }

    #[test]
    fn promote_outline_to_index_does_not_allocate_for_prose_bullets() {
        // W-2: prose bullets without an explicit `F###` token must not allocate
        // new ids. Only the F###-tokened bullet is promoted.
        let (_tmp, work) = work_dir();
        promote_outline_to_index(&work, "F001: the locket").unwrap();
        let allocated = promote_outline_to_index(
            &work,
            "- F002: the dagger\n\
             - Note: chapter is darker than planned\n\
             - TODO: resolve the locket payoff next chapter\n",
        )
        .unwrap();
        assert_eq!(allocated, vec!["F002".to_string()]);

        let rows = parse_foreshadowing_index(
            &fs::read_to_string(work.join("Outlines/foreshadowing.md")).unwrap(),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.id == "F001"));
        assert!(rows.iter().any(|r| r.id == "F002"));
        // No spurious ids allocated for the prose bullets.
        assert!(!rows.iter().any(|r| r.id == "F003"));
    }

    #[test]
    fn promote_outline_to_index_is_atomic_no_tmp_left_behind() {
        let (_tmp, work) = work_dir();
        promote_outline_to_index(&work, "F001: the locket").unwrap();
        // The temp file must have been renamed away.
        assert!(!work.join("Outlines/foreshadowing.md.tmp").exists());
    }

    #[test]
    fn promote_outline_to_index_noop_section_does_not_touch_mtime() {
        let (_tmp, work) = work_dir();
        promote_outline_to_index(&work, "F001: the locket").unwrap();
        let path = work.join("Outlines/foreshadowing.md");
        let mtime_before = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        // Empty section → nothing to promote → no write.
        let allocated = promote_outline_to_index(&work, "").unwrap();
        assert!(allocated.is_empty());
        let mtime_after = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(mtime_before, mtime_after, "no-op must not rewrite the file");
    }

    // ── read_foreshadowing_summary ─────────────────────────────────────────

    #[test]
    fn read_foreshadowing_summary_returns_none_for_empty() {
        let (_tmp, work) = work_dir();
        // Missing file.
        assert!(read_foreshadowing_summary(&work).is_none());
        // Empty-stub file (scaffolded shape, zero rows).
        fs::write(
            work.join("Outlines/foreshadowing.md"),
            serialize_foreshadowing_index(&[]),
        )
        .unwrap();
        assert!(read_foreshadowing_summary(&work).is_none());
    }

    #[test]
    fn read_foreshadowing_summary_returns_compact_markdown_for_populated() {
        let (_tmp, work) = work_dir();
        promote_outline_to_index(&work, "- F001: the locket\n- F002: the prophecy").unwrap();
        let summary = read_foreshadowing_summary(&work).expect("non-empty");
        assert!(summary.contains("- F001 | the locket | planned"));
        assert!(summary.contains("- F002 | the prophecy | planned"));
        // Compact: one line per row, no trailing blank line.
        assert_eq!(summary.lines().count(), 2);
    }

    // ── ForeshadowingStatus typed vocabulary (W-1) ─────────────────────────

    #[test]
    fn foreshadowing_status_display_is_canonical_lowercase() {
        assert_eq!(ForeshadowingStatus::Planned.to_string(), "planned");
        assert_eq!(ForeshadowingStatus::Buried.to_string(), "buried");
        assert_eq!(ForeshadowingStatus::PaidOff.to_string(), "paid_off");
    }

    #[test]
    fn foreshadowing_status_fromstr_is_case_insensitive() {
        // Case-insensitive tolerance for author typos in hand-edited files.
        assert_eq!(
            "PLANNED".parse::<ForeshadowingStatus>().unwrap(),
            ForeshadowingStatus::Planned
        );
        assert_eq!(
            "Buried".parse::<ForeshadowingStatus>().unwrap(),
            ForeshadowingStatus::Buried
        );
        assert_eq!(
            "PAID_OFF".parse::<ForeshadowingStatus>().unwrap(),
            ForeshadowingStatus::PaidOff
        );
        // Surrounding whitespace is tolerated.
        assert_eq!(
            "  planned  ".parse::<ForeshadowingStatus>().unwrap(),
            ForeshadowingStatus::Planned
        );
    }

    #[test]
    fn parse_foreshadowing_index_accepts_all_known_statuses() {
        let content = "\
            | ID | Description | Planted | Paid off | Status |\n\
            | --- | --- | --- | --- | --- |\n\
            | F001 | a | 1 |  | planned |\n\
            | F002 | b | 2 |  | buried |\n\
            | F003 | c | 3 | 5 | paid_off |\n";
        let rows = parse_foreshadowing_index(content).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].status, ForeshadowingStatus::Planned);
        assert_eq!(rows[1].status, ForeshadowingStatus::Buried);
        assert_eq!(rows[2].status, ForeshadowingStatus::PaidOff);
    }

    #[test]
    fn parse_foreshadowing_index_rejects_unknown_status() {
        // Unknown / typo'd status values must produce a structured error,
        // not silently pass through as a String.
        let content = "\
            | ID | Description | Planted | Paid off | Status |\n\
            | --- | --- | --- | --- | --- |\n\
            | F001 | a | 1 |  | planned |\n\
            | F002 | b | 2 |  | Payed off |\n";
        let err = parse_foreshadowing_index(content).unwrap_err();
        match err {
            IndexParseError::InvalidStatus { row_index, value } => {
                // Data row index 1 (zero-based, counting placeholder rows too).
                assert_eq!(row_index, 1);
                assert_eq!(value, "Payed off");
            }
        }
    }

    #[test]
    fn serialize_then_parse_roundtrip_preserves_known_statuses() {
        let rows = vec![
            ForeshadowingRow {
                id: "F001".to_string(),
                description: "a".to_string(),
                planted: String::new(),
                paid_off: String::new(),
                status: ForeshadowingStatus::Planned,
            },
            ForeshadowingRow {
                id: "F002".to_string(),
                description: "b".to_string(),
                planted: String::new(),
                paid_off: String::new(),
                status: ForeshadowingStatus::Buried,
            },
            ForeshadowingRow {
                id: "F003".to_string(),
                description: "c".to_string(),
                planted: String::new(),
                paid_off: "5".to_string(),
                status: ForeshadowingStatus::PaidOff,
            },
        ];
        let serialized = serialize_foreshadowing_index(&rows);
        let reparsed = parse_foreshadowing_index(&serialized).unwrap();
        assert_eq!(reparsed, rows);
        // Canonical wire strings present in the serialized output.
        assert!(serialized.contains("planned"));
        assert!(serialized.contains("buried"));
        assert!(serialized.contains("paid_off"));
    }
}
