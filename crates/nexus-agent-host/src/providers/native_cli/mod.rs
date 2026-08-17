//! Native CLI provider adapters.
//!
//! Wave 1 implements the Claude Code CLI native provider. Wave 2 adds
//! the Codex CLI native provider. Future waves may add Gemini CLI, etc.

pub mod claude;
pub mod codex;
pub mod dsh;
pub mod map_claude;
pub mod map_codex;
pub mod map_dsh;

/// Cap a crate error's `Display` string for `OpFailed::error_message` (N-2).
///
/// Both crates' `Deserialization` `Display` embeds the full raw wire line,
/// which can be multi-MB and echo prompt/tool payload text. The diagnostic
/// prefix is kept; the raw tail is cut on a UTF-8 char boundary.
pub(crate) fn truncate_error_message(message: &str) -> String {
    const MAX_LEN: usize = 512;
    const SUFFIX: &str = "... (truncated)";
    if message.len() <= MAX_LEN {
        return message.to_string();
    }
    let mut end = MAX_LEN - SUFFIX.len();
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(MAX_LEN);
    out.push_str(&message[..end]);
    out.push_str(SUFFIX);
    out
}
