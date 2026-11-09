//! Private bridge for the native access-policy operations that policy-preserving
//! `config.toml` publication needs.
//!
//! The whole-document publisher in `nexus-home-layout` stages the new document
//! in a private sibling directory and renames it over the old one, so the
//! replacement has to carry the old document's access policy. Only this crate
//! performs the native calls that requires: a restrictive native staging
//! creation, the captured owner/group/mode, extended attributes, the macOS
//! access ACL, the Windows security descriptor/link count, and the final policy
//! transition.
//!
//! This is the workspace's single, deliberately approved `unsafe` exception.
//! Every existing crate keeps its own lint table; the root workspace table keeps
//! `unsafe_code = "forbid"` and this crate does not inherit it (see
//! `Cargo.toml`). Every `unsafe` block either wraps exactly one documented OS
//! call with a SAFETY comment naming its descriptor/buffer contract, or is one
//! of the three bounded non-call reads in the credential DACL walk — the two
//! typed `ACE_HEADER`/`ACCESS_ALLOWED_ACE` field-read/classification blocks
//! (decision D44) and the single one-byte SID count metadata read (decision
//! D45) — each carrying a SAFETY
//! comment citing the record's documented layout, extent, alignment and
//! lifetime. Nothing here hand-parses security bytes or reinterprets unknown
//! layouts: unknown or unsupported policy still refuses, and the public surface
//! is safe. Nothing here decides *whether*
//! to publish: a failure is returned so the caller leaves the old document
//! unchanged. The same rule covers the Windows credential privacy gate the
//! memory credential loader consumes: a failure is returned so the loader
//! refuses the credential file.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(not(any(unix, windows)))]
mod portable;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// The access policy of the document an atomic publication replaces.
///
/// Captured from the real target before staging and applied to the staging file
/// before the rename, so the replacement keeps the old document's owner, group,
/// mode, ACL and extended access attributes. A policy that cannot be captured or
/// applied is an error, never a silent widening.
pub struct AccessPolicy {
    #[cfg(unix)]
    policy: unix::UnixPolicy,
    #[cfg(windows)]
    policy: windows::WindowsPolicy,
    #[cfg(not(any(unix, windows)))]
    policy: portable::PortablePolicy,
}

/// The staging location a publication writes before its atomic rename.
///
/// The directory is created first, the document file only afterwards, and the
/// platform's own protection is what keeps the staged bytes unreadable to other
/// principals while the publication runs (I-000331 contract):
///
/// * Unix: the directory is created owner-only and sealed before the file
///   exists, so no inherited ACL can reach a file that does not exist yet, and
///   the staged file is created `0600` inside it.
/// * Windows: the directory inherits the parent DACL — this crate applies no
///   private DACL of its own — and the staged file is opened with share mode
///   `0`, so no other principal can open it between creation and the policy
///   transition. That no-sharing handle is the protection, not the directory.
/// * Other targets: creation applies no policy transition; the staged file uses
///   the platform `OpenOptions` default policy.
///
/// The handle covers the write; after [`Staging::close`] the caller renames the
/// staged path, and that rename is authorized and protected by the target
/// directory alone. On Windows this is the inherited parent DACL for that
/// window — the documented contract this type carries; no private-DACL
/// implementation is claimed.
pub struct Staging {
    directory: PathBuf,
    path: PathBuf,
    file: File,
}

impl Staging {
    /// The staging directory that must be removed after publication.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The staged document path that is renamed over the target.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The open handle the publisher writes through.
    #[must_use]
    pub const fn file(&self) -> &File {
        &self.file
    }

    /// Close the staging handle before the rename publishes the bytes.
    ///
    /// Returns the staged path and the staging directory so the caller can
    /// rename and then remove them.
    #[must_use]
    pub fn close(self) -> (PathBuf, PathBuf) {
        drop(self.file);
        (self.path, self.directory)
    }
}

