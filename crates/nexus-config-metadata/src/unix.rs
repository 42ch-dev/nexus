//! Unix access policy: a private staging directory, owner, group, mode,
//! extended attributes and the macOS access ACL.
//!
//! Every `unsafe` block below wraps exactly one documented OS call
//! (`fchown`, `f*listxattr`/`f*getxattr`/`f*setxattr`/`f*removexattr`, and the
//! macOS `acl_*` family) and states the buffer/descriptor contract it relies on.

use std::ffi::{CStr, CString};
use std::fs::{File, Permissions};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};
use std::path::Path;
use std::ptr;

/// The policy a publication must carry from the old document to the new one.
pub struct UnixPolicy {
    uid: u32,
    gid: u32,
    mode: u32,
    xattrs: Vec<(CString, Vec<u8>)>,
    #[cfg(target_os = "macos")]
    acl: Option<AclHandle>,
}

/// The "no such attribute" errno of this platform.
#[cfg(target_os = "linux")]
const NO_ATTR: libc::c_int = libc::ENODATA;
/// The "no such attribute" errno of this platform.
#[cfg(target_os = "macos")]
const NO_ATTR: libc::c_int = libc::ENOATTR;

/// Create the staging directory owner-only.
pub fn create_staging_directory(directory: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new().mode(0o700).create(directory)
}

