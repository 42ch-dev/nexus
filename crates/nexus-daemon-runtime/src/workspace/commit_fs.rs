//! Recoverable workspace commit filesystem primitives (v1.188 P3 L2).
//!
//! Every workspace-relative mutation walks the scope directory component by
//! component with `openat(..., O_DIRECTORY|O_NOFOLLOW)`, so no path component
//! can be swapped for a symlink between resolution and use. The preimage
//! check and the mutating syscall (`renameat` / `unlinkat` / `linkat`) run
//! against the SAME directory descriptor, and the result is re-verified under
//! that descriptor, so a raced path swap is detected rather than accepted.
//!
//! External writers are not excluded — OCC detects third-state bytes at the
//! mutation boundary and surfaces them as a typed error.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
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

/// Scope-bound mutation handle over a verified directory descriptor.
pub struct ScopeMutation {
    scope_path: PathBuf,
    #[cfg(unix)]
    scope: unix_dir::DirFd,
}

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
        #[cfg(unix)]
        {
            let root = unix_dir::DirFd::open(canonical_root)?;
            let components = if scope_relative.is_empty() {
                Vec::new()
            } else {
                split_relative(scope_relative)?
            };
            let scope = unix_dir::walk_scope(&root, &components)?;
            Ok(Self { scope_path, scope })
        }
        #[cfg(not(unix))]
        {
            if !scope_path.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "scope directory must exist",
                ));
            }
            Ok(Self { scope_path })
        }
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

    /// Replace only when the current hash matches `expected_hash`.
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

    /// Delete only when the current hash matches `expected_hash`.
    pub fn atomic_delete_verified(&self, rel_path: &str, expected_hash: &str) -> io::Result<()> {
        self.with_parent(rel_path, |parent, name| {
            parent.atomic_delete_verified(name, expected_hash)
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
    pub fn cleanup_basename(&self, rel_path: &str, basename: &str) {
        let _ = self.with_parent(rel_path, |parent, _| parent.unlink(basename));
    }

    fn with_parent<T, F>(&self, rel_path: &str, f: F) -> io::Result<T>
    where
        F: FnOnce(&ParentDir, &str) -> io::Result<T>,
    {
        let (parent, name) = self.resolve_parent(rel_path)?;
        f(&parent, &name)
    }

    fn resolve_parent(&self, rel_path: &str) -> io::Result<(ParentDir, String)> {
        let components = split_relative(rel_path)?;
        #[cfg(unix)]
        {
            let (dir, name) = unix_dir::walk_child(&self.scope, &components)?;
            Ok((ParentDir::Unix(dir), name))
        }
        #[cfg(not(unix))]
        {
            let parent_path = if components.len() == 1 {
                self.scope_path.clone()
            } else {
                self.scope_path.join(components[..components.len() - 1].join("/"))
            };
            let name = components.last().cloned().unwrap();
            Ok((ParentDir::Path(parent_path), name))
        }
    }
}

/// Basename used for the rollback restore stage.
///
/// Derived (never the commit stage name) so a still-present commit stage is
/// never clobbered while evidence of the failed apply is preserved.
fn restore_basename(stage_basename: &str) -> String {
    format!("{stage_basename}-rb")
}

enum ParentDir {
    #[cfg(unix)]
    Unix(unix_dir::DirFd),
    #[cfg(not(unix))]
    Path(PathBuf),
}

impl ParentDir {
    fn exists(&self, name: &str) -> io::Result<bool> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.exists(name),
            #[cfg(not(unix))]
            Self::Path(path) => Ok(path.join(name).exists()),
        }
    }

    fn hash_file(&self, name: &str) -> io::Result<String> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.hash_file(name),
            #[cfg(not(unix))]
            Self::Path(path) => hash_file(&path.join(name)),
        }
    }

    fn write_stage_file(&self, name: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.write_stage_file(name, bytes, mode),
            #[cfg(not(unix))]
            Self::Path(path) => {
                let stage = path.join(name);
                write_stage_file_with_mode(&stage, bytes, mode)
            }
        }
    }

    fn backup_file(&self, name: &str, backup_basename: &str) -> io::Result<Option<u32>> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.backup_file(name, backup_basename),
            #[cfg(not(unix))]
            Self::Path(path) => {
                let src = path.join(name);
                let backup = path.join(backup_basename);
                let mode = read_file_mode(&src);
                backup_file(&src, &backup)?;
                Ok(mode)
            }
        }
    }

    fn atomic_create(&self, name: &str, stage_basename: &str) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.atomic_create(name, stage_basename),
            #[cfg(not(unix))]
            Self::Path(path) => {
                atomic_create_noclobber(&path.join(stage_basename), &path.join(name))
            }
        }
    }

    fn atomic_replace_verified(
        &self,
        name: &str,
        stage_basename: &str,
        expected_hash: &str,
    ) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.atomic_replace_verified(name, stage_basename, expected_hash),
            #[cfg(not(unix))]
            Self::Path(path) => atomic_replace_verified(
                &path.join(name),
                &path.join(stage_basename),
                expected_hash,
            ),
        }
    }

    fn atomic_delete_verified(&self, name: &str, expected_hash: &str) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.atomic_delete_verified(name, expected_hash),
            #[cfg(not(unix))]
            Self::Path(path) => atomic_delete_verified(&path.join(name), expected_hash),
        }
    }

    fn restore_preimage(
        &self,
        name: &str,
        backup_basename: Option<&str>,
        stage_basename: &str,
        pre_hash: Option<&str>,
        post_hash: Option<&str>,
        mode: Option<u32>,
    ) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => {
                dir.restore_preimage(name, backup_basename, stage_basename, pre_hash, post_hash, mode)
            }
            #[cfg(not(unix))]
            Self::Path(path) => {
                let target = path.join(name);
                let backup = backup_basename.map(|b| path.join(b));
                restore_preimage_verified(
                    &target,
                    backup.as_deref(),
                    pre_hash,
                    post_hash,
                    mode,
                    &restore_basename(stage_basename),
                )
            }
        }
    }

    fn unlink(&self, name: &str) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(dir) => dir.unlink(name),
            #[cfg(not(unix))]
            Self::Path(path) => {
                let _ = fs::remove_file(path.join(name));
                Ok(())
            }
        }
    }
}

