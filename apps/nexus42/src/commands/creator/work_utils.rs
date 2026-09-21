//! Bounded file readers shared by the retained `creator` authoring commands.
//!
//! v1.193 P2-T1: the daemon-transport helpers this module was extracted for
//! (`resolve_active_work_id`, the `query_path` URL builder) went with the
//! removed Creator runner and the `works intake` / `works resume-chain`
//! entrances; every retained Work arm resolves its active Work through
//! [`super::works::active_work_id_core`]. What remains is the bounded file
//! reader the retained authoring leaves share.

use crate::errors::{CliError, Result};

/// Read a file into a string with a client-side size cap (qc3 S-002).
///
/// Rejects an oversized input with a named CLI error *before* the read
/// (instead of unbounded `read_to_string`) so a `--content-file`/`--file`
/// pointed at a gigantic file by accident fails fast, matching the daemon's
/// preset/content caps.
///
/// # Errors
///
/// Returns a named `CliError::Other` when the file cannot be read or its
/// size exceeds `max_bytes`.
pub fn read_file_bounded(path: &str, max_bytes: usize, flag_name: &str) -> Result<String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| CliError::Other(format!("cannot read {flag_name} '{path}': {e}")))?;
    let size = usize::try_from(meta.len())
        .map_err(|_| CliError::Other(format!("{flag_name} '{path}' is too large to read")))?;
    if size > max_bytes {
        return Err(CliError::Other(format!(
            "{flag_name} '{path}' is {size} bytes, exceeding the {max_bytes}-byte limit; \
             trim the file or inline the content with the text flag"
        )));
    }
    std::fs::read_to_string(path)
        .map_err(|e| CliError::Other(format!("cannot read {flag_name} '{path}': {e}")))
}