/// Make the staging directory private before its child exists.
///
/// The directory is created before the document file, so removing its ACLs here
/// means the file is created without any inherited grant: no other principal can
/// reach the staged bytes, regardless of the target directory's policy.
///
/// Clearing an inherited ACL does not undo a mode change another principal
/// already completed through the inherited authority: the directory would stay
/// writable and could have its child pathname replaced before the rename. The
/// owner-only mode is therefore established again after the removal and then
/// validated, together with the creator's ownership, before any staged byte can
/// exist. A directory this process no longer owns, or whose mode cannot be
/// narrowed, refuses the publication.
pub fn seal_staging_directory(directory: &Path) -> io::Result<()> {
    let file = File::open(directory)?;
    #[cfg(target_os = "macos")]
    clear_acl(file.as_raw_fd())?;
    #[cfg(target_os = "linux")]
    {
        let descriptor = file.as_raw_fd();
        let mut name = c"system.posix_acl_access";
        remove_xattr(descriptor, name)?;
        name = c"system.posix_acl_default";
        remove_xattr(descriptor, name)?;
    }

    let descriptor = file.as_raw_fd();
    // SAFETY: `descriptor` is owned by `file` for the duration of this call, and
    // `0o700` is the owner-only staging contract this directory must carry.
    if unsafe { libc::fchmod(descriptor, 0o700) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let metadata = file.metadata()?;
    // SAFETY: geteuid takes no arguments and cannot fail.
    let creator = unsafe { libc::geteuid() };
    if metadata.uid() != creator {
        return Err(io::Error::other(format!(
            "refusing the staging directory {}: owned by uid {} instead of the creator uid {creator}",
            directory.display(),
            metadata.uid()
        )));
    }
    let mode = metadata.mode() & 0o7777;
    if mode != 0o700 {
        return Err(io::Error::other(format!(
            "refusing the staging directory {}: mode {mode:04o} is not owner-only",
            directory.display()
        )));
    }
    Ok(())
}

/// The document's hard-link count.
pub fn link_count(target: &Path) -> io::Result<u64> {
    Ok(std::fs::metadata(target)?.nlink())
}

pub fn capture(target: &Path) -> io::Result<UnixPolicy> {
    let file = File::open(target)?;
    let metadata = file.metadata()?;
    let descriptor = file.as_raw_fd();
    Ok(UnixPolicy {
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.mode() & 0o7777,
        xattrs: read_xattrs(descriptor)?,
        #[cfg(target_os = "macos")]
        acl: read_acl(descriptor)?,
    })
}

pub fn apply(policy: &UnixPolicy, file: &File, _path: &Path) -> io::Result<()> {
    let descriptor = file.as_raw_fd();
    let current = file.metadata()?;
    if current.uid() != policy.uid || current.gid() != policy.gid {
        // SAFETY: `descriptor` is owned by `file` for the duration of this call,
        // and the ids are the target document's own metadata values.
        let result = unsafe { libc::fchown(descriptor, policy.uid, policy.gid) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    // Replace the inherited ACL/extended policy *before* restoring group bits:
    // widening the mode first would reopen a named ACE inherited from the
    // target directory for the rest of the window.
    for (name, value) in &policy.xattrs {
        if read_xattr(descriptor, name)?.is_some_and(|existing| existing == *value) {
            continue;
        }
        set_xattr(descriptor, name, value)?;
    }

    #[cfg(target_os = "linux")]
    {
        // A directory default ACL is inherited by the staging file. Drop it when
        // the target had no access ACL, so the replacement cannot grant more
        // than the document it replaces.
        let acl = c"system.posix_acl_access";
        if !policy.xattrs.iter().any(|(name, _)| name.as_c_str() == acl) {
            remove_xattr(descriptor, acl)?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        match &policy.acl {
            // SAFETY: `descriptor` is owned by `file` and the captured handle
            // stays alive for this call; acl_set_fd copies the ACL into the
            // document.
            Some(acl) => {
                let result = unsafe { acl_set_fd(descriptor, acl.0) };
                if result != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            // The target had no extended ACL: clear any ACL the staging file
            // inherited so the replacement cannot grant more.
            None => clear_acl(descriptor)?,
        }
    }

    // The mode is applied last: on Linux this records the captured ACL mask,
    // on macOS it records the captured mode beside the explicit ACL entries.
    file.set_permissions(Permissions::from_mode(policy.mode))
}

pub fn access_policy_blob(target: &Path) -> io::Result<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        let file = File::open(target)?;
        let Some(acl) = read_acl(file.as_raw_fd())? else {
            return Ok(Vec::new());
        };
        let mut length: libc::ssize_t = 0;
        // SAFETY: `acl` is a live handle from `read_acl`; acl_to_text returns an
        // owned NUL-terminated buffer or null.
        let text = unsafe { acl_to_text(acl.0, &raw mut length) };
        if text.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `text` is a valid C string owned by this call until freed.
        let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
        // SAFETY: `text` was returned by the documented acl_to_text call above
        // and is freed exactly once here.
        unsafe { acl_free(text.cast()) };
        Ok(bytes)
    }
    #[cfg(target_os = "linux")]
    {
        let file = File::open(target)?;
        let acl = c"system.posix_acl_access";
        Ok(read_xattr(file.as_raw_fd(), acl)?.unwrap_or_default())
    }
}

/// Read every extended attribute of the open document.
fn read_xattrs(descriptor: RawFd) -> io::Result<Vec<(CString, Vec<u8>)>> {
    let size = list_xattr(descriptor, ptr::null_mut(), 0);
    if size < 0 {
        let error = io::Error::last_os_error();
        if is_unsupported(&error) {
            return Ok(Vec::new());
        }
        return Err(error);
    }
    let size = checked_len(size, "list extended attributes")?;
    if size == 0 {
        return Ok(Vec::new());
    }

    let mut names = vec![0_u8; size];
    let read = list_xattr(descriptor, names.as_mut_ptr(), names.len());
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    let read = checked_len(read, "list extended attributes")?;
    names.truncate(read);

    let mut xattrs = Vec::new();
    for name in names.split(|byte| *byte == 0) {
        if name.is_empty() {
            continue;
        }
        let name = CString::new(name.to_vec())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        // A name that disappeared between the list and the read is simply gone.
        if let Some(value) = read_xattr(descriptor, &name)? {
            xattrs.push((name, value));
        }
    }
    Ok(xattrs)
}

fn read_xattr(descriptor: RawFd, name: &CStr) -> io::Result<Option<Vec<u8>>> {
    let size = get_xattr_raw(descriptor, name.as_ptr(), ptr::null_mut(), 0);
    if size < 0 {
        let error = io::Error::last_os_error();
        let code = error.raw_os_error();
        if code == Some(NO_ATTR) || code == Some(libc::ENOTSUP) {
            return Ok(None);
        }
        return Err(error);
    }
    let size = checked_len(size, "read extended attribute")?;
    if size == 0 {
        return Ok(Some(Vec::new()));
    }

    let mut value = vec![0_u8; size];
    let read = get_xattr_raw(descriptor, name.as_ptr(), value.as_mut_ptr(), value.len());
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    let read = checked_len(read, "read extended attribute")?;
    value.truncate(read);
    Ok(Some(value))
}

fn set_xattr(descriptor: RawFd, name: &CStr, value: &[u8]) -> io::Result<()> {
    let result = set_xattr_raw(descriptor, name.as_ptr(), value.as_ptr(), value.len());
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn remove_xattr(descriptor: RawFd, name: &CStr) -> io::Result<()> {
    let result = remove_xattr_raw(descriptor, name.as_ptr());
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    let code = error.raw_os_error();
    if code == Some(NO_ATTR) || code == Some(libc::ENOTSUP) {
        return Ok(());
    }
    Err(error)
}

/// Convert a non-negative syscall length, refusing a negative result.
fn checked_len(length: libc::ssize_t, what: &str) -> io::Result<usize> {
    usize::try_from(length).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{what}: negative length"),
        )
    })
}

/// Whether the filesystem does not support extended attributes at all.
fn is_unsupported(error: &io::Error) -> bool {
    let code = error.raw_os_error();
    if code == Some(libc::ENOTSUP) {
        return true;
    }
    #[cfg(target_os = "macos")]
    if code == Some(libc::EOPNOTSUPP) {
        return true;
    }
    false
}

#[cfg(target_os = "linux")]
fn list_xattr(descriptor: RawFd, list: *mut u8, size: usize) -> libc::ssize_t {
    // SAFETY: `list` is null with a zero `size`, or names a buffer of `size`
    // writable bytes; `descriptor` is a valid descriptor owned by the caller.
    unsafe { libc::flistxattr(descriptor, list.cast(), size) }
}

#[cfg(target_os = "macos")]
fn list_xattr(descriptor: RawFd, list: *mut u8, size: usize) -> libc::ssize_t {
    // SAFETY: as for the Linux form; macOS adds an unused options argument.
    unsafe { libc::flistxattr(descriptor, list.cast(), size, 0) }
}

#[cfg(target_os = "linux")]
fn get_xattr_raw(
    descriptor: RawFd,
    name: *const libc::c_char,
    value: *mut u8,
    size: usize,
) -> libc::ssize_t {
    // SAFETY: `name` is a NUL-terminated C string; `value` is null with a zero
    // `size`, or names a buffer of `size` writable bytes.
    unsafe { libc::fgetxattr(descriptor, name, value.cast(), size) }
}

#[cfg(target_os = "macos")]
fn get_xattr_raw(
    descriptor: RawFd,
    name: *const libc::c_char,
    value: *mut u8,
    size: usize,
) -> libc::ssize_t {
    // SAFETY: as for the Linux form; macOS adds position and options arguments,
    // both zero here.
    unsafe { libc::fgetxattr(descriptor, name, value.cast(), size, 0, 0) }
}

#[cfg(target_os = "linux")]
fn set_xattr_raw(
    descriptor: RawFd,
    name: *const libc::c_char,
    value: *const u8,
    size: usize,
) -> libc::c_int {
    // SAFETY: `name` is a valid C string and `value` names `size` readable bytes.
    unsafe { libc::fsetxattr(descriptor, name, value.cast(), size, 0) }
}

#[cfg(target_os = "macos")]
fn set_xattr_raw(
    descriptor: RawFd,
    name: *const libc::c_char,
    value: *const u8,
    size: usize,
) -> libc::c_int {
    // SAFETY: as for the Linux form; macOS adds position and options arguments,
    // both zero here.
    unsafe { libc::fsetxattr(descriptor, name, value.cast(), size, 0, 0) }
}

#[cfg(target_os = "linux")]
fn remove_xattr_raw(descriptor: RawFd, name: *const libc::c_char) -> libc::c_int {
    // SAFETY: `name` is a valid NUL-terminated C string.
    unsafe { libc::fremovexattr(descriptor, name) }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn acl_get_fd(descriptor: libc::c_int) -> *mut libc::c_void;
    fn acl_set_fd(descriptor: libc::c_int, acl: *mut libc::c_void) -> libc::c_int;
    fn acl_init(count: libc::c_int) -> *mut libc::c_void;
    fn acl_free(object: *mut libc::c_void) -> libc::c_int;
    fn acl_to_text(acl: *mut libc::c_void, length: *mut libc::ssize_t) -> *mut libc::c_char;
}

/// An owned macOS ACL handle.
#[cfg(target_os = "macos")]
pub struct AclHandle(pub *mut libc::c_void);

#[cfg(target_os = "macos")]
impl Drop for AclHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` came from `acl_get_fd` and is not freed elsewhere.
            unsafe { acl_free(self.0) };
        }
    }
}

/// Capture the document's extended ACL, or `None` when it has none.
///
/// `acl_get_fd` reports "no ACL" as a null handle, so errno is cleared first and
/// then classified: only "no ACL" is a normal absence; any other failure is
/// returned so the caller refuses to publish without the policy.
#[cfg(target_os = "macos")]
fn read_acl(descriptor: RawFd) -> io::Result<Option<AclHandle>> {
    // SAFETY: `__error` returns this thread's errno location.
    unsafe { *libc::__error() = 0 };
    // SAFETY: `descriptor` is valid for this call and owned by the caller;
    // acl_get_fd returns an owned handle or null.
    let acl = unsafe { acl_get_fd(descriptor) };
    if acl.is_null() {
        let error = io::Error::last_os_error();
        if error
            .raw_os_error()
            .is_none_or(|code| code == 0 || code == libc::ENOENT)
        {
            return Ok(None);
        }
        return Err(error);
    }
    Ok(Some(AclHandle(acl)))
}

/// Clear the object's ACL (an empty ACL grants nothing).
#[cfg(target_os = "macos")]
fn clear_acl(descriptor: RawFd) -> io::Result<()> {
    // SAFETY: acl_init returns an owned, empty handle (or null).
    let empty = unsafe { acl_init(0) };
    if empty.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `empty` is a valid handle and `descriptor` is owned by the caller;
    // acl_set_fd copies the ACL into the object.
    let result = unsafe { acl_set_fd(descriptor, empty) };
    // SAFETY: `empty` was allocated by acl_init above and is freed exactly once.
    unsafe { acl_free(empty) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::access_policy_blob;
    use super::seal_staging_directory;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// Take a fresh scratch directory by exclusive creation: a name this call
    /// did not create is never removed.
    fn scratch_directory() -> PathBuf {
        loop {
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nexus-config-metadata-seal-{}-{unique}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("scratch directory: {error}"),
            }
        }
    }

    fn mode(directory: &std::path::Path) -> u32 {
        std::fs::metadata(directory)
            .expect("directory metadata")
            .permissions()
            .mode()
            & 0o7777
    }

    /// SEC-01: a mode another principal widened through inherited authority
    /// before sealing must not survive it, and sealing must refuse a directory
    /// it can no longer make owner-only rather than publishing from it.
    #[test]
    fn a_mode_widened_before_sealing_does_not_survive() {
        let directory = scratch_directory();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o777))
            .expect("the directory is widened as inherited authority allows");
        #[cfg(target_os = "macos")]
        {
            let status = std::process::Command::new("chmod")
                .arg("+a")
                .arg("nobody allow read")
                .arg(&directory)
                .status()
                .expect("run chmod");
            assert!(status.success(), "the inherited-authority ACL is installed");
            assert!(
                !access_policy_blob(&directory).expect("acl").is_empty(),
                "the directory really carries an ACL before sealing"
            );
        }

        seal_staging_directory(&directory).expect("sealing succeeds");

        assert_eq!(mode(&directory), 0o700, "the widened mode does not survive");
        #[cfg(target_os = "macos")]
        assert!(
            access_policy_blob(&directory).expect("acl").is_empty(),
            "the ACL that granted the inherited authority is gone"
        );
        std::fs::remove_dir(&directory).expect("scratch directory removed");
    }
}
