//! Shared native source serialization and atomic publication. Authority remains
//! with source/Flow owners; this module never decides that a caller may write.
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt},
    io::AsRawFd,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
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
    crate::projectcentral_flow::reject_symlink_components(root, Path::new(".central"))?;
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
            || crate::projectcentral_flow::content_revision_bytes(read(root, relative)?.as_bytes())
                != basis
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
