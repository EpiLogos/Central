//! Physical publication shared with AIKit's canonical Wiki writers. Semantic
//! validation and revisions remain the caller's responsibility. The permanent
//! sibling lock is `.<filename>.publication.lock` (flock, five-second deadline).
//! Source bytes are read and compared while that lock is held. Staging is unique,
//! metadata-preserving and durable; initial creation never clobbers a winner.
//! Legacy staging and construction locks are neither consumed nor removed.
use std::ffi::{CString, OsStr, OsString};
use std::fs::{self, File, Metadata};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

/// The atomic publication succeeded, but its durable native source could not
/// be confirmed. Keep this payload typed until the owner constructs its Return.
#[derive(Debug)]
pub(crate) struct PublicationUncertain {
    pub source_path: PathBuf,
    pub published: bool,
    pub cause: io::Error,
}
impl std::fmt::Display for PublicationUncertain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter,
            "Publication at {} occurred, but durability/readback failed: {}; inspect the original native action and source before retrying",
            self.source_path.display(), self.cause)
    }
}
impl std::error::Error for PublicationUncertain {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
pub(crate) fn uncertainty(error: &io::Error) -> Option<&PublicationUncertain> {
    error.get_ref()?.downcast_ref::<PublicationUncertain>()
}

fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn conflict(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, message)
}
fn native_name(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_encoded_bytes()).map_err(io::Error::other)
}
fn open_in(parent: &File, name: &OsStr, flags: libc::c_int) -> io::Result<File> {
    let name = native_name(name)?;
    // The parent descriptor and O_NOFOLLOW retain the physical identity even
    // if an external process redirects a pathname while the owner is working.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn ordinary(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(denied(
            "Wiki publication requires an ordinary source with one physical link",
        ));
    }
    Ok(())
}
fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

struct Snapshot {
    file: File,
    metadata: Metadata,
    bytes: Vec<u8>,
}
impl Snapshot {
    fn read(parent: &File, name: &OsStr) -> io::Result<Option<Self>> {
        let mut file = match open_in(parent, name, libc::O_RDONLY | libc::O_NONBLOCK) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        ordinary(&metadata)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_SOURCE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SOURCE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Wiki publication source exceeds 8 MiB",
            ));
        }
        Ok(Some(Self {
            file,
            metadata,
            bytes,
        }))
    }
    fn matches(&self, current: &Self) -> bool {
        same_file(&self.metadata, &current.metadata)
            && self.bytes == current.bytes
            && self.metadata.mode() == current.metadata.mode()
            && self.metadata.uid() == current.metadata.uid()
            && self.metadata.gid() == current.metadata.gid()
            && self.metadata.ctime() == current.metadata.ctime()
            && self.metadata.ctime_nsec() == current.metadata.ctime_nsec()
    }
}

