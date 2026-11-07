//! Private bridge for the native access-policy operations that policy-preserving
//! `config.toml` publication needs.
//!
//! The whole-document publisher in `nexus-home-layout` stages the new document
//! in a sibling file and renames it over the old one, so the replacement has to
//! carry the old document's access policy. Only this crate performs the native
//! calls that requires: owner/group/mode, extended attributes, the macOS access
//! ACL and the Windows security descriptor.
//!
//! This is the workspace's single, deliberately approved `unsafe` exception.
//! The root workspace table keeps `unsafe_code = "forbid"` and every other crate
//! keeps inheriting it; this crate does not inherit the lint table (see
//! `Cargo.toml`), every `unsafe` block names the documented OS call it wraps in
//! a SAFETY comment, and the public surface is safe. Nothing here decides
//! *whether* to publish: a failure is returned so the caller leaves the old
//! document unchanged.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

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
/// mode, ACL and extended access attributes.
pub struct AccessPolicy {
    #[cfg(unix)]
    policy: unix::UnixPolicy,
    #[cfg(windows)]
    policy: windows::WindowsPolicy,
    #[cfg(not(any(unix, windows)))]
    policy: portable::PortablePolicy,
}

/// Capture the access policy of the document a publication will replace.
///
/// # Errors
///
/// The I/O error that prevented reading the document's metadata or its extended
/// access policy, and [`io::ErrorKind::Unsupported`] on a platform with no
/// preservation contract. The caller refuses the publication when this fails.
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

/// Create the staging file a publication writes before its atomic rename.
///
/// On Unix the file is created owner-only (`0600`), the narrowest mode that
/// still allows writing, so a document byte is never staged under a policy
/// broader than the target's. On Windows the file inherits the directory's DACL,
/// so the captured policy is applied here, before the first byte; the handle
/// stays usable because the DACL governs other principals, not this handle.
///
/// # Errors
///
/// The I/O error that prevented creating the staging file, and on Windows the
/// error that prevented applying the policy before the first byte is written.
pub fn create_staging(path: &Path, policy: Option<&AccessPolicy>) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    if let Some(policy) = policy {
        if let Err(error) = windows::apply(&policy.policy, &file, path) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    let _ = policy;
    Ok(file)
}

/// Apply the captured policy once the document bytes are written.
///
/// A no-op on Windows, where [`create_staging`] already applied it before the
/// first byte.
///
/// # Errors
///
/// The I/O error that prevented preserving the owner, group, mode, ACL or an
/// extended attribute. The caller then removes the staging file and refuses the
/// publication, leaving the old document unchanged.
pub fn apply_after_write(
    policy: Option<&AccessPolicy>,
    file: &File,
    path: &Path,
) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(policy) = policy {
        return unix::apply(&policy.policy, file, path);
    }
    #[cfg(windows)]
    let _ = (policy, file, path);
    #[cfg(not(any(unix, windows)))]
    let _ = (policy, file, path);
    Ok(())
}

/// The document's extended access policy as comparable bytes, for verification
/// (regressions and the publication smoke check); empty when it has none.
///
/// # Errors
///
/// The I/O error that prevented reading the policy.
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
