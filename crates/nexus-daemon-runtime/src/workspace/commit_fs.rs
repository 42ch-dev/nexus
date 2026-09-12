//! Recoverable workspace commit filesystem primitives (v1.188 P3 L2).
//!
//! # Authority model
//!
//! Every workspace-relative path is walked component by component with
//! `openat(..., O_DIRECTORY|O_NOFOLLOW)` from the canonical root, so no path
//! component can be swapped for a symlink between resolution and use. Nothing
//! is resolved by joining an untrusted path onto a trusted prefix.
//!
//! # Compare-and-swap at the mutation boundary
//!
//! A preimage hash read followed by a separate `rename`/`unlink` cannot detect
//! an external writer landing in the gap. Instead every mutation CAPTURES the
//! displaced bytes atomically and verifies them:
//!
//! * `replace` — `renameat(target -> displaced)` atomically moves whatever
//!   occupies the target name aside, the displaced bytes are hashed, and the
//!   staged bytes are installed with `linkat` (which fails `EEXIST` rather
//!   than clobbering). A mismatch restores the captured file and reports a
//!   third-state conflict; a capture by an external writer during install
//!   leaves both artifacts in place as evidence.
//! * `delete` — the same capture/verify/restore cycle, never hash-then-unlink.
//! * `create` — `linkat` no-clobber install only.
//!
//! Non-Unix platforms REFUSE every mutation with `ErrorKind::Unsupported`:
//! there is no joined-path mutating fallback anywhere in this module.

use std::io;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Maximum decoded bytes per file in a commit manifest.
pub const MAX_FILE_BYTES: usize = 1_048_576;
/// Maximum total decoded bytes across all changes.
pub const MAX_TOTAL_BYTES: usize = 8_388_608;
/// Maximum number of changes in one commit.
pub const MAX_CHANGES: usize = 128;

/// Owner-read/write for newly created workspace files.
pub const CREATE_FILE_MODE: u32 = 0o600;

/// Whether `name` is a usable single-segment basename.
///
/// Rejects empty names, any path separator, the `.`/`..` pseudo-segments, and
/// embedded NULs — every shape that could make a directory-relative operation
/// leave its parent.
fn is_safe_basename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

/// Fire the gated after-delete-capture seam (no-op in production).
fn run_after_delete_capture_hook() {
    #[cfg(any(test, feature = "test-hooks"))]
    super::test_hooks::run_after_delete_capture_hook();
}

/// Name holding the bytes displaced by a mutation, derived from the stage name.
fn displaced_basename(stage_basename: &str) -> String {
    format!("{stage_basename}-displaced")
}

/// Name used to stage a rollback restore, derived from the stage name.
///
/// Derived (never the commit stage name) so a still-present commit stage is
/// never clobbered while evidence of the failed apply is preserved.
fn restore_basename(stage_basename: &str) -> String {
    format!("{stage_basename}-rb")
}

/// Scope-bound mutation handle over a verified directory descriptor.
pub struct ScopeMutation {
    scope_path: PathBuf,
    #[cfg(unix)]
    scope: unix_dir::DirFd,
}

#[cfg(not(unix))]
mod unsupported {
    use std::io;

    /// Refuse a workspace mutation on a platform without descriptor-relative
    /// atomic primitives. Failing closed is the only safe option: there is no
    /// path-joining fallback that could preserve the CAS contract.
    pub fn refuse<T>() -> io::Result<T> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "workspace commit requires a Unix platform for atomic \
             descriptor-relative mutation",
        ))
    }
}

#[cfg(not(unix))]
impl ScopeMutation {
    /// Always refuses on non-Unix platforms.
    pub fn open(_canonical_root: &Path, _scope_relative: &str) -> io::Result<Self> {
        unsupported::refuse()
    }

    /// The resolved scope directory (diagnostics only).
    #[must_use]
    pub fn scope_path(&self) -> &Path {
        &self.scope_path
    }

    pub fn target_exists(&self, _rel_path: &str) -> io::Result<bool> {
        unsupported::refuse()
    }

    pub fn hash_target(&self, _rel_path: &str) -> io::Result<String> {
        unsupported::refuse()
    }

