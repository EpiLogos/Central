//! Shared native source serialization and atomic publication, plus the
//! source-path and revision-identity helpers every native writer shares.
//! Authority remains with source owners; this module never decides that a
//! caller may write.
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt},
    io::AsRawFd,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
pub(crate) const MAX_SOURCE: usize = 4 * 1024 * 1024;
pub(crate) struct SourceLock(File);
impl Drop for SourceLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
pub(crate) fn lock(root: &Path, name: &str) -> io::Result<SourceLock> {
    let dir = root.join(".central");
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    };
    reject_symlink_components(root, Path::new(".central"))?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(name))?;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(SourceLock(f))
}
pub(crate) fn read(root: &Path, relative: &str) -> io::Result<String> {
    let f = crate::file_mutation::open_native_file(root, relative)?;
    if !f.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source requires a regular file",
        ));
    }
    let mut bytes = Vec::new();
    f.take((MAX_SOURCE + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SOURCE || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Source exceeds bounded UTF-8 text constraints",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Source is not UTF-8 text"))
}
pub(crate) fn replace(root: &Path, relative: &str, basis: &str, content: &str) -> io::Result<()> {
    if content.len() > MAX_SOURCE || content.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source exceeds bounded text constraints",
        ));
    }
    let original = crate::file_mutation::open_native_file(root, relative)?;
    let metadata = original.metadata()?;
    if metadata.nlink() != 1 || metadata.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source mutation requires one writable regular file link",
        ));
    }
    let path = Path::new(relative);
    let parent = crate::file_mutation::directory(
        root,
        path.parent()
            .ok_or_else(|| io::Error::other("Missing source parent"))?,
    )?;
    let name = format!(
        ".central-source-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    let mut staged = crate::file_mutation::create_in(&parent, &name, metadata.mode() & 0o777)?;
    let result = (|| {
        #[cfg(target_os = "macos")]
        {
            unsafe extern "C" {
                fn fcopyfile(
                    from: libc::c_int,
                    to: libc::c_int,
                    state: *mut libc::c_void,
                    flags: libc::c_uint,
                ) -> libc::c_int;
            }
            if unsafe {
                fcopyfile(
                    original.as_raw_fd(),
                    staged.as_raw_fd(),
                    std::ptr::null_mut(),
                    7,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        staged.set_permissions(metadata.permissions())?;
        staged.write_all(content.as_bytes())?;
        staged.sync_all()?;
        let now = crate::file_mutation::open_native_file(root, relative)?.metadata()?;
        let now_parent =
            crate::file_mutation::directory(root, path.parent().unwrap())?.metadata()?;
        let held_parent = parent.metadata()?;
        if now.dev() != metadata.dev()
            || now.ino() != metadata.ino()
            || now_parent.dev() != held_parent.dev()
            || now_parent.ino() != held_parent.ino()
            || content_revision_bytes(read(root, relative)?.as_bytes()) != basis
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Source changed before atomic publication",
            ));
        }
        crate::file_mutation::rename_in(
            &parent,
            &name,
            path.file_name()
                .and_then(|v| v.to_str())
                .ok_or_else(|| io::Error::other("Invalid source filename"))?,
        )?;
        parent.sync_all()
    })();
    let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    unsafe {
        libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0);
    }
    result
}

// --- Shared source-path and revision-identity helpers, re-homed from the
// retired flow store: they never belonged to Flows alone. ---

pub(crate) fn relative_member(raw: &str) -> io::Result<PathBuf> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source path must be non-empty without surrounding whitespace",
        ));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source path must stay inside its owner world and contain no parent/root components",
        ));
    }
    Ok(path.to_path_buf())
}

pub(crate) fn reject_symlink_components(project_root: &Path, relative: &Path) -> io::Result<()> {
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "non-normal source path",
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "refusing symlink source path component: {}",
                        current.display()
                    ),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(crate) fn safe_source_member_path(
    project_root: &Path,
    raw: &str,
    must_exist: bool,
) -> io::Result<PathBuf> {
    let relative = relative_member(raw)?;
    reject_symlink_components(project_root, &relative)?;
    let path = project_root.join(&relative);
    if must_exist {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source must be an ordinary file",
            ));
        }
        let root = fs::canonicalize(project_root)?;
        let source = fs::canonicalize(&path)?;
        if !source.starts_with(root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source escaped its Project world",
            ));
        }
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        reject_symlink_components(project_root, relative.parent().unwrap_or(Path::new("")))?;
    }
    Ok(path)
}

/// The content revision of a byte span: the scheme every recorded source
/// revision uses, exposed so read-only callers can compare a retained source
/// against its recorded revision without reconciling anything.
pub(crate) fn content_revision_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len())
}

pub(crate) fn validate_actor_kind(kind: &str) -> io::Result<()> {
    if matches!(kind, "human" | "agent" | "system") {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "actor_kind must be human, agent, or system",
        ))
    }
}