/// Capture the access policy of the document a publication will replace.
///
/// # Errors
///
/// The I/O error that prevented reading the document's metadata or its extended
/// access policy, [`io::ErrorKind::Unsupported`] when the platform or the
/// document carries a policy class this writer cannot reproduce, and the other
/// native error the capture reported. The caller refuses the publication when
/// this fails.
pub fn capture(target: &Path) -> io::Result<AccessPolicy> {
    #[cfg(unix)]
    {
        Ok(AccessPolicy {
            policy: unix::capture(target)?,
        })
    }
    #[cfg(windows)]
    {
        Ok(AccessPolicy {
            policy: windows::capture(target)?,
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(AccessPolicy {
            policy: portable::capture(target)?,
        })
    }
}

/// Create the staging directory and its document file.
///
/// The directory must not exist yet; the caller picks its name. On Unix the
/// directory is created owner-only and sealed first, then the file is created
/// `0600` inside it, so no staged byte is exposed under the target directory's
/// inherited policy. On Windows the directory inherits the parent DACL and the
/// file is opened with no sharing; [`Staging`] carries the exact per-platform
/// contract. A failure after the directory exists removes it again and surfaces
/// both the primary error and any cleanup failure with the exact leftover path.
///
/// # Errors
///
/// The I/O error that prevented creating or sealing the directory, or creating
/// the file, combined with a failed cleanup when the leftover could not be
/// removed.
pub fn create_staging(directory: &Path) -> io::Result<Staging> {
    #[cfg(unix)]
    unix::create_staging_directory(directory)?;
    #[cfg(not(unix))]
    std::fs::create_dir(directory)?;

    #[cfg(unix)]
    let sealed = unix::seal_staging_directory(directory);
    #[cfg(not(unix))]
    let sealed = Ok(());

    if let Err(error) = sealed {
        return Err(cleanup_after_creation_error(directory, error));
    }

    open_staged_file(directory).map_err(|error| cleanup_after_creation_error(directory, error))
}

fn open_staged_file(directory: &Path) -> io::Result<Staging> {
    let path = directory.join("config.toml");

    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?
    };
    #[cfg(windows)]
    let file = windows::open_staging_file(&path)?;
    #[cfg(not(any(unix, windows)))]
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;

    Ok(Staging {
        directory: directory.to_path_buf(),
        path,
        file,
    })
}

/// Apply the captured policy once the document bytes are written.
///
/// The call happens while the staged handle is still open, so a Windows policy
/// transition cannot race another principal through a reopened name, and it
/// happens before the mode is restored, so a Linux inherited POSIX ACL mask is
/// replaced rather than reopened. On success the staged document carries the
/// target's policy; on failure the caller removes it and refuses the
/// publication.
///
/// # Errors
///
/// The native error that prevented preserving the owner, group, mode, ACL or an
/// extended attribute.
pub fn apply_after_write(
    policy: Option<&AccessPolicy>,
    file: &File,
    path: &Path,
) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(policy) = policy {
        return unix::apply(&policy.policy, file, path);
    }
    // The Windows transition works from the open handle alone; the portable
    // fallback refuses, so neither needs the staged path.
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(windows)]
    if let Some(policy) = policy {
        return windows::apply(&policy.policy, file);
    }
    #[cfg(not(any(unix, windows)))]
    if let Some(policy) = policy {
        return portable::apply(&policy.policy, file, path);
    }
    Ok(())
}

/// The document's extended access policy as comparable bytes, for verification
/// (regressions and the publication smoke check); empty when it has none.
///
/// # Errors
///
/// The I/O error that prevented reading the policy, and
/// [`io::ErrorKind::Unsupported`] on a platform with no such view.
pub fn access_policy_blob(target: &Path) -> io::Result<Vec<u8>> {
    #[cfg(unix)]
    {
        unix::access_policy_blob(target)
    }
    #[cfg(windows)]
    {
        windows::access_policy_blob(target)
    }
    #[cfg(not(any(unix, windows)))]
    {
        portable::access_policy_blob(target)
    }
}

/// The document's hard-link count, so a publication can refuse to detach an
/// alias instead of replacing only one name.
///
/// # Errors
///
/// The native error that prevented reading the link count; an indeterminate
/// count is never treated as one link.
pub fn link_count(target: &Path) -> io::Result<u64> {
    #[cfg(unix)]
    {
        unix::link_count(target)
    }
    #[cfg(windows)]
    {
        windows::link_count(target)
    }
    #[cfg(not(any(unix, windows)))]
    {
        portable::link_count(target)
    }
}

/// The Windows credential privacy gate the memory credential loader consumes.
///
/// It reports whether the caller's already-open file handle grants access only
/// to trusted principals: the file's owner, LOCAL SYSTEM and the Administrators
/// group. Every allow grant the DACL carries — read or nonread — must belong to
/// one of the three; nonread rights such as `WRITE_DAC` are refused exactly the
/// same, since a principal that can rewrite the DACL can grant itself read
/// later. `Ok(())` accepts the bytes behind the handle; any error refuses the
/// credential file instead. Like every function here, a failure is returned,
/// never repaired: the caller decides whether to keep the file.
///
/// # Errors
///
/// The native error the handle query reported,
/// [`io::ErrorKind::PermissionDenied`] when an allow grant belongs to a
/// principal beyond the three trusted ones, when the owner/DACL is missing or
/// unverifiable, or when the owner or a trustee SID fails the native
/// `IsValidSid` validation, and [`io::ErrorKind::Unsupported`] when the DACL
/// carries an access-control entry class this checker cannot interpret or an
/// allowed-access record is truncated below the complete minimal `SID`
/// structure.
#[cfg(windows)]
pub fn grants_only_to_trusted_principals(file: &File) -> io::Result<()> {
    windows::grants_only_to_trusted_principals(file)
}

/// Report a creation failure whose staging directory could not be removed.
///
/// The primary error keeps its kind and message; the cleanup error and the
/// exact leftover path are appended so the caller sees both. A failed cleanup is
/// never discarded: the directory may still hold a partially created file.
fn cleanup_after_creation_error(directory: &Path, primary: io::Error) -> io::Error {
    match std::fs::remove_dir(directory) {
        Ok(()) => primary,
        Err(cleanup) if cleanup.kind() == io::ErrorKind::NotFound => primary,
        Err(cleanup) => io::Error::new(
            primary.kind(),
            format!(
                "{primary}; the staging directory {} could not be removed: {cleanup}",
                directory.display()
            ),
        ),
    }
}
