//! Desktop bundle commands for nexus42.
//!
//! `nexus42 desktop bundle` is a thin wrapper over the repository's unsigned
//! Electron packaging entry (root `package.json` → `build:desktop`), so the
//! packaging pipeline stays in one place instead of being reimplemented in
//! Rust.

use crate::errors::{CliError, Result};
use clap::Subcommand;
use std::path::PathBuf;
use std::process::Stdio;

/// Desktop shell subcommands.
#[derive(Debug, Subcommand)]
pub enum DesktopCommand {
    /// Build the unsigned Electron desktop bundle.
    ///
    /// Delegates to the repository packaging driver
    /// (`pnpm build:desktop -- --arch <arch>`), which requires a
    /// native-architecture macOS host plus prebuilt web, service and native
    /// inputs, and fails closed when one of them is missing.
    ///
    /// The lane is unsigned: signing, notarization and Apple credentials are
    /// not supported.
    Bundle {
        /// Target architecture to package (`arm64` or `x64`).
        ///
        /// Defaults to this host's architecture. The packaging driver accepts
        /// only a native-architecture runner.
        #[arg(long, value_parser = ["arm64", "x64"])]
        arch: Option<String>,
    },
}

/// Run a desktop shell command.
///
/// # Errors
///
/// Returns a [`CliError::Io`] if the underlying packaging build fails, or a
/// [`CliError::Config`] if the repository root cannot be resolved.
pub async fn run(command: DesktopCommand) -> Result<()> {
    match command {
        DesktopCommand::Bundle { arch } => bundle_desktop(arch).await,
    }
}

/// Build the unsigned Electron desktop bundle through the root packaging
/// entry, forwarding the target architecture.
///
/// # Errors
///
/// Returns a [`CliError::Io`] if the packaging command cannot be spawned or
/// exits with a non-zero status, or a [`CliError::Config`] if the repository
/// root cannot be resolved.
async fn bundle_desktop(arch: Option<String>) -> Result<()> {
    let arch = match arch {
        Some(arch) => arch,
        None => native_arch()?.to_owned(),
    };
    let repo_root = repo_root()?;

    let mut cmd = std::process::Command::new("pnpm");
    cmd.arg("build:desktop")
        .arg("--")
        .arg("--arch")
        .arg(&arch)
        .current_dir(&repo_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = tokio::task::spawn_blocking(move || cmd.status())
        .await
        .map_err(|e| CliError::Io(std::io::Error::other(e)))?
        .map_err(CliError::Io)?;

    if !status.success() {
        return Err(CliError::Io(std::io::Error::other(format!(
            "desktop bundle build failed with status {status}"
        ))));
    }

    println!("Desktop bundle built successfully.");
    Ok(())
}

/// Translate this host's architecture into the packaging driver's vocabulary.
///
/// # Errors
///
/// Returns a [`CliError::Config`] when the host architecture has no packaging
/// equivalent.
fn native_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("x64"),
        other => Err(CliError::Config(format!(
            "unsupported host architecture '{other}'; pass --arch arm64 or --arch x64"
        ))),
    }
}

