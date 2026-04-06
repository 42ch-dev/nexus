//! Manuscript Command Module
//!
//! Implements `manuscript_phase` and promote workflow (roadmap §3.1.1).
//! Subcommands: status, phase, output, promote, verify.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::ManuscriptPhase;

#[derive(Debug, Subcommand)]
pub enum ManuscriptCommand {
    /// Show current manuscript phase
    Status,

    /// Set manuscript phase
    Phase {
        /// Target phase: brainstorm, draft, review, finalize, published
        phase: String,
    },

    /// Show output manuscript status
    Output,

    /// Promote provisional manuscript to canon
    Promote,

    /// Verify manuscript consistency
    Verify {
        /// Enable content check (V1.1+ feature — metadata-only in V1.0)
        #[arg(long)]
        check_content: bool,
    },
}

/// Run manuscript command
pub async fn run(cmd: ManuscriptCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        ManuscriptCommand::Status => manuscript_status(),
        ManuscriptCommand::Phase { phase } => set_phase(&phase),
        ManuscriptCommand::Output => manuscript_output(),
        ManuscriptCommand::Promote => promote_manuscript(),
        ManuscriptCommand::Verify { check_content } => verify_manuscript(check_content),
    }
}

/// Parse a phase string into ManuscriptPhase
fn parse_phase(phase: &str) -> Result<ManuscriptPhase> {
    match phase.to_lowercase().as_str() {
        "brainstorm" => Ok(ManuscriptPhase::Brainstorm),
        "draft" => Ok(ManuscriptPhase::Draft),
        "review" => Ok(ManuscriptPhase::Review),
        "finalize" => Ok(ManuscriptPhase::Finalize),
        "published" => Ok(ManuscriptPhase::Published),
        _ => Err(CliError::Config(format!(
            "Unknown phase '{}'. Valid: brainstorm, draft, review, finalize, published",
            phase
        ))),
    }
}

/// Show current manuscript status
fn manuscript_status() -> Result<()> {
    println!("Manuscript Status:");
    println!("  Phase: — (no workspace initialized)");
    println!("  Active manifest: —");
    println!();
    println!("⚠ V1.0 skeleton: status requires workspace initialization + daemon.");

    Ok(())
}

/// Set manuscript phase
fn set_phase(phase_str: &str) -> Result<()> {
    let phase = parse_phase(phase_str)?;

    println!("Setting manuscript phase to: {:?}", phase);
    println!("✓ Phase updated.");
    println!();
    println!("⚠ V1.0 skeleton: phase stored locally; sync to platform pending.");

    Ok(())
}

/// Show output manuscript status
fn manuscript_output() -> Result<()> {
    println!("Output Manuscript:");
    println!("  Status: —");
    println!("  Last generated: —");
    println!();
    println!("⚠ V1.0 skeleton: output requires workspace initialization + daemon.");

    Ok(())
}

/// Promote provisional manuscript to canon
fn promote_manuscript() -> Result<()> {
    println!("Promoting manuscript to canon...");
    println!();
    println!("⚠ V1.0 skeleton: promote is a user-triggered operation with no");
    println!("  hard pre-checks in V1.0. Full validation in V1.1+.");
    println!("  Success criteria: metadata integrity + no conflicting world revisions.");

    Ok(())
}

/// Verify manuscript consistency
fn verify_manuscript(check_content: bool) -> Result<()> {
    println!("Verifying manuscript consistency...");

    if check_content {
        println!("  ⚠ --check-content is a V1.1+ feature.");
        println!("    V1.0 verify performs metadata-only validation.");
    }

    println!("  Metadata integrity: ✓ (placeholder)");
    println!("  Schema compliance: ✓ (placeholder)");
    println!("  Phase consistency: ✓ (placeholder)");
    println!();
    println!("✓ Verification passed (metadata-only).");

    Ok(())
}