pub(crate) struct Publication {
    parent_path: PathBuf,
    parent: File,
    name: OsString,
    lock_name: OsString,
    lock: File,
    current: Option<Snapshot>,
}
impl Drop for Publication {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.lock.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
impl Publication {
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        let name = path
            .file_name()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Wiki publication has no filename",
                )
            })?
            .to_os_string();
        let parent_path = fs::canonicalize(path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Wiki publication has no parent",
            )
        })?)?;
        use std::os::unix::fs::OpenOptionsExt;
        let parent = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&parent_path)?;
        let mut lock_name = OsString::from(".");
        lock_name.push(&name);
        lock_name.push(".publication.lock");
        let lock = open_in(
            &parent,
            &lock_name,
            libc::O_RDWR | libc::O_CREAT | libc::O_NONBLOCK,
        )?;
        ordinary(&lock.metadata()?)?;
        let started = Instant::now();
        loop {
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::WouldBlock
                && error.kind() != io::ErrorKind::Interrupted
            {
                return Err(error);
            }
            let remaining = LOCK_TIMEOUT.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Wiki publication lock was not acquired within five seconds; source unchanged",
                ));
            }
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
        }
        let current = Snapshot::read(&parent, &name)?;
        let publication = Self {
            parent_path,
            parent,
            name,
            lock_name,
            lock,
            current,
        };
        publication.check_basis()?;
        Ok(publication)
    }
    pub(crate) fn bytes(&self) -> Option<&[u8]> {
        self.current
            .as_ref()
            .map(|current| current.bytes.as_slice())
    }
    fn check_basis(&self) -> io::Result<()> {
        if !same_file(&self.parent.metadata()?, &fs::metadata(&self.parent_path)?) {
            return Err(conflict(
                "Wiki publication parent changed; source unchanged",
            ));
        }
        let lock = open_in(
            &self.parent,
            &self.lock_name,
            libc::O_RDONLY | libc::O_NONBLOCK,
        )?;
        ordinary(&lock.metadata()?)?;
        if !same_file(&lock.metadata()?, &self.lock.metadata()?) {
            return Err(conflict(
                "Wiki publication lock identity changed; source unchanged",
            ));
        }
        match (&self.current, Snapshot::read(&self.parent, &self.name)?) {
            (None, None) => Ok(()),
            (Some(basis), Some(current)) if basis.matches(&current) => Ok(()),
            _ => Err(conflict(
                "Wiki publication source changed outside the owner lock; source not replaced",
            )),
        }
    }
    pub(crate) fn replace(&self, bytes: &[u8]) -> io::Result<bool> {
        if self.bytes() == Some(bytes) {
            self.check_basis()?;
            return Ok(false);
        }
        if let Some(current) = &self.current {
            if current.metadata.mode() & 0o222 == 0 {
                return Err(denied(
                    "Wiki publication refuses to replace a readonly source",
                ));
            }
            // Atomic replacement must not bypass the source ACL merely
            // because its parent directory permits a rename.
            let writable = open_in(&self.parent, &self.name, libc::O_WRONLY | libc::O_NONBLOCK)?;
            if !same_file(&current.metadata, &writable.metadata()?) {
                return Err(conflict("Wiki source changed during write admission"));
            }
        }
        self.publish(bytes, self.current.as_ref(), None)?;
        Ok(true)
    }
    pub(crate) fn create_new(&self, bytes: &[u8]) -> io::Result<()> {
        if self.current.is_some() {
            return Err(conflict("Refusing to overwrite an existing native source"));
        }
        self.publish(bytes, None, None)
    }
    pub(crate) fn copy_new(&self, source: &Path) -> io::Result<()> {
        if self.current.is_some() {
            return Err(conflict(
                "Refusing to overwrite an existing migration target",
            ));
        }
        let source_parent = fs::canonicalize(source.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Migration source has no parent",
            )
        })?)?;
        let parent = File::open(&source_parent)?;
        let name = source.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Migration source has no filename",
            )
        })?;
        let snapshot = Snapshot::read(&parent, name)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Migration source disappeared")
        })?;
        self.publish(
            &snapshot.bytes,
            Some(&snapshot),
            Some((&parent, name, &snapshot)),
        )
    }
    fn publish(
        &self,
        bytes: &[u8],
        metadata_source: Option<&Snapshot>,
        copy_basis: Option<(&File, &OsStr, &Snapshot)>,
    ) -> io::Result<()> {
        if bytes.len() as u64 > MAX_SOURCE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Wiki publication exceeds 8 MiB",
            ));
        }
        let stage_name = OsString::from(format!(
            ".central-publication-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut stage = open_in(
            &self.parent,
            &stage_name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        )?;
        let mut committed = false;
        let result = (|| {
            // Capture the actual create-new defaults before any candidate
            // bytes or observer can alter this inode. Retained sources instead
            // supply their own immutable expectation before metadata copying.
            let defaults = Privacy::read(&stage)?;
            #[cfg(test)]
            tests::stage_created(&self.parent_path.join(&stage_name));
            let expected = if let Some(source) = metadata_source {
                let expected = Privacy::read(&source.file)?;
                verify_ownership_mode(&source.metadata, &expected.metadata)?;
                preserve_metadata(source, &stage)?;
                expected
            } else {
                defaults
            };
            // Refuse unsupported or changed privacy before materialising the
            // candidate. A stage and its eventual published source are one
            // mutable inode, so neither can be the other's expected privacy.
            expected.verify(&stage)?;
            self.check_basis()?;
            if let Some((parent, name, basis)) = copy_basis {
                let current = Snapshot::read(parent, name)?
                    .ok_or_else(|| conflict("Migration source disappeared"))?;
                if !basis.matches(&current) {
                    return Err(conflict("Migration source changed before staging"));
                }
            }
            stage.set_len(0)?;
            stage.seek(SeekFrom::Start(0))?;
            stage.write_all(bytes)?;
            stage.sync_all()?;
            expected.verify(&stage)?;
            self.check_basis()?;
            if let Some((parent, name, basis)) = copy_basis {
                let current = Snapshot::read(parent, name)?
                    .ok_or_else(|| conflict("Migration source disappeared"))?;
                if !basis.matches(&current) {
                    return Err(conflict("Migration source changed before publication"));
                }
            }
            // Publish only the create-new inode held by this operation.
            // A substituted named stage must not become native source.
            let named_stage =
                open_in(&self.parent, &stage_name, libc::O_RDONLY | libc::O_NONBLOCK)?;
            ordinary(&named_stage.metadata()?)?;
            if !same_file(&stage.metadata()?, &named_stage.metadata()?) {
                return Err(conflict(
                    "Publication staging identity changed; source unchanged",
                ));
            }
            let stage_c = native_name(&stage_name)?;
            let target_c = native_name(&self.name)?;
            if self.current.is_some() {
                if unsafe {
                    libc::renameat(
                        self.parent.as_raw_fd(),
                        stage_c.as_ptr(),
                        self.parent.as_raw_fd(),
                        target_c.as_ptr(),
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
            } else {
                rename_new(&self.parent, &stage_c, &target_c)?;
            }
            committed = true;
            #[cfg(test)]
            tests::published(&self.parent_path.join(&self.name));
            self.parent.sync_all()?;
            if !same_file(&self.parent.metadata()?, &fs::metadata(&self.parent_path)?) {
                return Err(io::Error::other("Published Wiki parent identity changed"));
            }
            let reading = Snapshot::read(&self.parent, &self.name)?
                .ok_or_else(|| io::Error::other("Published Wiki disappeared"))?;
            if reading.bytes != bytes || !same_file(&reading.metadata, &stage.metadata()?) {
                return Err(io::Error::other("Published Wiki readback differs"));
            }
            expected.verify(&reading.file)?;
            if !same_file(&self.parent.metadata()?, &fs::metadata(&self.parent_path)?) {
                return Err(io::Error::other("Published Wiki parent identity changed"));
            }
            Ok(())
        })();
        // Failed staging is retained as bounded evidence. A separate inode
        // comparison followed by pathname unlink cannot atomically prove
        // ownership, and could delete a concurrent foreign replacement.
        // Successful descriptor-relative rename already removes our stage.
        if committed {
            result.map_err(|cause| {
                io::Error::other(PublicationUncertain {
                    source_path: self.parent_path.join(&self.name),
                    published: true,
                    cause,
                })
            })
        } else {
            result.map_err(|error: io::Error| {
                let identity = stage.metadata().map(|metadata| format!(
                    "{}:{}", metadata.dev(), metadata.ino()))
                    .unwrap_or_else(|_| "unavailable".to_string());
                io::Error::new(error.kind(), format!(
                    "{error}; uncommitted stage inode {identity} was not deleted; initial staging name {} (a substituted name is not owned)",
                    self.parent_path.join(&stage_name).display()))
            })
        }
    }
}

fn verify_ownership_mode(source: &Metadata, stage: &Metadata) -> io::Result<()> {
    if (source.uid(), source.gid(), source.mode() & 0o7777)
        != (stage.uid(), stage.gid(), stage.mode() & 0o7777)
    {
        return Err(denied(
            "Source ownership or permissions retention could not be confirmed",
        ));
    }
    Ok(())
}
fn preserve_metadata(source: &Snapshot, stage: &File) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        // Bound retained metadata before the native copy, as well as checking
        // parity after writing. Failed staging must remain bounded evidence.
        extended_attributes(&source.file)?;
        native_acl(&source.file)?;
        unsafe extern "C" {
            fn fcopyfile(
                from: libc::c_int,
                to: libc::c_int,
                state: *mut libc::c_void,
                flags: libc::c_uint,
            ) -> libc::c_int;
        }
        // Native COPYFILE_METADATA = ACL | STAT | XATTR. As in Central's
        // ordinary-file owner, a null state lets copyfile manage its state.
        // Copy before writing so the mutation receives a fresh mtime.
        if unsafe {
            fcopyfile(
                source.file.as_raw_fd(),
                stage.as_raw_fd(),
                std::ptr::null_mut(),
                7,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(target_os = "linux")]
    {
        preserve_linux_metadata(source, stage)?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return Err(io::Error::new(io::ErrorKind::Unsupported,
            "Metadata-preserving Wiki replacement is unsupported on this platform; source unchanged"));
    }
    stage.set_permissions(fs::Permissions::from_mode(source.metadata.mode() & 0o7777))?;
    verify_ownership_mode(&source.metadata, &stage.metadata()?)
}

// This is an invocation-local physical expectation, not another source or
// semantic registry. Freeze it before writing: kernels may clear security
// xattrs on data mutation, and a foreign actor may change the published inode.
// Linux POSIX ACLs are included in xattrs; macOS has a separate native ACL.
struct Privacy {
    metadata: Metadata,
    attributes: std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
    #[cfg(target_os = "macos")]
    acl: Option<Vec<u8>>,
}
impl Privacy {
    fn read(file: &File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        ordinary(&metadata)?;
        let attributes = extended_attributes(file)?;
        #[cfg(target_os = "macos")]
        let acl = native_acl(file)?;
        let current = file.metadata()?;
        ordinary(&current)?;
        if !same_file(&metadata, &current)
            || metadata.ctime() != current.ctime()
            || metadata.ctime_nsec() != current.ctime_nsec()
        {
            return Err(conflict("Publication metadata changed during readback"));
        }
        verify_ownership_mode(&metadata, &current)?;
        Ok(Self {
            metadata,
            attributes,
            #[cfg(target_os = "macos")]
            acl,
        })
    }

    fn verify(&self, file: &File) -> io::Result<()> {
        let reading = Self::read(file)?;
        verify_ownership_mode(&self.metadata, &reading.metadata)?;
        if self.attributes != reading.attributes {
            return Err(denied(
                "Source extended attribute retention could not be confirmed",
            ));
        }
        #[cfg(target_os = "macos")]
        if self.acl != reading.acl {
            return Err(denied("Source ACL retention could not be confirmed"));
        }
        Ok(())
    }
}

const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn extended_attributes(file: &File) -> io::Result<std::collections::BTreeMap<Vec<u8>, Vec<u8>>> {
    fn names(file: &File, buffer: *mut libc::c_char, size: usize) -> libc::ssize_t {
        #[cfg(target_os = "macos")]
        unsafe {
            libc::flistxattr(file.as_raw_fd(), buffer, size, 0)
        }
        #[cfg(target_os = "linux")]
        unsafe {
            libc::flistxattr(file.as_raw_fd(), buffer, size)
        }
    }
    fn value(file: &File, name: &CString, buffer: *mut libc::c_void, size: usize) -> libc::ssize_t {
        #[cfg(target_os = "macos")]
        unsafe {
            libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), buffer, size, 0, 0)
        }
        #[cfg(target_os = "linux")]
        unsafe {
            libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), buffer, size)
        }
    }
    fn bounded_size(size: libc::ssize_t) -> io::Result<usize> {
        if size < 0 {
            return Err(io::Error::last_os_error());
        }
        let size = size as usize;
        if size > MAX_METADATA_BYTES {
            return Err(io::Error::other(
                "Source metadata exceeds publication bound",
            ));
        }
        Ok(size)
    }
    let size = bounded_size(names(file, std::ptr::null_mut(), 0))?;
    let mut names_buffer = vec![0u8; size];
    let read = bounded_size(names(
        file,
        names_buffer.as_mut_ptr().cast(),
        names_buffer.len(),
    ))?;
    if read > names_buffer.len() {
        return Err(conflict("Source metadata changed during readback"));
    }
    names_buffer.truncate(read);
    if !names_buffer.is_empty() && names_buffer.last() != Some(&0) {
        return Err(io::Error::other("Source metadata names are not terminated"));
    }
    let mut total = names_buffer.len();
    let mut attributes = std::collections::BTreeMap::new();
    for name_bytes in names_buffer
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = CString::new(name_bytes).map_err(io::Error::other)?;
        let size = bounded_size(value(file, &name, std::ptr::null_mut(), 0))?;
        total = total
            .checked_add(size)
            .filter(|total| *total <= MAX_METADATA_BYTES)
            .ok_or_else(|| io::Error::other("Source metadata exceeds publication bound"))?;
        let mut bytes = vec![0u8; size];
        let read = bounded_size(value(file, &name, bytes.as_mut_ptr().cast(), bytes.len()))?;
        if read > bytes.len() {
            return Err(conflict("Source metadata changed during readback"));
        }
        bytes.truncate(read);
        attributes.insert(name_bytes.to_vec(), bytes);
    }
    Ok(attributes)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn extended_attributes(_file: &File) -> io::Result<std::collections::BTreeMap<Vec<u8>, Vec<u8>>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Metadata readback is unsupported; source unchanged",
    ))
}