    pub fn write_stage(
        &self,
        _rel_path: &str,
        _stage_basename: &str,
        _bytes: &[u8],
        _mode: Option<u32>,
    ) -> io::Result<()> {
        unsupported::refuse()
    }

    pub fn backup_target(
        &self,
        _rel_path: &str,
        _backup_basename: &str,
    ) -> io::Result<Option<u32>> {
        unsupported::refuse()
    }

    pub fn atomic_create(&self, _rel_path: &str, _stage_basename: &str) -> io::Result<()> {
        unsupported::refuse()
    }

    pub fn atomic_replace_verified(
        &self,
        _rel_path: &str,
        _stage_basename: &str,
        _expected_hash: &str,
    ) -> io::Result<()> {
        unsupported::refuse()
    }

    pub fn atomic_delete_verified(
        &self,
        _rel_path: &str,
        _expected_hash: &str,
        _naming_basename: &str,
    ) -> io::Result<()> {
        unsupported::refuse()
    }

    pub fn restore_preimage(
        &self,
        _rel_path: &str,
        _backup_basename: Option<&str>,
        _stage_basename: &str,
        _pre_hash: Option<&str>,
        _post_hash: Option<&str>,
        _mode: Option<u32>,
    ) -> io::Result<()> {
        unsupported::refuse()
    }

    pub fn cleanup_basename(&self, _rel_path: &str, _basename: &str) {}

    pub fn cleanup_entry(
        &self,
        _rel_path: &str,
        _stage_basename: &str,
        _backup_basename: Option<&str>,
    ) {
    }
}

#[cfg(unix)]
impl ScopeMutation {
    /// Open the workspace scope.
    ///
    /// `canonical_root` is opened with `O_DIRECTORY|O_NOFOLLOW`; each
    /// component of `scope_relative` is then walked with
    /// `openat(..., O_DIRECTORY|O_NOFOLLOW)`. A symlinked (or swapped)
    /// component anywhere on the way to the scope fails closed.
    pub fn open(canonical_root: &Path, scope_relative: &str) -> io::Result<Self> {
        let scope_path = if scope_relative.is_empty() {
            canonical_root.to_path_buf()
        } else {
            canonical_root.join(scope_relative)
        };
        let root = unix_dir::DirFd::open(canonical_root)?;
        let components = if scope_relative.is_empty() {
            Vec::new()
        } else {
            split_relative(scope_relative)?
        };
        let scope = unix_dir::walk_scope(&root, &components)?;
        Ok(Self { scope_path, scope })
    }

    /// The resolved scope directory (diagnostics only).
    #[must_use]
    pub fn scope_path(&self) -> &Path {
        &self.scope_path
    }

    /// Whether a relative target exists (no symlink follow).
    pub fn target_exists(&self, rel_path: &str) -> io::Result<bool> {
        self.with_parent(rel_path, |parent, name| parent.exists(name))
    }

    /// Hash a relative target through a no-follow open.
    pub fn hash_target(&self, rel_path: &str) -> io::Result<String> {
        self.with_parent(rel_path, |parent, name| parent.hash_file(name))
    }

    /// Write an exclusive stage file beside the target (create-new, no-clobber).
    pub fn write_stage(
        &self,
        rel_path: &str,
        stage_basename: &str,
        bytes: &[u8],
        mode: Option<u32>,
    ) -> io::Result<()> {
        self.with_parent(rel_path, |parent, _| {
            parent.write_stage_file(stage_basename, bytes, mode)
        })
    }

    /// Backup an existing target into a sibling basename; returns captured mode.
    pub fn backup_target(
        &self,
        rel_path: &str,
        backup_basename: &str,
    ) -> io::Result<Option<u32>> {
        self.with_parent(rel_path, |parent, name| {
            parent.backup_file(name, backup_basename)
        })
    }

    /// Atomic no-clobber create, coupled to the parent descriptor.
    pub fn atomic_create(&self, rel_path: &str, stage_basename: &str) -> io::Result<()> {
        self.with_parent(rel_path, |parent, name| parent.atomic_create(name, stage_basename))
    }