/// Split a validated relative path into its components.
///
/// Rejects absolute paths, `..`, `.`, path prefixes, and empty segments.
fn split_relative(rel_path: &str) -> io::Result<Vec<String>> {
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
    use super::CREATE_FILE_MODE;
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
            linkat(
                Some(self.raw_fd()),
                stage_basename,
                Some(self.raw_fd()),
                target,
                AtFlags::empty(),
            )
            .map_err(|err| match err {
                nix::errno::Errno::EEXIST => {
                    io::Error::new(io::ErrorKind::AlreadyExists, "target exists")
                }
                other => errno_io(other),
            })?;
            self.unlink(stage_basename)?;
            self.sync()?;
            Ok(())
        }

        /// Replace only when the target currently carries `expected_hash`.
        ///
        /// The preimage check, the `renameat`, and the post-rename
        /// re-verification all run against THIS directory descriptor, so a
        /// raced path swap is detected instead of accepted.
        pub fn atomic_replace_verified(
            &self,
            target: &str,
            stage_basename: &str,
            expected_hash: &str,
        ) -> io::Result<()> {
            let current = self.hash_file(target)?;
            if current != expected_hash {
                return Err(third_state("replace", "preimage mismatch"));
            }
            let staged = self.hash_file(stage_basename)?;
            renameat(
                Some(self.raw_fd()),
                stage_basename,
                Some(self.raw_fd()),
                target,
            )
            .map_err(errno_io)?;
            let after = self.hash_file(target)?;
            if after != staged {
                return Err(third_state("replace", "post-rename bytes differ"));
            }
            self.sync()?;
            Ok(())
        }

        /// Delete only when the target currently carries `expected_hash`.
        pub fn atomic_delete_verified(&self, target: &str, expected_hash: &str) -> io::Result<()> {
            let current = self.hash_file(target)?;
            if current != expected_hash {
                return Err(third_state("delete", "preimage mismatch"));
            }
            self.unlink(target)?;
            self.sync()?;
            Ok(())
        }

        /// Restore the preimage from the backup (or remove a created target).
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
                    let restore = super::restore_basename(stage_basename);
                    self.write_stage_file(&restore, &bytes, mode)?;
                    let outcome = if self.exists(target)? {
                        let expected = post_hash.unwrap_or(pre_hash.unwrap_or(""));
                        self.atomic_replace_verified(target, &restore, expected)
                    } else {
                        self.atomic_create(target, &restore)
                    };
                    if outcome.is_err() {
                        // Preserve the restore material: it is evidence.
                        return outcome;
                    }
                    Ok(())
                }
                _ => {
                    if self.exists(target)? {
                        let expected = post_hash.unwrap_or(pre_hash.unwrap_or(""));
                        self.atomic_delete_verified(target, expected)?;
                    }
                    Ok(())
                }
            }
        }

        /// `unlinkat` on this descriptor (never follows the final component).
        pub fn unlink(&self, name: &str) -> io::Result<()> {
            unlinkat(Some(self.raw_fd()), name, nix::unistd::UnlinkatFlags::NoRemoveDir)
                .map_err(errno_io)
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

/// Whether `path` is a symlink (never follows it).
pub fn is_symlink(path: &Path) -> io::Result<bool> {
    Ok(fs::symlink_metadata(path)?.file_type().is_symlink())
}

/// fsync a file and its parent directory.
pub fn fsync_file_and_parent(path: &Path) -> io::Result<()> {
    if path.exists() {
        let file = OpenOptions::new().write(true).open(path)?;
        file.sync_all()?;
    }
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }
    }
    Ok(())
}

