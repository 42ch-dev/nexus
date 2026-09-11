//! Recoverable workspace commit filesystem primitives (v1.188 P3 L2).
//!
//! Descriptor-relative no-follow operations; stage/backup files live in the
//! same directory as their target. External writers are not excluded — OCC
//! detects third-state bytes at the mutation boundary.

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

/// Scope-bound mutation handle using verified directory-relative lookups.
pub struct ScopeMutation {
    scope: PathBuf,
}

impl ScopeMutation {
    /// Open a scope directory for descriptor-relative mutations.
    pub fn open(scope_dir: &Path) -> io::Result<Self> {
        if !scope_dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "scope directory must exist",
            ));
        }
        Ok(Self {
            scope: scope_dir.to_path_buf(),
        })
    }

    /// Whether a relative target exists as a regular file (no symlink follow).
    pub fn target_exists(&self, rel_path: &str) -> io::Result<bool> {
        self.with_parent(rel_path, |parent, name| parent.exists(name))
    }

    /// Hash a relative target through a no-follow open.
    pub fn hash_target(&self, rel_path: &str) -> io::Result<String> {
        self.with_parent(rel_path, |parent, name| parent.hash_file(name))
    }

    /// Write an exclusive stage file beside the target.
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

    /// Atomic create via rename within the parent directory.
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

    /// Restore preimage during rollback using descriptor-relative operations.
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
            let (dir, name) = unix_dir::walk_parent(&self.scope, &components)?;
            Ok((ParentDir::Unix(dir), name))
        }
        #[cfg(not(unix))]
        {
            let parent_path = if components.len() == 1 {
                self.scope.clone()
            } else {
                self.scope.join(components[..components.len() - 1].join("/"))
            };
            let name = components.last().cloned().unwrap();
            Ok((ParentDir::Path(parent_path), name))
        }
    }
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
            Self::Path(path) => atomic_create(&path.join(name), &path.join(stage_basename)),
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
            Self::Path(path) => {
                atomic_replace_verified(
                    &path.join(name),
                    &path.join(stage_basename),
                    expected_hash,
                )
            }
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
                    stage_basename,
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
    use nix::fcntl::{openat, renameat, OFlag};
    use nix::sys::stat::Mode;
    use nix::unistd::unlinkat;
    use sha2::Digest;
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    pub struct DirFd {
        file: File,
    }

    impl DirFd {
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

        pub fn write_stage_file(&self, name: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
            if self.exists(name)? {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "stage exists"));
            }
            let mode = Mode::from_bits_truncate(mode.unwrap_or(CREATE_FILE_MODE) as u16);
            let fd = openat(
                Some(self.raw_fd()),
                name,
                OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                mode,
            )
            .map_err(errno_io)?;
            let mut file = adopt_fd(fd, true)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            self.sync()?;
            Ok(())
        }

        pub fn backup_file(&self, name: &str, backup_basename: &str) -> io::Result<Option<u32>> {
            let mut src = self.openat(name, OFlag::O_RDONLY)?;
            let mode = file_mode_fd(&src);
            let mut bytes = Vec::new();
            src.read_to_end(&mut bytes)?;
            self.write_stage_file(backup_basename, &bytes, mode)?;
            Ok(mode)
        }

        pub fn atomic_create(&self, target: &str, stage_basename: &str) -> io::Result<()> {
            if self.exists(target)? {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "target exists"));
            }
            renameat(
                Some(self.raw_fd()),
                stage_basename,
                Some(self.raw_fd()),
                target,
            )
            .map_err(errno_io)?;
            self.sync()?;
            Ok(())
        }

        pub fn atomic_replace_verified(
            &self,
            target: &str,
            stage_basename: &str,
            expected_hash: &str,
        ) -> io::Result<()> {
            let current = self.hash_file(target)?;
            if current != expected_hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "third-state bytes at replace boundary",
                ));
            }
            renameat(
                Some(self.raw_fd()),
                stage_basename,
                Some(self.raw_fd()),
                target,
            )
            .map_err(errno_io)?;
            self.openat(target, OFlag::O_RDONLY)?.sync_all()?;
            self.sync()?;
            Ok(())
        }

        pub fn atomic_delete_verified(&self, target: &str, expected_hash: &str) -> io::Result<()> {
            let current = self.hash_file(target)?;
            if current != expected_hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "third-state bytes at delete boundary",
                ));
            }
            self.unlink(target)?;
            self.sync()?;
            Ok(())
        }

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
                if let Some(post) = post_hash {
                    if current != post {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "third-state during rollback",
                        ));
                    }
                } else if let Some(pre) = pre_hash {
                    if current != pre {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unexpected bytes during rollback",
                        ));
                    }
                }
            }
            match backup_basename {
                Some(backup) if self.exists(backup)? => {
                    let mut file = self.openat(backup, OFlag::O_RDONLY)?;
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)?;
                    self.write_stage_file(stage_basename, &bytes, mode)?;
                    if self.exists(target)? {
                        let expected = post_hash.unwrap_or(pre_hash.unwrap_or(""));
                        self.atomic_replace_verified(target, stage_basename, expected)?;
                    } else {
                        self.atomic_create(target, stage_basename)?;
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

        pub fn unlink(&self, name: &str) -> io::Result<()> {
            unlinkat(Some(self.raw_fd()), name, nix::unistd::UnlinkatFlags::NoRemoveDir)
                .map_err(errno_io)
        }

        fn openat(&self, name: &str, flags: OFlag) -> io::Result<File> {
            let fd = openat(
                Some(self.raw_fd()),
                name,
                flags | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(errno_io)?;
            adopt_fd(fd, false)
        }
    }

    pub fn walk_parent(scope_path: &Path, components: &[String]) -> io::Result<(DirFd, String)> {
        if components.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
        }
        if components.len() == 1 {
            return Ok((DirFd::open(scope_path)?, components[0].clone()));
        }
        let parent_path = scope_path.join(components[..components.len() - 1].join("/"));
        Ok((DirFd::open(&parent_path)?, components.last().cloned().unwrap()))
    }


    fn adopt_fd(fd: i32, write: bool) -> io::Result<File> {
        let path = format!("/dev/fd/{}", fd);
        let mut opts = OpenOptions::new();
        opts.read(true);
        if write {
            opts.write(true);
        }
        let file = opts.open(&path)?;
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

pub fn is_symlink(path: &Path) -> io::Result<bool> {
    Ok(fs::symlink_metadata(path)?.file_type().is_symlink())
}

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
pub fn write_stage_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_stage_file_with_mode(path, bytes, None)
}

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

/// Atomic create via rename; fails if target exists (no-clobber).
pub fn atomic_create(target: &Path, stage: &Path) -> io::Result<()> {
    if target.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "target exists"));
    }
    fs::rename(stage, target)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