    /// Replace via atomic capture + verify + no-clobber install.
    pub fn atomic_replace_verified(
        &self,
        rel_path: &str,
        stage_basename: &str,
        expected_hash: &str,
    ) -> io::Result<()> {
        self.with_parent(rel_path, |parent, name| {
            parent.atomic_replace_verified(name, stage_basename, expected_hash)
        })
    }

    /// Delete via atomic capture + verify (never hash-then-unlink).
    ///
    /// `naming_basename` only names the private capture artifact; it is the
    /// entry's stage basename, which the delete op never creates.
    pub fn atomic_delete_verified(
        &self,
        rel_path: &str,
        expected_hash: &str,
        naming_basename: &str,
    ) -> io::Result<()> {
        self.with_parent(rel_path, |parent, name| {
            parent.atomic_delete_verified(name, expected_hash, naming_basename)
        })
    }

    /// Restore the preimage during rollback.
    pub fn restore_preimage(
        &self,
        rel_path: &str,
        backup_basename: Option<&str>,
        stage_basename: &str,
        pre_hash: Option<&str>,
        post_hash: Option<&str>,
        mode: Option<u32>,
    ) -> io::Result<()> {
        self.with_parent(rel_path, |parent, name| {
            parent.restore_preimage(
                name,
                backup_basename,
                stage_basename,
                pre_hash,
                post_hash,
                mode,
            )
        })
    }

    /// Remove a sibling basename (stage/backup) relative to a target path.
    ///
    /// Only a plain single-segment basename is accepted: a separator or a `..`
    /// would let the directory-relative unlink escape the parent, so such a
    /// name is refused outright (defence in depth behind the metadata
    /// validation, which should already have rejected it).
    pub fn cleanup_basename(&self, rel_path: &str, basename: &str) {
        if !is_safe_basename(basename) {
            return;
        }
        let _ = self.with_parent(rel_path, |parent, _| parent.unlink(basename));
    }

    /// Remove every artifact derived from one entry (stage, backup, and the
    /// capture/restore siblings).
    ///
    /// Called only once an intent has SETTLED: while an intent is unsettled
    /// these files are the recovery evidence and must survive.
    pub fn cleanup_entry(
        &self,
        rel_path: &str,
        stage_basename: &str,
        backup_basename: Option<&str>,
    ) {
        if let Some(backup) = backup_basename {
            self.cleanup_basename(rel_path, backup);
        }
        self.cleanup_basename(rel_path, &displaced_basename(stage_basename));
        self.cleanup_basename(rel_path, &restore_basename(stage_basename));
        self.cleanup_basename(rel_path, stage_basename);
    }

    fn with_parent<T, F>(&self, rel_path: &str, f: F) -> io::Result<T>
    where
        F: FnOnce(&unix_dir::DirFd, &str) -> io::Result<T>,
    {
        let components = split_relative(rel_path)?;
        let (parent, name) = unix_dir::walk_child(&self.scope, &components)?;
        f(&parent, &name)
    }
}

/// Split a validated relative path into its components.
///
/// Rejects absolute paths, `..`, `.`, path prefixes, and empty segments.
pub(crate) fn split_relative(rel_path: &str) -> io::Result<Vec<String>> {
    let path = Path::new(rel_path);
    let mut out = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Normal(s) => {
                let part = s.to_str().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid path component")
                })?;
                if part.is_empty() || part == "." || part == ".." {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid path component",
                    ));
                }
                out.push(part.to_string());
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path must be relative",
                ));
            }
            Component::CurDir => {}
        }
    }
    if out.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
    }
    Ok(out)
}

#[cfg(unix)]
mod unix_dir {
    use super::{displaced_basename, restore_basename, CREATE_FILE_MODE};
    use nix::fcntl::{openat, renameat, AtFlags, OFlag};
    use nix::sys::stat::Mode;
    use nix::unistd::{linkat, unlinkat};
    use sha2::Digest;
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    /// An owned directory descriptor.
    pub struct DirFd {
        file: File,
    }