/// Hash a path (path-based fallback; refuses symlinks).
pub fn hash_file(path: &Path) -> io::Result<String> {
    if is_symlink(path)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "symlink not allowed"));
    }
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut sha = Sha256::new();
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

#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut sha = Sha256::new();
    sha.update(bytes);
    hex::encode(sha.finalize())
}

/// Write a new exclusive stage file (create-new, no-clobber).
pub fn write_stage_file_with_mode(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "stage exists"));
    }
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fsync_file_and_parent(path)?;
    Ok(())
}

/// Copy `src` to a new exclusive `backup` (path-based fallback).
pub fn backup_file(src: &Path, backup: &Path) -> io::Result<()> {
    if is_symlink(src)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot backup symlink"));
    }
    if backup.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "backup exists"));
    }
    let mut src_file = OpenOptions::new().read(true).open(src)?;
    let mut bytes = Vec::new();
    src_file.read_to_end(&mut bytes)?;
    let mode = read_file_mode(src);
    write_stage_file_with_mode(backup, &bytes, mode)?;
    Ok(())
}

pub fn read_file_mode(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).ok().map(|m| m.permissions().mode())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Atomic no-clobber create (`hard_link` fails with `EEXIST`).
pub fn atomic_create_noclobber(stage: &Path, target: &Path) -> io::Result<()> {
    fs::hard_link(stage, target)?;
    fs::remove_file(stage)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

/// Replace only when the current hash matches `expected_hash`.
pub fn atomic_replace_verified(target: &Path, stage: &Path, expected_hash: &str) -> io::Result<()> {
    let current = hash_file(target)?;
    if current != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at replace boundary: preimage mismatch",
        ));
    }
    let staged = hash_file(stage)?;
    fs::rename(stage, target)?;
    if hash_file(target)? != staged {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at replace boundary: post-rename bytes differ",
        ));
    }
    fsync_file_and_parent(target)?;
    Ok(())
}

/// Delete only when the current hash matches `expected_hash`.
pub fn atomic_delete_verified(target: &Path, expected_hash: &str) -> io::Result<()> {
    if is_symlink(target)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot delete symlink"));
    }
    let current = hash_file(target)?;
    if current != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at delete boundary: preimage mismatch",
        ));
    }
    fs::remove_file(target)?;
    if let Some(parent) = target.parent() {
        if parent.exists() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }
    }
    Ok(())
}

/// Restore the preimage from a backup (or remove a created target).
pub fn restore_preimage_verified(
    target: &Path,
    backup: Option<&Path>,
    pre_hash: Option<&str>,
    post_hash: Option<&str>,
    mode: Option<u32>,
    restore_basename: &str,
) -> io::Result<()> {
    if target.exists() {
        let current = hash_file(target)?;
        match (post_hash, pre_hash) {
            (Some(post), _) if current != post => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "third-state during rollback: target is not the applied postimage",
                ));
            }
            (None, Some(pre)) if current != pre => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected bytes during rollback: target is not the preimage",
                ));
            }
            _ => {}
        }
    }
    match backup {
        Some(backup_path) if backup_path.exists() => {
            let mut file = OpenOptions::new().read(true).open(backup_path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let stage = target.with_file_name(restore_basename);
            write_stage_file_with_mode(&stage, &bytes, mode.or_else(|| read_file_mode(backup_path)))?;
            if target.exists() {
                let expected = post_hash.unwrap_or(pre_hash.unwrap_or(""));
                atomic_replace_verified(target, &stage, expected)?;
            } else {
                atomic_create_noclobber(&stage, target)?;
            }
            Ok(())
        }
        _ => {
            if target.exists() {
                atomic_delete_verified(target, post_hash.unwrap_or(pre_hash.unwrap_or("")))?;
            }
            Ok(())
        }
    }
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