/// Replace only when current hash matches `expected` (third-state guard).
pub fn atomic_replace_verified(target: &Path, stage: &Path, expected_hash: &str) -> io::Result<()> {
    let current = hash_file(target)?;
    if current != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at replace boundary",
        ));
    }
    fs::rename(stage, target)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

pub fn atomic_delete_verified(target: &Path, expected_hash: &str) -> io::Result<()> {
    if is_symlink(target)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot delete symlink"));
    }
    let current = hash_file(target)?;
    if current != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at delete boundary",
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

/// Restore preimage when current bytes match `post_hash` or target absent for create rollback.
pub fn restore_preimage_verified(
    target: &Path,
    backup: Option<&Path>,
    pre_hash: Option<&str>,
    post_hash: Option<&str>,
    mode: Option<u32>,
    stage_basename: &str,
) -> io::Result<()> {
    if target.exists() {
        let current = hash_file(target)?;
        if let Some(post) = post_hash {
            if current != post {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "third-state during rollback",
                ));
            }
        } else if let Some(pre) = pre_hash {
            if current != pre {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected bytes during rollback",
                ));
            }
        }
    }
    match backup {
        Some(backup_path) if backup_path.exists() => {
            let mut file = OpenOptions::new().read(true).open(backup_path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let stage = target.with_file_name(stage_basename);
            if stage.exists() {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "restore stage exists"));
            }
            write_stage_file_with_mode(&stage, &bytes, mode.or_else(|| read_file_mode(backup_path)))?;
            if target.exists() {
                let pre = pre_hash.unwrap_or("");
                atomic_replace_verified(target, &stage, post_hash.unwrap_or(pre))?;
            } else {
                atomic_create(target, &stage)?;
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

pub fn cleanup_temp(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

pub fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("invalid base64: {e}"))
}

pub fn require_parent_exists(target: &Path) -> io::Result<()> {
    match target.parent() {
        Some(parent) if parent.is_dir() => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "parent directory must exist",
        )),
    }
}