    impl DirFd {
        /// Open a directory with `O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC`.
        pub fn open(path: &Path) -> io::Result<Self> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)?;
            Ok(Self { file })
        }

        fn raw_fd(&self) -> i32 {
            self.file.as_raw_fd()
        }

        fn try_clone(&self) -> io::Result<Self> {
            Ok(Self {
                file: self.file.try_clone()?,
            })
        }

        pub fn sync(&self) -> io::Result<()> {
            self.file.sync_all()
        }

        pub fn exists(&self, name: &str) -> io::Result<bool> {
            match self.openat(name, OFlag::O_RDONLY) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e),
            }
        }

        pub fn hash_file(&self, name: &str) -> io::Result<String> {
            let mut file = self.openat(name, OFlag::O_RDONLY)?;
            let mut sha = sha2::Sha256::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                sha.update(&buf[..n]);
            }
            Ok(hex::encode(sha.finalize()))
        }

        /// Create a new exclusive stage file (`O_CREAT|O_EXCL`, no-clobber).
        pub fn write_stage_file(
            &self,
            name: &str,
            bytes: &[u8],
            mode: Option<u32>,
        ) -> io::Result<()> {
            if self.exists(name)? {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "stage exists"));
            }
            let mode = Mode::from_bits_truncate(mode.unwrap_or(CREATE_FILE_MODE) as u16);
            let fd = openat(
                Some(self.raw_fd()),
                name,
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_NOFOLLOW
                    | OFlag::O_CLOEXEC,
                mode,
            )
            .map_err(errno_io)?;
            let mut file = adopt_fd(fd, true)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            self.sync()?;
            Ok(())
        }

        /// Copy an existing target to a sibling backup basename.
        pub fn backup_file(&self, name: &str, backup_basename: &str) -> io::Result<Option<u32>> {
            let mut src = self.openat(name, OFlag::O_RDONLY)?;
            let mode = file_mode_fd(&src);
            let mut bytes = Vec::new();
            src.read_to_end(&mut bytes)?;
            self.write_stage_file(backup_basename, &bytes, mode)?;
            Ok(mode)
        }

        /// Atomic no-clobber create: `linkat` fails with `EEXIST` when the
        /// target name is taken, so there is no check-then-rename window.
        pub fn atomic_create(&self, target: &str, stage_basename: &str) -> io::Result<()> {
            let staged = self.hash_file(stage_basename)?;
            self.link_no_clobber(stage_basename, target, "create")?;
            self.unlink(stage_basename)?;
            let installed = self.hash_file(target)?;
            if installed != staged {
                return Err(third_state("create", "installed bytes differ from staged"));
            }
            self.sync()
        }

        /// Replace through an atomic capture + verify + no-clobber install.
        ///
        /// The capture (`renameat`) and the install (`linkat`) both run
        /// against THIS directory descriptor, and the displaced bytes are
        /// verified, so an external writer landing between the preimage read
        /// and the mutation is detected instead of silently overwritten.
        pub fn atomic_replace_verified(
            &self,
            target: &str,
            stage_basename: &str,
            expected_hash: &str,
        ) -> io::Result<()> {
            let staged = self.hash_file(stage_basename)?;
            let displaced = displaced_basename(stage_basename);

            // 1. Atomically capture whatever currently occupies the target.
            self.rename_within(target, &displaced)
                .map_err(|err| match err {
                    CaptureError::Absent => {
                        third_state("replace", "target is absent at the mutation boundary")
                    }
                    CaptureError::Io(e) => e,
                })?;

            // 2. The captured bytes MUST be the preimage we validated.
            let captured = self.hash_file(&displaced)?;
            if captured != expected_hash {
                self.restore_captured(&displaced, target)?;
                return Err(third_state(
                    "replace",
                    "displaced bytes are not the expected preimage",
                ));
            }

            // 3. Install the staged bytes without clobbering.
            self.link_no_clobber(stage_basename, target, "replace-displaced")?;
            self.unlink(stage_basename)?;
            let installed = self.hash_file(target)?;
            if installed != staged {
                return Err(third_state("replace", "installed bytes differ from staged"));
            }
            self.unlink(&displaced)?;
            self.sync()
        }

        /// Delete through an atomic capture + verify.
        ///
        /// Equivalent real CAS: the original is renamed into a private capture
        /// name and its bytes are verified there; a mismatch restores it. A
        /// writer landing after the capture is untouched.
        pub fn atomic_delete_verified(
            &self,
            target: &str,
            expected_hash: &str,
            naming_basename: &str,
        ) -> io::Result<()> {
            let displaced = displaced_basename(naming_basename);
            self.rename_within(target, &displaced)
                .map_err(|err| match err {
                    CaptureError::Absent => {
                        third_state("delete", "target is absent at the mutation boundary")
                    }
                    CaptureError::Io(e) => e,
                })?;

            let captured = self.hash_file(&displaced)?;
            if captured != expected_hash {
                self.restore_captured(&displaced, target)?;
                return Err(third_state(
                    "delete",
                    "displaced bytes are not the expected preimage",
                ));
            }

            // Deterministic injection point for the capture/finalize window.
            super::run_after_delete_capture_hook();

            // A delete is only valid while the target name is STILL absent.
            // A writer that re-created it during the window means the workspace
            // does not match the committed post-state: keep every artifact
            // (the capture, the backup, and the writer's bytes) and report a
            // conflict rather than finalizing a delete that left unrelated
            // bytes behind.
            if self.exists(target)? {
                self.sync()?;
                return Err(third_state(
                    "delete",
                    "target name recreated by an external writer after capture; \
                     evidence preserved",
                ));
            }

            self.unlink(&displaced)?;
            self.sync()
        }

        /// Restore the preimage (or remove a created target) during rollback.
        pub fn restore_preimage(
            &self,
            target: &str,
            backup_basename: Option<&str>,
            stage_basename: &str,
            pre_hash: Option<&str>,
            post_hash: Option<&str>,
            mode: Option<u32>,
        ) -> io::Result<()> {
            if self.exists(target)? {
                let current = self.hash_file(target)?;
                match (post_hash, pre_hash) {
                    (Some(post), _) if current != post => {
                        return Err(third_state("rollback", "target is not the applied postimage"));
                    }
                    (None, Some(pre)) if current != pre => {
                        return Err(third_state("rollback", "target is not the preimage"));
                    }
                    _ => {}
                }
            }
            match backup_basename {
                Some(backup) if self.exists(backup)? => {
                    let mut file = self.openat(backup, OFlag::O_RDONLY)?;
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)?;
                    let restore = restore_basename(stage_basename);
                    self.write_stage_file(&restore, &bytes, mode)?;
                    if self.exists(target)? {
                        let expected = post_hash.unwrap_or(pre_hash.unwrap_or(""));
                        self.atomic_replace_verified(target, &restore, expected)?;
                    } else {
                        self.atomic_create(target, &restore)?;
                    }
                    Ok(())
                }
                _ => {
                    if self.exists(target)? {
                        let expected = post_hash.unwrap_or(pre_hash.unwrap_or(""));
                        self.atomic_delete_verified(target, expected, stage_basename)?;
                    }
                    Ok(())
                }
            }
        }

        /// `unlinkat` on this descriptor (never follows the final component).
        pub fn unlink(&self, name: &str) -> io::Result<()> {
            unlinkat(
                Some(self.raw_fd()),
                name,
                nix::unistd::UnlinkatFlags::NoRemoveDir,
            )
            .map_err(errno_io)
        }

        /// Atomically move `from` onto `to` within this directory.
        fn rename_within(&self, from: &str, to: &str) -> Result<(), CaptureError> {
            match renameat(Some(self.raw_fd()), from, Some(self.raw_fd()), to) {
                Ok(()) => Ok(()),
                Err(nix::errno::Errno::ENOENT) => Err(CaptureError::Absent),
                Err(other) => Err(CaptureError::Io(errno_io(other))),
            }
        }

        /// `linkat` install that fails `EEXIST` instead of clobbering; a taken
        /// name means an external writer won the race, and BOTH the captured
        /// original and the stage are left in place as evidence.
        fn link_no_clobber(&self, from: &str, to: &str, op: &str) -> io::Result<()> {
            match linkat(
                Some(self.raw_fd()),
                from,
                Some(self.raw_fd()),
                to,
                AtFlags::empty(),
            ) {
                Ok(()) => Ok(()),
                Err(nix::errno::Errno::EEXIST) => {
                    self.sync()?;
                    Err(third_state(
                        op,
                        "target name captured by an external writer during install; \
                         evidence preserved",
                    ))
                }
                Err(other) => Err(errno_io(other)),
            }
        }

        /// Put the captured bytes back at the target name.
        fn restore_captured(&self, displaced: &str, target: &str) -> io::Result<()> {
            match linkat(
                Some(self.raw_fd()),
                displaced,
                Some(self.raw_fd()),
                target,
                AtFlags::empty(),
            ) {
                Ok(()) => {}
                Err(nix::errno::Errno::EEXIST) => {
                    self.sync()?;
                    return Err(third_state(
                        "capture-restore",
                        "target name occupied during capture restore; evidence preserved",
                    ));
                }
                Err(other) => return Err(errno_io(other)),
            }
            self.unlink(displaced)?;
            self.sync()
        }

        fn openat(&self, name: &str, flags: OFlag) -> io::Result<File> {
            let write = flags.contains(OFlag::O_WRONLY);
            let fd = openat(
                Some(self.raw_fd()),
                name,
                flags | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(errno_io)?;
            adopt_fd(fd, write)
        }
    }

    enum CaptureError {
        /// The name to capture did not exist.
        Absent,
        Io(io::Error),
    }

    /// Walk every component of a scope path with `openat(O_DIRECTORY|O_NOFOLLOW)`.
    pub fn walk_scope(root: &DirFd, components: &[String]) -> io::Result<DirFd> {
        let mut current = root.try_clone()?;
        for component in components {
            current = open_subdir(&current, component)?;
        }
        Ok(current)
    }

    /// Walk to the parent of `components`, returning it plus the final name.
    ///
    /// Every intermediate component is opened descriptor-relative with
    /// `O_DIRECTORY|O_NOFOLLOW`; nothing is resolved by joined path.
    pub fn walk_child(scope: &DirFd, components: &[String]) -> io::Result<(DirFd, String)> {
        let Some((name, parents)) = components.split_last() else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
        };
        let mut current = scope.try_clone()?;
        for component in parents {
            current = open_subdir(&current, component)?;
        }
        Ok((current, name.clone()))
    }

    fn open_subdir(parent: &DirFd, name: &str) -> io::Result<DirFd> {
        let fd = openat(
            Some(parent.raw_fd()),
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(errno_io)?;
        Ok(DirFd {
            file: adopt_fd(fd, false)?,
        })
    }

    fn third_state(op: &str, detail: &str) -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("third-state bytes at {op} boundary: {detail}"),
        )
    }

    /// Reopen an owned raw fd without widening its access mode.
    ///
    /// Stage files are created `O_WRONLY`; reopening them read+write returns
    /// `EACCES` on Darwin/Linux and breaks create commits.
    fn adopt_fd(fd: i32, write: bool) -> io::Result<File> {
        let path = format!("/dev/fd/{fd}");
        let file = if write {
            OpenOptions::new().write(true).open(&path)?
        } else {
            OpenOptions::new().read(true).open(&path)?
        };
        nix::unistd::close(fd).map_err(errno_io)?;
        Ok(file)
    }

    fn errno_io(err: nix::errno::Errno) -> io::Error {
        io::Error::from_raw_os_error(err as i32)
    }

    fn file_mode_fd(file: &File) -> Option<u32> {
        use std::os::unix::fs::PermissionsExt;
        file.metadata().ok().map(|m| m.permissions().mode())
    }
}

#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut sha = Sha256::new();
    sha.update(bytes);
    hex::encode(sha.finalize())
}

pub fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("invalid base64: {e}"))
}

/// Require that the target's parent directory already exists.
pub fn require_parent_exists(target: &Path) -> io::Result<()> {
    match target.parent() {
        Some(parent) if parent.is_dir() => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "parent directory must exist",
        )),
    }
}