/// Resolve the repository root from `CARGO_MANIFEST_DIR`.
///
/// `apps/nexus42` lives two levels below the repository root.
fn repo_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Config(
                "could not resolve repository root from CARGO_MANIFEST_DIR".to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::error::ErrorKind;
    use clap::Parser;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// A `pnpm` stand-in that records its working directory and argv into
    /// `<stub dir>/invocation` and exits with `exit_code`.
    ///
    /// Tests therefore spawn a real child process through the production
    /// command path without ever reaching real packaging.
    fn stub_pnpm(exit_code: i32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub directory");
        let script = format!(
            "#!/bin/sh\n{{ pwd -P; printf '%s\\n' \"$@\"; }} > \"$(dirname \"$0\")/invocation\"\nexit {exit_code}\n"
        );
        let path = dir.path().join("pnpm");
        fs::write(&path, script).expect("write stub");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod stub");
        dir
    }

    fn recorded_invocation(stub_dir: &Path) -> Vec<String> {
        fs::read_to_string(stub_dir.join("invocation"))
            .expect("stub recorded its invocation")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Prepends a stub directory to this process's `PATH` until dropped.
    struct StubPath(Option<OsString>);

    impl StubPath {
        fn prepend(dir: &Path) -> Self {
            let previous = std::env::var_os("PATH");
            let mut value = dir.as_os_str().to_os_string();
            if let Some(previous) = &previous {
                value.push(":");
                value.push(previous);
            }
            std::env::set_var("PATH", value);
            Self(previous)
        }
    }

    impl Drop for StubPath {
        fn drop(&mut self) {
            match &self.0 {
                Some(previous) => std::env::set_var("PATH", previous),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    fn host_arch() -> &'static str {
        match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            other => panic!("unsupported test host architecture {other}"),
        }
    }

    #[test]
    fn bundle_accepts_supported_arches() {
        for arch in ["arm64", "x64"] {
            let cli = Cli::try_parse_from(["nexus42", "desktop", "bundle", "--arch", arch])
                .expect("supported arch is accepted");
            match cli.into_command() {
                Some(Commands::Desktop {
                    command: DesktopCommand::Bundle { arch: parsed },
                }) => assert_eq!(parsed.as_deref(), Some(arch)),
                other => panic!("expected desktop bundle, got {other:?}"),
            }
        }
    }

    #[test]
    fn bundle_rejects_unsupported_arch() {
        let error = Cli::try_parse_from(["nexus42", "desktop", "bundle", "--arch", "ppc64"])
            .expect_err("unsupported arch is rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn bundle_rejects_obsolete_signing_request() {
        let error = Cli::try_parse_from([
            "nexus42",
            "desktop",
            "bundle",
            "--sign-identity",
            "Developer ID Application: Acme",
        ])
        .expect_err("the retired signing option is rejected");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn bundle_help_documents_the_unsigned_electron_lane() {
        let error = Cli::try_parse_from(["nexus42", "desktop", "bundle", "--help"])
            .expect_err("--help is reported through clap's DisplayHelp error");
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
        assert_eq!(error.exit_code(), 0);
        let help = error.to_string();
        for expected in ["Electron", "unsigned", "--arch"] {
            assert!(
                help.contains(expected),
                "help is missing {expected}: {help}"
            );
        }
        assert!(
            !help.contains("--sign-identity"),
            "help still advertises the retired signing option: {help}"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn bundle_forwards_arch_to_the_packaging_entry_at_repo_root() {
        let stub = stub_pnpm(0);
        let _path = StubPath::prepend(stub.path());

        run(DesktopCommand::Bundle {
            arch: Some("x64".to_string()),
        })
        .await
        .expect("a successful packaging run succeeds");

        let recorded = recorded_invocation(stub.path());
        assert_eq!(
            recorded[0],
            fs::canonicalize(repo_root().expect("repo root"))
                .expect("canonical repo root")
                .to_string_lossy()
        );
        assert_eq!(
            recorded[1..],
            ["build:desktop", "--", "--arch", "x64"],
            "root packaging entry must receive the forwarded architecture"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn bundle_defaults_to_the_host_architecture() {
        let stub = stub_pnpm(0);
        let _path = StubPath::prepend(stub.path());

        run(DesktopCommand::Bundle { arch: None })
            .await
            .expect("a successful packaging run succeeds");

        assert_eq!(
            recorded_invocation(stub.path())[1..],
            ["build:desktop", "--", "--arch", host_arch()]
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn bundle_propagates_packaging_failure() {
        let stub = stub_pnpm(7);
        let _path = StubPath::prepend(stub.path());

        let error = run(DesktopCommand::Bundle {
            arch: Some("arm64".to_string()),
        })
        .await
        .expect_err("a failing packaging run fails the command");

        match error {
            CliError::Io(io) => {
                let message = io.to_string();
                assert!(
                    message.contains("desktop bundle build failed with status"),
                    "unexpected failure message: {message}"
                );
                assert!(
                    message.contains("exit status: 7"),
                    "failure message lost the child status: {message}"
                );
            }
            other => panic!("expected CliError::Io, got {other:?}"),
        }
        assert_eq!(
            recorded_invocation(stub.path())[1..],
            ["build:desktop", "--", "--arch", "arm64"],
            "the command must reach the packaging entry before failing"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn bundle_ignores_ambient_signing_environment() {
        let stub = stub_pnpm(0);
        let _path = StubPath::prepend(stub.path());
        let previous = std::env::var_os("APPLE_SIGNING_IDENTITY");
        std::env::set_var(
            "APPLE_SIGNING_IDENTITY",
            "Developer ID Application: Acme (TEAMID)",
        );

        let outcome = run(DesktopCommand::Bundle { arch: None }).await;

        match previous {
            Some(previous) => std::env::set_var("APPLE_SIGNING_IDENTITY", previous),
            None => std::env::remove_var("APPLE_SIGNING_IDENTITY"),
        }

        outcome.expect("ambient signing credentials do not change the unsigned lane");
        assert_eq!(
            recorded_invocation(stub.path())[1..],
            ["build:desktop", "--", "--arch", host_arch()],
            "ambient signing credentials must not alter the invocation"
        );
    }
}