#[cfg(target_os = "macos")]
fn native_acl(file: &File) -> io::Result<Option<Vec<u8>>> {
    unsafe extern "C" {
        fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> *mut libc::c_void;
        fn acl_size(acl: *mut libc::c_void) -> libc::ssize_t;
        fn acl_copy_ext(
            buffer: *mut libc::c_void,
            acl: *mut libc::c_void,
            size: libc::ssize_t,
        ) -> libc::ssize_t;
        fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
    }
    // ACL_TYPE_EXTENDED from the native sys/acl.h contract. Descriptor APIs
    // avoid pathname redirection and serialise identities without name lookup.
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
    if acl.is_null() {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let result = (|| {
        let size = unsafe { acl_size(acl) };
        if size < 0 {
            return Err(io::Error::last_os_error());
        }
        if size as usize > MAX_METADATA_BYTES {
            return Err(io::Error::other("Source ACL exceeds publication bound"));
        }
        let mut bytes = vec![0u8; size as usize];
        let read = unsafe { acl_copy_ext(bytes.as_mut_ptr().cast(), acl, size) };
        if read < 0 {
            return Err(io::Error::last_os_error());
        }
        if read as usize > bytes.len() {
            return Err(io::Error::other("Source ACL size changed during readback"));
        }
        bytes.truncate(read as usize);
        Ok(Some(bytes))
    })();
    unsafe {
        acl_free(acl);
    }
    result
}

#[cfg(target_os = "linux")]
fn preserve_linux_metadata(source: &Snapshot, stage: &File) -> io::Result<()> {
    let stage_meta = stage.metadata()?;
    if (source.metadata.uid(), source.metadata.gid()) != (stage_meta.uid(), stage_meta.gid())
        && unsafe {
            libc::fchown(
                stage.as_raw_fd(),
                source.metadata.uid(),
                source.metadata.gid(),
            )
        } != 0
    {
        return Err(io::Error::last_os_error());
    }
    stage.set_permissions(fs::Permissions::from_mode(source.metadata.mode() & 0o7777))?;
    let attributes = extended_attributes(&source.file)?;
    for name_bytes in extended_attributes(stage)?.keys() {
        if !attributes.contains_key(name_bytes) {
            let name = CString::new(name_bytes.clone()).map_err(io::Error::other)?;
            if unsafe { libc::fremovexattr(stage.as_raw_fd(), name.as_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    for (name_bytes, bytes) in attributes {
        let name = CString::new(name_bytes).map_err(io::Error::other)?;
        if unsafe {
            libc::fsetxattr(
                stage.as_raw_fd(),
                name.as_ptr(),
                bytes.as_ptr().cast(),
                bytes.len(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

// Exclusive rename has one publication effect. A link+unlink implementation
// would expose two links and strand a committed source if the process died
// before unlink, contradicting the single-link restart contract.
fn rename_new(parent: &File, stage: &CString, target: &CString) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let result = {
        unsafe extern "C" {
            fn renameatx_np(
                from_dir: libc::c_int,
                from: *const libc::c_char,
                to_dir: libc::c_int,
                to: *const libc::c_char,
                flags: libc::c_uint,
            ) -> libc::c_int;
        }
        // RENAME_EXCL, as defined by the native sys/stdio.h contract.
        unsafe {
            renameatx_np(
                parent.as_raw_fd(),
                stage.as_ptr(),
                parent.as_raw_fd(),
                target.as_ptr(),
                0x4,
            )
        }
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            parent.as_raw_fd(),
            stage.as_ptr(),
            parent.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        ) as libc::c_int
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Atomic exclusive native source creation is unsupported; source unchanged",
    ));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};

    struct NativeFixture(PathBuf);
    impl NativeFixture {
        fn new_in(parent: &Path) -> Self {
            let path = parent.join(format!("native-source-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for NativeFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    std::thread_local! {
        static ON_STAGE_CREATED: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
            const { std::cell::RefCell::new(None) };
        static ON_PUBLISHED: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
            const { std::cell::RefCell::new(None) };
    }

    pub(crate) fn after_publication(observer: impl FnOnce(&Path) + 'static) {
        ON_PUBLISHED.with(|slot| *slot.borrow_mut() = Some(Box::new(observer)));
    }

    pub(super) fn published(path: &Path) {
        let observer = ON_PUBLISHED.with(|slot| slot.borrow_mut().take());
        if let Some(observer) = observer {
            observer(path);
        }
    }

    pub(super) fn stage_created(path: &Path) {
        ON_STAGE_CREATED.with(|observer| {
            if let Some(observer) = observer.borrow_mut().take() {
                observer(path);
            }
        });
    }

    const EXEC_OBSERVATIONS: &str = "NATIVE_PUBLICATION_EXEC_OBSERVATIONS";
    const EXEC_CONTROL: &str = "NATIVE_PUBLICATION_EXEC_CONTROL";

    fn receipt(file: &File) -> String {
        let metadata = file.metadata().unwrap();
        format!("{},{},{}", file.as_raw_fd(), metadata.dev(), metadata.ino())
    }

    fn inherited(receipt: &str) -> bool {
        let parts: Vec<_> = receipt.split(',').collect();
        let fd: libc::c_int = parts[0].parse().unwrap();
        let dev: u64 = parts[1].parse().unwrap();
        let ino: u64 = parts[2].parse().unwrap();
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd, metadata.as_mut_ptr()) } != 0 {
            return false;
        }
        let metadata = unsafe { metadata.assume_init() };
        metadata.st_dev as u64 == dev && metadata.st_ino as u64 == ino
    }

    #[test]
    #[ignore = "actual exec child entry; invoked by owner descriptor regression"]
    fn exec_inheritance_child() {
        assert!(
            inherited(&std::env::var(EXEC_CONTROL).unwrap()),
            "the deliberate non-CLOEXEC control must cross exec, making the regression sensitive"
        );
        for observation in std::env::var(EXEC_OBSERVATIONS).unwrap().split(';') {
            assert!(
                !inherited(observation),
                "native owner descriptor crossed exec: {observation}"
            );
        }
    }

    #[test]
    fn actual_owner_refusal_and_drop_retain_substituted_stage_and_source() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ProjectCentral/now/tmp")
            .join(format!(
                "native-publication-substitution-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&scratch).unwrap();
        let fixture = Fixture(scratch);
        let path = fixture.0.join("source.json");
        fs::write(&path, b"retained physical source").unwrap();
        let publication = Publication::acquire(&path).unwrap();
        let retained_owned = fixture.0.join("retained-owned-stage");
        let observed = std::sync::Arc::new(std::sync::Mutex::new(None::<PathBuf>));
        let observed_stage = observed.clone();
        let retained_stage = retained_owned.clone();
        // The actual owner is paused only by this test observer. Real native
        // rename/write operations substitute its named staging inode while
        // Publication still owns the original descriptor and kernel lock.
        ON_STAGE_CREATED.with(|observer| {
            *observer.borrow_mut() = Some(Box::new(move |stage| {
                fs::rename(stage, &retained_stage).unwrap();
                fs::write(stage, b"foreign replacement retained").unwrap();
                *observed_stage.lock().unwrap() = Some(stage.to_path_buf());
            }))
        });
        let error = publication.replace(b"uncommitted candidate").unwrap_err();
        assert!(
            uncertainty(&error).is_none(),
            "pre-publication refusal is not an unknown effect"
        );
        assert!(error.to_string().contains("was not deleted"), "{error}");
        drop(publication);
        let substituted = observed.lock().unwrap().clone().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"retained physical source");
        assert_eq!(
            fs::read(&substituted).unwrap(),
            b"foreign replacement retained"
        );
        assert_eq!(fs::read(&retained_owned).unwrap(), b"uncommitted candidate");
        let next = Publication::acquire(&path).unwrap();
        next.replace(b"subsequent acknowledged source").unwrap();
        drop(next);
        assert_eq!(fs::read(&path).unwrap(), b"subsequent acknowledged source");
        assert_eq!(
            fs::read(&substituted).unwrap(),
            b"foreign replacement retained"
        );
        assert_eq!(fs::read(&retained_owned).unwrap(), b"uncommitted candidate");
    }

    #[test]
    fn actual_published_source_survives_a_typed_failed_parent_readback() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let parent = fixture.path().join("source-parent");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("wiki.json");
        fs::write(&target, b"original source").unwrap();
        let retained = fixture.path().join("retained-parent");
        let retained_for_observer = retained.clone();
        after_publication(move |path| {
            let parent = path.parent().unwrap();
            fs::rename(parent, &retained_for_observer).unwrap();
            fs::create_dir(parent).unwrap();
            fs::write(path, b"foreign source at replacement parent").unwrap();
        });
        let publication = Publication::acquire(&target).unwrap();
        let error = publication
            .replace(b"actual published candidate")
            .unwrap_err();
        let failure = uncertainty(&error).expect("native effect facts must remain downcastable");
        assert!(failure.published);
        assert_eq!(
            failure.source_path,
            fs::canonicalize(&parent).unwrap().join("wiki.json")
        );
        assert!(failure
            .cause
            .to_string()
            .contains("parent identity changed"));
        assert_eq!(
            fs::read(retained.join("wiki.json")).unwrap(),
            b"actual published candidate"
        );
        assert_eq!(
            fs::read(&target).unwrap(),
            b"foreign source at replacement parent"
        );
        drop(publication);
        assert_eq!(
            fs::read(retained.join("wiki.json")).unwrap(),
            b"actual published candidate"
        );
    }

    #[test]
    fn actual_published_permission_change_returns_uncertainty_without_undoing_source() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let target = fixture.path().join("wiki.json");
        fs::write(&target, b"original source").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        after_publication(|source| {
            fs::set_permissions(source, fs::Permissions::from_mode(0o777)).unwrap();
        });
        let owner = Publication::acquire(&target).unwrap();
        let error = owner.replace(b"actually published source").unwrap_err();
        let failure = uncertainty(&error).unwrap();
        assert!(failure.published);
        assert_eq!(failure.cause.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&target).unwrap(), b"actually published source");
        assert_eq!(fs::metadata(&target).unwrap().mode() & 0o7777, 0o777);
        assert!(!failure.cause.to_string().contains("source unchanged"));
        drop(owner);
        assert_eq!(fs::read(&target).unwrap(), b"actually published source");
    }

    #[test]
    fn actual_bootstrap_stage_permission_change_refuses_before_candidate_bytes() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let target = fixture.path().join("wiki.json");
        let observed = std::sync::Arc::new(std::sync::Mutex::new(None::<PathBuf>));
        let observed_stage = observed.clone();
        ON_STAGE_CREATED.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |stage| {
                assert_eq!(fs::metadata(stage).unwrap().mode() & 0o077, 0);
                fs::set_permissions(stage, fs::Permissions::from_mode(0o777)).unwrap();
                *observed_stage.lock().unwrap() = Some(stage.to_path_buf());
            }));
        });
        let owner = Publication::acquire(&target).unwrap();
        assert!(owner.bytes().is_none());
        let error = owner
            .create_new(b"private bootstrap candidate")
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(uncertainty(&error).is_none());
        assert!(!target.exists());
        let stage = observed.lock().unwrap().clone().unwrap();
        assert_eq!(fs::read(&stage).unwrap(), b"");
        drop(owner);
        assert!(!target.exists());
        assert_eq!(fs::read(&stage).unwrap(), b"");
    }

    #[test]
    fn actual_bootstrap_published_permission_change_retains_effect_and_uncertainty() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let target = fixture.path().join("wiki.json");
        after_publication(|source| {
            assert_eq!(fs::metadata(source).unwrap().mode() & 0o077, 0);
            fs::set_permissions(source, fs::Permissions::from_mode(0o777)).unwrap();
        });
        let owner = Publication::acquire(&target).unwrap();
        assert!(owner.bytes().is_none());
        let error = owner
            .create_new(b"actually published bootstrap")
            .unwrap_err();
        let failure = uncertainty(&error).expect("an actual bootstrap effect must remain typed");
        assert!(failure.published);
        assert_eq!(
            failure.source_path,
            fs::canonicalize(fixture.path()).unwrap().join("wiki.json")
        );
        assert_eq!(failure.cause.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&target).unwrap(), b"actually published bootstrap");
        assert_eq!(fs::metadata(&target).unwrap().mode() & 0o7777, 0o777);
        drop(owner);
        assert_eq!(fs::read(&target).unwrap(), b"actually published bootstrap");
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn actual_bootstrap_published_xattr_change_retains_effect_and_uncertainty() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let target = fixture.path().join("wiki.json");
        after_publication(|source| {
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(source)
                .unwrap();
            let name = CString::new("user.central-publication-bootstrap-proof").unwrap();
            let value = b"foreign post-publication metadata";
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            };
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            };
            assert_eq!(result, 0, "{}", io::Error::last_os_error());
        });
        let owner = Publication::acquire(&target).unwrap();
        assert!(owner.bytes().is_none());
        let error = owner
            .create_new(b"actually published bootstrap")
            .unwrap_err();
        let failure = uncertainty(&error).expect("metadata drift cannot validate itself");
        assert!(failure.published);
        assert_eq!(
            failure.source_path,
            fs::canonicalize(fixture.path()).unwrap().join("wiki.json")
        );
        assert_eq!(failure.cause.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&target).unwrap(), b"actually published bootstrap");
        assert_eq!(
            extended_attributes(&File::open(&target).unwrap())
                .unwrap()
                .get(b"user.central-publication-bootstrap-proof".as_slice())
                .unwrap()
                .as_slice(),
            b"foreign post-publication metadata"
        );
        drop(owner);
        assert_eq!(fs::read(&target).unwrap(), b"actually published bootstrap");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn actual_bootstrap_published_acl_change_retains_effect_and_uncertainty() {
        struct Reap(Child);
        impl Drop for Reap {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let target = fixture.path().join("wiki.json");
        after_publication(|source| {
            let file = File::open(source).unwrap();
            let before_mode = file.metadata().unwrap().mode();
            let before_acl = native_acl(&file).unwrap();
            let mut child = Reap(
                Command::new("/bin/chmod")
                    .args(["+a", "everyone allow read"])
                    .arg(source)
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            let status = loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                assert!(
                    Instant::now() < deadline,
                    "actual native ACL mutation exceeded five seconds"
                );
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(status.success());
            assert_eq!(
                file.metadata().unwrap().mode(),
                before_mode,
                "this negative control must change ACL privacy independently of mode"
            );
            assert_ne!(native_acl(&file).unwrap(), before_acl);
        });
        let owner = Publication::acquire(&target).unwrap();
        assert!(owner.bytes().is_none());
        let error = owner
            .create_new(b"actually published bootstrap")
            .unwrap_err();
        let failure = uncertainty(&error).expect("bootstrap ACL drift cannot validate itself");
        assert!(failure.published);
        assert_eq!(
            failure.source_path,
            fs::canonicalize(fixture.path()).unwrap().join("wiki.json")
        );
        assert_eq!(failure.cause.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&target).unwrap(), b"actually published bootstrap");
        drop(owner);
        assert_eq!(fs::read(&target).unwrap(), b"actually published bootstrap");
    }

    #[test]
    fn native_source_lock_stage_and_directory_do_not_cross_actual_exec() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        struct Reap(Child);
        impl Drop for Reap {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ProjectCentral/now/tmp")
            .join(format!(
                "native-publication-exec-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&scratch).unwrap();
        let fixture = Fixture(scratch);
        let path = fixture.0.join("source.json");
        fs::write(&path, b"retained physical source").unwrap();
        let publication = Publication::acquire(&path).unwrap();
        let stage = open_in(
            &publication.parent,
            OsStr::new("owned-stage.tmp"),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        )
        .unwrap();
        let control_path = fixture.0.join("deliberately-inheritable-control");
        fs::write(&control_path, b"private regression control").unwrap();
        let control = File::open(&control_path).unwrap();
        let flags = unsafe { libc::fcntl(control.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe {
                libc::fcntl(
                    control.as_raw_fd(),
                    libc::F_SETFD,
                    flags & !libc::FD_CLOEXEC,
                )
            },
            0
        );
        let observations = [
            receipt(&publication.parent),
            receipt(&publication.lock),
            receipt(&publication.current.as_ref().unwrap().file),
            receipt(&stage),
        ]
        .join(";");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "wiki_publication::tests::exec_inheritance_child",
                "--nocapture",
            ])
            .env(EXEC_OBSERVATIONS, observations)
            .env(EXEC_CONTROL, receipt(&control));
        // A pre_exec hook selects the fork/exec route on macOS, rather than a
        // spawn mode that closes every undesignated fd independently of these
        // owner flags. The positive control proves this actual route admits an
        // inheritable descriptor; the native descriptors must still not leak.
        unsafe {
            command.pre_exec(|| Ok(()));
        }
        let mut child = Reap(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.0.kill().unwrap();
                let _ = child.0.wait();
                panic!("actual exec inheritance child exceeded five-second bound");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "native descriptors must close across actual exec"
        );
        drop(publication);
        // The exact same stable lock inode remains usable after child exit.
        Publication::acquire(&path).unwrap();
    }
}
