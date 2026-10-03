//! Ordinary-file owner CAS and recovery. Participating sources never enter this
//! route. History is owner-kept recovery material, not authored ground or git.
use crate::{
    action::*,
    files::{participating_source, read_file, CentralPathRef},
    result::{ActionResult, ResultStatus},
    root::resolve_central_root,
    source_safety::{content_revision_bytes, reject_symlink_components},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    io::{AsRawFd, FromRawFd},
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
pub(crate) const MAX: usize = 4 * 1024 * 1024;
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn conflict(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, message)
}
fn open(path: &Path, write: bool) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}
fn bytes(file: &mut File) -> io::Result<Vec<u8>> {
    let mut b = Vec::new();
    file.take((MAX + 1) as u64).read_to_end(&mut b)?;
    if b.len() > MAX {
        return Err(invalid("File exceeds bounded text size"));
    }
    Ok(b)
}
pub(crate) fn directory(root: &Path, relative: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(root)?;
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err(invalid("Invalid parent component"));
        }
        let name = std::ffi::CString::new(part.as_os_str().as_encoded_bytes())
            .map_err(io::Error::other)?;
        let fd = unsafe {
            libc::openat(
                file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        file = unsafe { File::from_raw_fd(fd) };
    }
    Ok(file)
}
pub(crate) fn open_native_file(root: &Path, relative: &str) -> io::Result<File> {
    let path = Path::new(relative);
    let parent = directory(
        root,
        path.parent().ok_or_else(|| invalid("File has no parent"))?,
    )?;
    let name = std::ffi::CString::new(
        path.file_name()
            .ok_or_else(|| invalid("File has no name"))?
            .as_encoded_bytes(),
    )
    .map_err(io::Error::other)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// Read-only material capture through the existing native descriptors. The
/// caller retains any original root alias and its treatment/owner relation.
pub(crate) struct NativeFileRead {
    file: File,
    root: File,
    parent: File,
    canonical_root: PathBuf,
    relative: PathBuf,
    admitted: fs::Metadata,
}

impl NativeFileRead {
    pub(crate) fn open(canonical_root: &Path, expected_root: (u64, u64), relative: &Path) -> io::Result<Self> {
        let relative_text = relative.to_str().ok_or_else(|| invalid("Read member is not UTF-8"))?;
        if relative.components().count() == 0 || !relative.components().all(|part|
            matches!(part, std::path::Component::Normal(_)))
        {
            return Err(invalid("Read requires a normal relative member"));
        }
        let root = directory(canonical_root, Path::new(""))?;
        let root_metadata = root.metadata()?;
        if (root_metadata.dev(), root_metadata.ino()) != expected_root {
            return Err(io::Error::other("Native read owner root changed before material capture"));
        }
        let parent = directory(canonical_root, relative.parent().ok_or_else(|| invalid("Read has no parent"))?)?;
        let file = open_native_file(canonical_root, relative_text)?;
        let admitted = file.metadata()?;
        if !admitted.is_file() {
            return Err(invalid("Read requires a regular file"));
        }
        let reading = Self { file, root, parent, canonical_root: canonical_root.to_path_buf(), relative: relative.to_path_buf(), admitted };
        reading.validate()?;
        Ok(reading)
    }

    pub(crate) fn validate(&self) -> io::Result<()> {
        let root = directory(&self.canonical_root, Path::new(""))?.metadata()?;
        let held_root = self.root.metadata()?;
        let parent = directory(&self.canonical_root, self.relative.parent().ok_or_else(|| invalid("Read has no parent"))?)?.metadata()?;
        let held_parent = self.parent.metadata()?;
        let named = fs::symlink_metadata(self.canonical_root.join(&self.relative))?;
        let held = self.file.metadata()?;
        if root.dev() != held_root.dev() || root.ino() != held_root.ino()
            || parent.dev() != held_parent.dev() || parent.ino() != held_parent.ino()
            || !named.is_file() || named.dev() != self.admitted.dev() || named.ino() != self.admitted.ino()
            || held.dev() != self.admitted.dev() || held.ino() != self.admitted.ino()
            || named.len() != self.admitted.len() || held.len() != self.admitted.len()
            || named.mtime() != self.admitted.mtime() || named.mtime_nsec() != self.admitted.mtime_nsec()
            || held.mtime() != self.admitted.mtime() || held.mtime_nsec() != self.admitted.mtime_nsec()
        {
            return Err(io::Error::other("Native read material affiliation or content basis changed"));
        }
        Ok(())
    }

    pub(crate) fn read_bytes(&mut self, limit: usize) -> io::Result<Vec<u8>> {
        if limit == 0 || limit > MAX {
            return Err(invalid("Native read capacity must be within the existing 4 MiB bound"));
        }
        self.read_bytes_with_capacity(limit, "Native source exceeds the 4 MiB eager read capacity")
    }

    /// Owner declarations retain the existing file-map 8 MiB metadata capacity.
    /// This does not widen ordinary Source delivery or change material capture.
    pub(crate) fn read_metadata_bytes(&mut self, limit: usize) -> io::Result<Vec<u8>> {
        if limit == 0 || limit > 8 * 1024 * 1024 {
            return Err(invalid("Native metadata capacity must be within the existing 8 MiB bound"));
        }
        self.read_bytes_with_capacity(limit, "Native declaration exceeds the 8 MiB metadata read capacity")
    }

    fn read_bytes_with_capacity(&mut self, limit: usize, overflow: &'static str) -> io::Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        let read = |file: &mut File| -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(io::Error::new(io::ErrorKind::InvalidData, overflow));
            }
            Ok(bytes)
        };
        self.validate()?;
        let bytes = read(&mut self.file)?;
        self.validate()?;
        self.file.seek(SeekFrom::Start(0))?;
        let current = read(&mut self.file)?;
        self.validate()?;
        if current != bytes {
            return Err(io::Error::other("Native source bytes changed during read"));
        }
        Ok(bytes)
    }
}
pub(crate) fn rename_in(parent: &File, from: &str, to: &str) -> io::Result<()> {
    let from = std::ffi::CString::new(from).map_err(io::Error::other)?;
    let to = std::ffi::CString::new(to).map_err(io::Error::other)?;
    if unsafe {
        libc::renameat(
            parent.as_raw_fd(),
            from.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) fn create_in(parent: &File, name: &str, mode: u32) -> io::Result<File> {
    let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(crate) fn ordinary_address(root: &Path, loc: &CentralPathRef) -> io::Result<PathBuf> {
    let path = loc.resolve(root)?;
    ordinary_policy(root, loc)?;
    Ok(path)
}
fn ordinary_policy(root: &Path, loc: &CentralPathRef) -> io::Result<()> {
    let components: Vec<_> = Path::new(&loc.path)
        .components()
        .map(|p| p.as_os_str().to_string_lossy().into_owned())
        .collect();
    // Control is protected ground with one ratified exception: the
    // user-section flows area (Control/user/flows/) holds the owner's dated,
    // self-contained Flow documents and accepts ordinary attributed writes.
    let user_flows_instance = components.first().is_some_and(|p| p == "Control")
        && components.get(1).is_some_and(|p| p == "user")
        && components.get(2).is_some_and(|p| p == "flows");
    if components.first().is_some_and(|p| p == "Control") && !user_flows_instance
        || components
            .iter()
            .any(|p| matches!(p.as_str(), "ProjectCentral" | ".central" | ".git"))
    {
        return Err(denied("Protected ground or owner state must use its native authored operation; ordinary file writes are unavailable"));
    }
    // Do not turn malformed participating-project ground into an ordinary file
    // simply because the discovery convenience reader returns no binding.
    if components.len() > 2 && components[0] == "Work" {
        let project = root.join("Work").join(&components[1]);
        if project.join("ProjectCentral").exists() {
            let manifest = crate::projectcentral::read_project_manifest(&project)?;
            if !manifest.validate().valid {
                return Err(denied("Project identity is invalid; ordinary write cannot infer absence of source participation"));
            }
            crate::source_horizon::project_source_bindings(&project)?;
        }
    }
    if participating_source(root, &loc.path).is_some() {
        return Err(denied("Participating SourceRef must use projectcentral.source operations; filesystem identity cannot bypass source authority"));
    }
    Ok(())
}
fn ordinary(root: &Path, loc: &CentralPathRef) -> io::Result<PathBuf> {
    let path = ordinary_address(root, loc)?;
    let reading = read_file(root, loc, crate::files::FileEncoding::Base64)?;
    if reading.source.is_some() {
        return Err(denied("Participating source requires source authority"));
    }
    Ok(path)
}
struct Lock {
    file: File,
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn state(root: &Path) -> io::Result<(PathBuf, Lock)> {
    let dir = root.join(".central/file-history");
    for p in [root.join(".central"), dir.clone()] {
        match fs::create_dir(&p) {
            Ok(()) => fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        };
        if !fs::symlink_metadata(&p)?.file_type().is_dir() {
            return Err(denied("Owner history directory redirected"));
        }
    }
    reject_symlink_components(root, Path::new(".central/file-history"))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((dir, Lock { file }))
}
fn key(text: &[u8]) -> String {
    content_revision_bytes(text).replace([':', '/'], "_")
}
/// The semantic owner decides whether this is a first identity or an update.
/// This is a physical publication choice, never a Source identity or lock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordDisposition {
    CreateNew,
    ReplaceOrCreate,
}

struct RecordDirectory {
    requested_root: PathBuf,
    canonical_root: PathBuf,
    relative_parent: PathBuf,
    root: File,
    parent: File,
}
impl RecordDirectory {
    fn capture(root: &Path, relative: &Path) -> io::Result<Self> {
        if relative.components().count() == 0
            || !relative.components().all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(invalid("Record requires a normal relative member"));
        }
        let requested_root = if root.is_absolute() { root.to_path_buf() } else { std::env::current_dir()?.join(root) };
        let canonical_root = fs::canonicalize(&requested_root)?;
        let held_root = directory(&canonical_root, Path::new(""))?;
        let relative_parent = relative.parent().ok_or_else(|| invalid("Record has no parent"))?.to_path_buf();
        let parent = Self::walk(&held_root, &relative_parent)?;
        let context = Self { requested_root, canonical_root, relative_parent, root: held_root, parent };
        context.validate()?;
        Ok(context)
    }
    fn walk(root: &File, relative: &Path) -> io::Result<File> {
        let mut current = root.try_clone()?;
        for part in relative.components() {
            if !matches!(part, std::path::Component::Normal(_)) {
                return Err(invalid("Record has an invalid parent component"));
            }
            let name = std::ffi::CString::new(part.as_os_str().as_encoded_bytes()).map_err(io::Error::other)?;
            let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            if fd < 0 { return Err(io::Error::last_os_error()); }
            current = unsafe { File::from_raw_fd(fd) };
        }
        Ok(current)
    }
    fn validate(&self) -> io::Result<()> {
        let original = fs::canonicalize(&self.requested_root)?;
        let requested = directory(&original, Path::new(""))?;
        let canonical = directory(&self.canonical_root, Path::new(""))?;
        let held = self.root.metadata()?;
        if !same_record_inode(&held, &requested.metadata()?) || !same_record_inode(&held, &canonical.metadata()?) {
            return Err(conflict("Native record owner root affiliation changed"));
        }
        let current_parent = Self::walk(&canonical, &self.relative_parent)?;
        if !same_record_inode(&self.parent.metadata()?, &current_parent.metadata()?) {
            return Err(conflict("Native record parent affiliation changed"));
        }
        Ok(())
    }
    fn path(&self, name: &std::ffi::OsStr) -> PathBuf {
        self.canonical_root.join(&self.relative_parent).join(name)
    }
}
fn same_record_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}
fn ordinary_record(metadata: &fs::Metadata) -> io::Result<()> {
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(denied("Native record requires a regular inode with one physical link"));
    }
    Ok(())
}
fn record_open_in(parent: &File, name: &std::ffi::OsStr, flags: libc::c_int) -> io::Result<File> {
    let name = std::ffi::CString::new(name.as_encoded_bytes()).map_err(io::Error::other)?;
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(),
        flags | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC, 0o600) };
    if fd < 0 { return Err(io::Error::last_os_error()); }
    Ok(unsafe { File::from_raw_fd(fd) })
}
struct RecordBasis {
    file: File,
    metadata: fs::Metadata,
    privacy: crate::wiki_publication::Privacy,
}
impl RecordBasis {
    fn capture(parent: &File, name: &std::ffi::OsStr) -> io::Result<Option<Self>> {
        let file = match record_open_in(parent, name, libc::O_RDONLY) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        ordinary_record(&metadata)?;
        let privacy = crate::wiki_publication::Privacy::read(&file)?;
        let result = Self { file, metadata, privacy };
        result.validate(parent, name)?;
        Ok(Some(result))
    }
    fn validate(&self, parent: &File, name: &std::ffi::OsStr) -> io::Result<()> {
        let current = record_open_in(parent, name, libc::O_RDONLY)?;
        let named = current.metadata()?;
        let held = self.file.metadata()?;
        ordinary_record(&named)?;
        ordinary_record(&held)?;
        for metadata in [&named, &held] {
            if !same_record_inode(&self.metadata, metadata)
                || self.metadata.len() != metadata.len()
                || self.metadata.ctime() != metadata.ctime()
                || self.metadata.ctime_nsec() != metadata.ctime_nsec()
                || self.metadata.mtime() != metadata.mtime()
                || self.metadata.mtime_nsec() != metadata.mtime_nsec()
            {
                return Err(conflict("Native record basis changed before publication"));
            }
        }
        self.privacy.verify(&current)?;
        self.privacy.verify(&self.file)
    }
}

// Keep the existing Wiki physical error and actual IO cause, while identifying
// this physical producer by type, never by pathname/message parsing.
struct RecordPublicationUncertain {
    publication: crate::wiki_publication::PublicationUncertain,
}
impl std::fmt::Debug for RecordPublicationUncertain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RecordPublicationUncertain")
            .field("published", &self.publication.published)
            .field("cause_kind", &self.publication.cause.kind())
            .field("cause_raw_os_error", &self.publication.cause.raw_os_error())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for RecordPublicationUncertain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Native record publication occurred, but its acknowledgement is unconfirmed: {}; inspect the existing owner state before retrying", self.publication.cause)
    }
}
impl std::error::Error for RecordPublicationUncertain {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.publication.cause) }
}
struct RecordStageFailure {
    stage_path: PathBuf,
    cause: io::Error,
}
impl std::fmt::Debug for RecordStageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RecordStageFailure").field("cause_kind", &self.cause.kind())
            .field("cause_raw_os_error", &self.cause.raw_os_error()).finish_non_exhaustive()
    }
}
impl std::fmt::Display for RecordStageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}; native record stage retained at {} (a substituted name is not owned)", self.cause, self.stage_path.display())
    }
}
impl std::error::Error for RecordStageFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.cause) }
}
fn diagnostic_record_text(value: &str) -> Value {
    if value.len() <= 4096 { json!(value) } else { json!({"observation_omitted":"diagnostic_text_profile","byte_len":value.len()}) }
}
pub(crate) fn record_publication_observation(error: &io::Error) -> Option<Value> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    for _ in 0..8 {
        let cause = current?;
        if let Some(io) = cause.downcast_ref::<io::Error>() {
            if let Some(failure) = io.get_ref().and_then(|payload| payload.downcast_ref::<RecordPublicationUncertain>()) {
                let path = failure.publication.source_path.to_string_lossy();
                let mut facts = json!({"material_kind":"native_owner_material","semantic_acceptance":"not_established_by_physical_publication","published":failure.publication.published,
                    "path":diagnostic_record_text(&path), "durability_and_current_state":"unconfirmed",
                    "cause":{"kind":format!("{:?}",failure.publication.cause.kind()),"raw_os_error":failure.publication.cause.raw_os_error()}});
                if !record_observation_fits(&facts) { facts["path"] = json!({"observation_omitted":"record_diagnostic_profile","byte_len":path.len()}); }
                return Some(facts);
            }
        }
        if let Some(io) = cause.downcast_ref::<io::Error>() {
            if let Some(progress) = io.get_ref().and_then(|payload| payload.downcast_ref::<RecordOwnerProgress>()) {
                if let Some(observation) = progress.supplemental.as_ref().and_then(record_publication_observation) {
                    return Some(observation);
                }
            }
        }
        current = cause.source();
    }
    None
}
fn record_observation_fits(value: &Value) -> bool {
    serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 8 * 1024)
}
fn record_cause_facts(error: &io::Error) -> Value {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    let mut sources = Vec::new();
    for _ in 0..8 {
        let Some(cause) = current else { break; };
        if let Some(io) = cause.downcast_ref::<io::Error>() {
            sources.push(json!({"kind":format!("{:?}",io.kind()),"raw_os_error":io.raw_os_error()}));
        }
        current = cause.source();
    }
    json!({"kind":format!("{:?}",error.kind()),"raw_os_error":error.raw_os_error(),
        "sources":sources,"source_chain_truncated":current.is_some()})
}

/// Actual prior owner progress when a supporting record/acknowledgement fails.
/// These scalar observations are invocation evidence, never a new receipt or
/// current-state authority. No request, document Value or body is retained.
struct RecordOwnerProgress {
    phase: &'static str,
    source_ref: Option<String>,
    owner_ref: Option<String>,
    revision: Option<String>,
    source_publication: &'static str,
    cause: io::Error,
    supplemental: Option<io::Error>,
}
impl std::fmt::Debug for RecordOwnerProgress {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RecordOwnerProgress").field("phase", &self.phase)
            .field("source_publication", &self.source_publication)
            .field("cause_kind", &self.cause.kind())
            .field("cause_raw_os_error", &self.cause.raw_os_error())
            .field("supplemental_kind", &self.supplemental.as_ref().map(io::Error::kind))
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for RecordOwnerProgress {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Native owner {} reached {}; completion is unconfirmed: {}; inspect the same native owner state without automatic resend", self.phase, self.source_publication, self.cause)
    }
}
impl std::error::Error for RecordOwnerProgress {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.cause) }
}
pub(crate) fn record_owner_error(cause: io::Error, phase: &'static str, source_ref: Option<&str>, revision: Option<&str>, source_publication: &'static str) -> io::Error {
    io::Error::new(cause.kind(), RecordOwnerProgress { phase, source_ref: source_ref.map(str::to_owned),
        owner_ref: None, revision: revision.map(str::to_owned), source_publication, cause, supplemental: None })
}
pub(crate) fn record_owner_error_with_ref(cause: io::Error, phase: &'static str, owner_ref: &str,
    source_ref: Option<&str>, revision: Option<&str>, source_publication: &'static str) -> io::Error {
    io::Error::new(cause.kind(), RecordOwnerProgress { phase, source_ref: source_ref.map(str::to_owned),
        owner_ref: Some(owner_ref.to_owned()), revision: revision.map(str::to_owned), source_publication,
        cause, supplemental: None })
}
pub(crate) fn record_owner_errors(cause: io::Error, supplemental: io::Error, phase: &'static str,
    source_ref: Option<&str>, revision: Option<&str>, source_publication: &'static str) -> io::Error {
    io::Error::new(cause.kind(), RecordOwnerProgress { phase, source_ref: source_ref.map(str::to_owned),
        owner_ref: None, revision: revision.map(str::to_owned), source_publication, cause, supplemental: Some(supplemental) })
}
pub(crate) fn record_owner_progress(error: &io::Error) -> Option<Value> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    for _ in 0..8 {
        let cause = current?;
        if let Some(io) = cause.downcast_ref::<io::Error>() {
            if let Some(progress) = io.get_ref().and_then(|payload| payload.downcast_ref::<RecordOwnerProgress>()) {
                let mut facts = json!({"phase":progress.phase,"source_publication":progress.source_publication,
                    "source_ref":progress.source_ref.as_deref().map(diagnostic_record_text),
                    "owner_ref":progress.owner_ref.as_deref().map(diagnostic_record_text),
                    "revision_observation":progress.revision.as_deref().map(diagnostic_record_text),
                    "primary_cause":record_cause_facts(&progress.cause),
                    "supplemental_cause":progress.supplemental.as_ref().map(|error| json!({
                        "cause":record_cause_facts(error),"record_publication":record_publication_observation(error)})),
                    "current_state":"use_existing_native_owner_read"});
                if !record_observation_fits(&facts) {
                    facts["source_ref"] = progress.source_ref.as_ref().map(|value| json!({"observation_omitted":"record_diagnostic_profile","byte_len":value.len()})).unwrap_or(Value::Null);
                    facts["revision_observation"] = progress.revision.as_ref().map(|value| json!({"observation_omitted":"record_diagnostic_profile","byte_len":value.len()})).unwrap_or(Value::Null);
                    facts["owner_ref"] = progress.owner_ref.as_ref().map(|value| json!({"observation_omitted":"record_diagnostic_profile","byte_len":value.len()})).unwrap_or(Value::Null);
                }
                // A nested secondary publication can occupy the observation
                // profile itself. Keep its fixed effect/cause, omit only path.
                if !record_observation_fits(&facts) {
                    if let Some(publication) = facts.get_mut("supplemental_cause").and_then(|cause| cause.get_mut("record_publication")).filter(|value| value.is_object()) {
                        publication["path"] = json!({"observation_omitted":"record_diagnostic_profile"});
                    }
                }
                return Some(facts);
            }
        }
        current = cause.source();
    }
    None
}
pub(crate) fn record_failure_result(action: &str, error: &io::Error) -> Option<ActionResult> {
    let publication = record_publication_observation(error);
    let prior = record_owner_progress(error);
    if publication.is_none() && prior.is_none() { return None; }
    Some(ActionResult::failure_coded(Some(action), ResultStatus::PartialCompletion,
        "central.publication_uncertain", error.to_string(), Some(json!({
            "outcome":if publication.is_some() { "unknown" } else { "partial" },
            "record_publication":publication,"prior_owner_observation":prior,
            "automatic_retry":false,"automatic_authority_widening":false,
            "current_state_authority":"existing_native_owner_read_and_recovery",
            "cause":{"kind":format!("{:?}",error.kind()),"raw_os_error":error.raw_os_error()}
        }))))
}

// Every owner keeps its own semantic serialization. This routine owns only
// exact material publication; it never consumes the old record-staging name.
pub(crate) fn atomic_record(root: &Path, relative: &Path, data: &[u8], disposition: RecordDisposition) -> io::Result<()> {
    static NEXT_STAGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let context = RecordDirectory::capture(root, relative)?;
    let name = relative.file_name().ok_or_else(|| invalid("Record has no name"))?;
    let basis = RecordBasis::capture(&context.parent, name)?;
    if disposition == RecordDisposition::CreateNew && basis.is_some() {
        return Err(conflict("Native record first publication already exists"));
    }
    context.validate()?;
    let mut staged = None;
    for _ in 0..16 {
        let nonce = NEXT_STAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
        let stage_name = std::ffi::OsString::from(format!(".central-record-{}-{time}-{nonce}", std::process::id()));
        match record_open_in(&context.parent, &stage_name, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL) {
            Ok(file) => { staged = Some((stage_name, file)); break; }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let (stage_name, mut stage) = staged.ok_or_else(|| conflict("Native record staging collision budget exhausted"))?;
    let mut published = false;
    let result = (|| {
        let stage_metadata = stage.metadata()?;
        ordinary_record(&stage_metadata)?;
        let created_privacy;
        let expected = match &basis {
            Some(basis) => {
                basis.validate(&context.parent, name)?;
                crate::wiki_publication::preserve_file_metadata(&basis.file, &basis.metadata, &stage)?;
                basis.privacy.verify(&stage)?;
                // Retain the immutable admitted source expectation; never
                // admit a changed source by rereading it after metadata copy.
                &basis.privacy
            }
            None => {
                created_privacy = crate::wiki_publication::Privacy::read(&stage)?;
                &created_privacy
            }
        };
        #[cfg(test)]
        record_publication_tests::checkpoint(RecordCheckpoint::BeforeData, &context.path(&stage_name));
        context.validate()?;
        verify_record_stage(&context.parent, &stage_name, &stage, &stage_metadata)?;
        expected.verify(&stage)?;
        if let Some(basis) = &basis { basis.validate(&context.parent, name)?; }
        stage.write_all(data)?;
        if let Some(basis) = &basis { stage.set_permissions(fs::Permissions::from_mode(basis.metadata.mode() & 0o7777))?; }
        expected.verify(&stage)?;
        stage.sync_all()?;
        #[cfg(test)]
        record_publication_tests::checkpoint(RecordCheckpoint::BeforePublish, &context.path(&stage_name));
        context.validate()?;
        verify_record_stage(&context.parent, &stage_name, &stage, &stage_metadata)?;
        expected.verify(&stage)?;
        record_bytes_match(&mut stage, data)?;
        match &basis {
            Some(basis) => basis.validate(&context.parent, name)?,
            None => match record_open_in(&context.parent, name, libc::O_RDONLY) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => return Err(conflict("Native record target appeared before first publication")),
            },
        }
        context.validate()?;
        let stage_c = std::ffi::CString::new(stage_name.as_encoded_bytes()).map_err(io::Error::other)?;
        let target_c = std::ffi::CString::new(name.as_encoded_bytes()).map_err(io::Error::other)?;
        if basis.is_some() {
            if unsafe { libc::renameat(context.parent.as_raw_fd(), stage_c.as_ptr(), context.parent.as_raw_fd(), target_c.as_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
        } else {
            crate::wiki_publication::rename_new(&context.parent, &stage_c, &target_c)?;
        }
        published = true;
        #[cfg(test)]
        record_publication_tests::checkpoint(RecordCheckpoint::AfterPublish, &context.path(name));
        context.parent.sync_all()?;
        context.validate()?;
        let mut reading = record_open_in(&context.parent, name, libc::O_RDONLY)?;
        let actual = reading.metadata()?;
        ordinary_record(&actual)?;
        if !same_record_inode(&actual, &stage.metadata()?) { return Err(conflict("Published native record inode changed")); }
        expected.verify(&reading)?;
        record_bytes_match(&mut reading, data)?;
        record_bytes_match(&mut reading, data)?;
        expected.verify(&reading)?;
        verify_record_stage(&context.parent, name, &reading, &stage_metadata)?;
        context.validate()?;
        Ok(())
    })();
    if published {
        result.map_err(|cause| io::Error::new(cause.kind(), RecordPublicationUncertain {
            publication: crate::wiki_publication::PublicationUncertain { source_path: context.path(name), published: true, cause }
        }))
    } else {
        // Never check-then-unlink: the name may already belong to somebody else.
        result.map_err(|cause| io::Error::new(cause.kind(), RecordStageFailure { stage_path: context.path(&stage_name), cause }))
    }
}
fn verify_record_stage(parent: &File, name: &std::ffi::OsStr, held: &File, admitted: &fs::Metadata) -> io::Result<()> {
    let named = record_open_in(parent, name, libc::O_RDONLY)?;
    let current = held.metadata()?;
    let named = named.metadata()?;
    ordinary_record(&current)?;
    ordinary_record(&named)?;
    if !same_record_inode(admitted, &current) || !same_record_inode(admitted, &named) {
        return Err(conflict("Native record staging/published affiliation changed"));
    }
    Ok(())
}
fn record_bytes_match(file: &mut File, expected: &[u8]) -> io::Result<()> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut offset = 0usize;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        // Read at most the exact admitted candidate plus one byte, without
        // allocating a second body or imposing a smaller domain capacity.
        let remaining = expected.len().saturating_sub(offset);
        let limit = buffer.len().min(remaining.saturating_add(1));
        let count = file.read(&mut buffer[..limit])?;
        if count == 0 { break; }
        if count > remaining || buffer[..count] != expected[offset..offset + count] {
            return Err(conflict("Native record exact byte readback differs"));
        }
        offset += count;
    }
    if offset != expected.len() { return Err(conflict("Native record exact byte readback is short")); }
    Ok(())
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordCheckpoint { BeforeData, BeforePublish, AfterPublish }

fn area(root: &Path, dir: &Path, loc: &CentralPathRef) -> io::Result<PathBuf> {
    let area = dir.join(key(loc.ref_id.as_bytes()));
    match fs::create_dir(&area) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    if !fs::symlink_metadata(&area)?.is_dir()
        || fs::symlink_metadata(&area)?.file_type().is_symlink()
    {
        return Err(denied("History address redirected"));
    }
    let identity = area.join("identity.json");
    let data = serde_json::to_vec(loc)?;
    if identity.exists() {
        if bytes(&mut open(&identity, false)?)? != data {
            return Err(invalid("History identity collision"));
        }
    } else {
        atomic_record(root, identity.strip_prefix(root).map_err(io::Error::other)?, &data, RecordDisposition::CreateNew)?;
    }
    Ok(area)
}
// Existing UTF-8 snapshots remain readable byte-for-byte. Binary snapshots
// retain the same content-revision identity and owner recovery namespace.
fn utf8_encoding() -> String {
    "utf-8".into()
}
fn is_utf8(encoding: &String) -> bool {
    encoding == "utf-8"
}
fn decode_content(content: &str, encoding: &str) -> io::Result<Vec<u8>> {
    let bytes = match encoding {
        "utf-8" if content.len() <= MAX && !content.contains('\0') => content.as_bytes().to_vec(),
        "base64" if content.len() <= MAX.div_ceil(3) * 4 => STANDARD
            .decode(content)
            .map_err(|_| invalid("Content is not canonical base64"))?,
        _ => {
            return Err(invalid(
                "Content must be bounded UTF-8 without NUL or base64",
            ))
        }
    };
    if bytes.len() > MAX {
        return Err(invalid("Content exceeds the native byte limit"));
    }
    Ok(bytes)
}
fn encode_content(bytes: &[u8]) -> (String, String) {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => ("utf-8".into(), text.into()),
        _ => ("base64".into(), STANDARD.encode(bytes)),
    }
}
fn recovery_read(root: &Path, loc: &CentralPathRef) -> io::Result<crate::files::FileReading> {
    let mut reading = read_file(root, loc, crate::files::FileEncoding::Base64)?;
    let bytes = decode_content(&reading.content, &reading.content_encoding)?;
    (reading.content_encoding, reading.content) = encode_content(&bytes);
    if reading.content_encoding == "utf-8" {
        reading.mime_hint = None;
    }
    Ok(reading)
}
#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    revision: String,
    content: String,
    #[serde(default = "utf8_encoding", skip_serializing_if = "is_utf8")]
    content_encoding: String,
}
fn snapshot(root: &Path, area: &Path, content: &str) -> io::Result<String> {
    snapshot_bytes(root, area, content.as_bytes())
}
fn snapshot_bytes(root: &Path, area: &Path, bytes: &[u8]) -> io::Result<String> {
    if bytes.len() > MAX {
        return Err(invalid("History exceeds native byte limit"));
    }
    let revision = content_revision_bytes(bytes);
    let path = area.join(format!("{}.json", key(revision.as_bytes())));
    let (content_encoding, content) = encode_content(bytes);
    let value = Snapshot {
        revision: revision.clone(),
        content,
        content_encoding,
    };
    let data = serde_json::to_vec(&value)?;
    if path.exists() {
        let existing: Snapshot =
            serde_json::from_reader(open(&path, false)?.take((MAX * 6 + 1024) as u64))?;
        if existing.revision != revision
            || decode_content(&existing.content, &existing.content_encoding)? != bytes
        {
            return Err(invalid("History revision collision"));
        }
    } else {
        atomic_record(root, path.strip_prefix(root).map_err(io::Error::other)?, &data, RecordDisposition::CreateNew)?;
    }
    Ok(revision)
}
fn historical(area: &Path, revision: &str) -> io::Result<Vec<u8>> {
    let value: Snapshot = serde_json::from_reader(
        open(
            &area.join(format!("{}.json", key(revision.as_bytes()))),
            false,
        )?
        .take((MAX * 6 + 1024) as u64),
    )?;
    let bytes = decode_content(&value.content, &value.content_encoding)?;
    if value.revision != revision || content_revision_bytes(&bytes) != revision {
        return Err(invalid("History revision is corrupt"));
    }
    Ok(bytes)
}
#[derive(Debug, Serialize, Deserialize)]
struct Change {
    cursor: u64,
    previous_revision: String,
    revision: String,
    actor: String,
    actor_kind: String,
    agent_session_ref: Option<String>,
    restored_from: Option<String>,
}
fn events(area: &Path, limit: usize, before: Option<u64>) -> io::Result<Vec<Change>> {
    let mut selected = BTreeMap::new();
    for entry in fs::read_dir(area)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(id) = name
            .strip_prefix("event-")
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<u64>().ok())
        {
            if before.is_some_and(|b| id >= b) {
                continue;
            }
            selected.insert(id, entry.path());
            if selected.len() > limit {
                selected.pop_first();
            }
        }
    }
    selected
        .into_iter()
        .rev()
        .map(|(cursor, path)| {
            let item: Change = serde_json::from_reader(open(&path, false)?.take(65536))?;
            if item.cursor != cursor {
                return Err(invalid("History cursor mismatch"));
            }
            Ok(item)
        })
        .collect()
}
fn text<'a>(input: &'a Value, name: &str) -> io::Result<&'a str> {
    input
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("{name} must be a string")))
}
fn attribution(input: &Value) -> io::Result<(String, String, Option<String>)> {
    let actor = text(input, "actor")?;
    if actor.trim().is_empty() || actor.len() > 1024 {
        return Err(invalid("actor must be bounded and nonempty"));
    }
    let kind = text(input, "actor_kind")?;
    crate::source_safety::validate_actor_kind(kind)?;
    let session = input
        .get("agent_session_ref")
        .filter(|s| !s.is_null())
        .map(|s| {
            s.as_str()
                .ok_or_else(|| invalid("agent_session_ref must be a string"))
        })
        .transpose()?;
    if session.is_some_and(|s| s.is_empty() || s.len() > 4096) {
        return Err(invalid("agent_session_ref must be nonempty and bounded"));
    }
    if kind == "human" && session.is_some() {
        return Err(invalid("Human attribution cannot carry an agent session"));
    }
    Ok((actor.into(), kind.into(), session.map(str::to_owned)))
}
fn execute(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
    let root = root.canonicalize()?;
    let loc: CentralPathRef =
        serde_json::from_value(input.get("location").cloned().unwrap_or(Value::Null))?;
    // The create door: an absent file under Control/user/flows/ is created
    // by an explicit write with an empty expected revision. resolve()
    // canonicalizes the final component and cannot name an absent file, so
    // the absent case is validated by hand before the ordinary route.
    let absent = op == "write"
        && fs::symlink_metadata(root.join(&loc.path)).is_err()
        && loc.path.starts_with("Control/user/flows/");
    let path = if absent {
        validate_absent_instance_location(&root, &loc)?;
        root.join(&loc.path)
    } else {
        ordinary(&root, &loc)?
    };
    if op == "history"
        && !root
            .join(".central/file-history")
            .join(key(loc.ref_id.as_bytes()))
            .exists()
    {
        return Ok(
            json!({"schema":"central.file-history/v1","location":loc,"current_revision":recovery_read(&root,&loc)?.revision,"entries":[],"next_before":null,"more":false,"automatic_agent_or_model_invocation":false}),
        );
    }
    if op == "recovery_preview"
        && !root
            .join(".central/file-history")
            .join(key(loc.ref_id.as_bytes()))
            .exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No recorded ordinary-file history",
        ));
    }
    let (dir, _lock) = state(&root)?;
    let area = area(&root, &dir, &loc)?;
    if absent {
        return create_user_flow_instance(&root, &loc, &path, input, &area);
    }
    // Revalidate after the cross-process owner lock: another writer may have
    // changed the source or introduced authored participation while waiting.
    ordinary(&root, &loc)?;
    let current = if op == "write" {
        read_file(&root, &loc, crate::files::FileEncoding::Utf8)?
    } else {
        recovery_read(&root, &loc)?
    };
    let current_bytes = decode_content(&current.content, &current.content_encoding)?;
    let pending = area.join("pending.json");
    if pending.exists() {
        let event: Change = serde_json::from_reader(open(&pending, false)?.take(65536))?;
        if current.revision == event.revision {
            fs::rename(&pending, area.join(format!("event-{}.json", event.cursor)))?;
            File::open(&area)?.sync_all()?;
        } else if current.revision == event.previous_revision {
            fs::remove_file(&pending)?;
        } else {
            return Err(io::Error::other("File commit has an unresolved interrupted receipt; current bytes match neither journal basis nor target. Owner recovery is required; do not resend."));
        }
    }
    if op == "history" {
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| invalid("limit must be an integer"))
            })
            .transpose()?
            .unwrap_or(20);
        if limit == 0 || limit > 200 {
            return Err(invalid("limit must be 1..200"));
        }
        let before = input
            .get("before")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| invalid("before must be an integer"))
            })
            .transpose()?;
        let mut page = events(&area, limit as usize + 1, before)?;
        let more = page.len() > limit as usize;
        page.truncate(limit as usize);
        let next = if more {
            page.last().map(|c| c.cursor)
        } else {
            None
        };
        return Ok(
            json!({"schema":"central.file-history/v1","location":loc,"current_revision":current.revision,"entries":page,"next_before":next,"more":more,"automatic_agent_or_model_invocation":false}),
        );
    }
    let basis = text(input, "expected_revision")?;
    if basis != current.revision {
        return Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"conflict","location":loc,"expected_revision":basis,"current":current,"changed":false}),
        );
    }
    let restored = if op == "restore" || op == "recovery_preview" {
        Some(text(input, "revision")?)
    } else {
        None
    };
    let content = match restored {
        Some(revision) => historical(&area, revision)?,
        None => decode_content(text(input, "content")?, "utf-8")?,
    };
    let revision = content_revision_bytes(&content);
    if op == "recovery_preview" {
        let (content_encoding, preview_content) = encode_content(&content);
        return Ok(
            json!({"schema":"central.file-recovery-preview/v1","outcome":"preview","location":loc,"expected_revision":basis,"revision":revision,"content":preview_content,"content_encoding":content_encoding,"current_content":current.content,"current_content_encoding":current.content_encoding,"changed":content!=current_bytes,"automatic_agent_or_model_invocation":false}),
        );
    }
    let (actor, actor_kind, agent_session_ref) = attribution(input)?;
    if content == current_bytes {
        return Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"unchanged","location":loc,"previous_revision":basis,"revision":revision,"changed":false}),
        );
    }
    let mut original = open_native_file(&root, &loc.path)?;
    let meta = original.metadata()?;
    if meta.permissions().readonly() {
        return Err(denied("File permissions are read-only"));
    }
    if meta.nlink() != 1 {
        return Err(denied(
            "Files with multiple hard links require an explicit native operation",
        ));
    }
    if bytes(&mut original)? != current_bytes {
        return Err(conflict("File changed before commit"));
    }
    snapshot_bytes(&root, &area, &current_bytes)?;
    snapshot_bytes(&root, &area, &content)?;
    let relative_parent = Path::new(&loc.path)
        .parent()
        .ok_or_else(|| invalid("Missing parent"))?;
    let parent = directory(&root, relative_parent)?;
    let name = format!(
        ".central-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    let mut staged = create_in(&parent, &name, meta.mode() & 0o777)?;
    let cursor = events(&area, 1, None)?
        .first()
        .map(|e| e.cursor)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| invalid("History cursor exhausted"))?;
    let event = Change {
        cursor,
        previous_revision: basis.into(),
        revision: revision.clone(),
        actor,
        actor_kind,
        agent_session_ref,
        restored_from: restored.map(str::to_owned),
    };
    let mut committed = false;
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
            // COPYFILE_METADATA = ACL | STAT | XATTR. Source data is deliberately
            // excluded: the staged content is the caller's explicit CAS target.
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
        staged.set_permissions(meta.permissions())?;
        staged.write_all(&content)?;
        staged.sync_all()?;
        ordinary(&root, &loc)?;
        let now = open_native_file(&root, &loc.path)?.metadata()?;
        if now.dev() != meta.dev()
            || now.ino() != meta.ino()
            || recovery_read(&root, &loc)?.revision != basis
        {
            return Err(conflict("File changed during commit"));
        }
        let check_parent = directory(&root, relative_parent)?.metadata()?;
        let held_parent = parent.metadata()?;
        if check_parent.dev() != held_parent.dev() || check_parent.ino() != held_parent.ino() {
            return Err(conflict("Parent directory changed during commit"));
        }
        atomic_record(&root, pending.strip_prefix(&root).map_err(io::Error::other)?, &serde_json::to_vec(&event)?, RecordDisposition::ReplaceOrCreate)?;
        // The journal fsync can yield to an external editor. Re-read after it
        // before publishing the staged file; owner writers are already locked.
        let final_meta = open_native_file(&root, &loc.path)?.metadata()?;
        if final_meta.dev() != meta.dev()
            || final_meta.ino() != meta.ino()
            || recovery_read(&root, &loc)?.revision != basis
        {
            fs::remove_file(&pending)?;
            return Err(conflict("File changed while preparing durable receipt"));
        }

        rename_in(
            &parent,
            &name,
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("Invalid filename"))?,
        )?;
        committed = true;
        parent.sync_all()?;
        fs::rename(&pending, area.join(format!("event-{cursor}.json")))?;
        File::open(&area)?.sync_all()?;
        Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"written","location":loc,"previous_revision":basis,"revision":revision,"changed":true,"change":event,"automatic_agent_or_model_invocation":false}),
        )
    })();
    let c_name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    unsafe {
        libc::unlinkat(parent.as_raw_fd(), c_name.as_ptr(), 0);
    }
    if committed {
        result.map_err(|error| record_owner_error(error, "ordinary_file.source_publication", Some(&loc.ref_id), Some(&revision), "published"))
    } else {
        result
    }
}

/// Bounding for an absent flow instance location: relative normal
/// components, no symlink components on the existing prefix, the ref naming
/// exactly this root and path, and the path inside the user-section flows
/// area. `resolve()` cannot run here — it canonicalizes the final component
/// and an absent file has none.
fn validate_absent_instance_location(root: &Path, loc: &CentralPathRef) -> io::Result<()> {
    if !loc.path.starts_with("Control/user/flows/") {
        return Err(denied(
            "An absent ordinary file is created only as a flow instance under Control/user/flows/",
        ));
    }
    let relative = Path::new(&loc.path);
    if relative.is_absolute()
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Central location must contain only relative path components",
        ));
    }
    reject_symlink_components(root, relative)?;
    let expected = format!("central:path:{}:{}", root.display(), loc.path);
    if loc.ref_id != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Central location belongs to another root or an unsupported schema",
        ));
    }
    Ok(())
}

/// An absent ordinary file is created only as a flow instance under
/// `Control/user/flows/` — the user-section home of the ground's dated,
/// self-contained Flow documents. The creation is an explicit CAS event:
/// empty expected revision, bounded bytes, one journal event seeded at the
/// next cursor, staged and renamed like every other commit.
fn create_user_flow_instance(
    root: &Path,
    loc: &CentralPathRef,
    path: &Path,
    input: &Value,
    area: &Path,
) -> io::Result<Value> {
    if !loc.path.starts_with("Control/user/flows/") {
        return Err(denied(
            "An absent ordinary file is created only as a flow instance under Control/user/flows/",
        ));
    }
    if !text(input, "expected_revision")?.is_empty() {
        return Err(conflict(
            "Expected revision names a file that does not exist",
        ));
    }
    let content: String = text(input, "content")?.into();
    if content.len() > MAX || content.contains('\0') {
        return Err(invalid("Content must be bounded UTF-8 text without NUL"));
    }
    let (actor, actor_kind, agent_session_ref) = attribution(input)?;
    let revision = content_revision_bytes(content.as_bytes());
    let relative_parent = Path::new(&loc.path)
        .parent()
        .ok_or_else(|| invalid("Missing parent"))?
        .to_path_buf();
    fs::create_dir_all(root.join(&relative_parent))?;
    let parent = directory(root, &relative_parent)?;
    let name = format!(
        ".central-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    let mut staged = create_in(&parent, &name, 0o644)?;
    staged.write_all(content.as_bytes())?;
    staged.sync_all()?;
    let cursor = events(area, 1, None)?
        .first()
        .map(|e| e.cursor)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| invalid("History cursor exhausted"))?;
    let event = Change {
        cursor,
        previous_revision: String::new(),
        revision: revision.clone(),
        actor,
        actor_kind,
        agent_session_ref,
        restored_from: None,
    };
    if area.join("pending.json").exists() {
        return Err(io::Error::other(
            "Interrupted creation receipt is unresolved; owner recovery is required; do not resend.",
        ));
    }
    let mut committed = false;
    let result = (|| {
        snapshot(root, area, &content)?;
        atomic_record(root, area.join("pending.json").strip_prefix(root).map_err(io::Error::other)?, &serde_json::to_vec(&event)?, RecordDisposition::ReplaceOrCreate)?;
        File::open(area)?.sync_all()?;
        rename_in(
            &parent,
            &name,
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("Invalid filename"))?,
        )?;
        committed = true;
        parent.sync_all()?;
        fs::rename(
            area.join("pending.json"),
            area.join(format!("event-{}.json", event.cursor)),
        )?;
        File::open(area)?.sync_all()?;
        Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"created","location":loc,"revision":revision,"changed":true,"change":event,"automatic_agent_or_model_invocation":false}),
        )
    })();
    let c_name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    unsafe {
        libc::unlinkat(parent.as_raw_fd(), c_name.as_ptr(), 0);
    }
    if committed {
        result.map_err(|error| record_owner_error(error, "ordinary_file.source_publication", Some(&loc.ref_id), Some(&revision), "published"))
    } else {
        result
    }
}
fn action(op: &str, id: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let result = resolve_central_root(context.root_options)
        .map_err(io::Error::other)
        .and_then(|r| execute(&r.path, op, input));
    match result {
        Ok(data) => ActionResult::success(id, data),
        Err(e) if crate::file_mutation::record_failure_result(id, &e).is_some() =>
            crate::file_mutation::record_failure_result(id, &e).expect("matched native record failure"),
        Err(e) => ActionResult::failure(
            Some(id),
            match e.kind() {
                io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
                io::ErrorKind::AlreadyExists | io::ErrorKind::InvalidData => {
                    ResultStatus::VerificationFailure
                }
                _ => ResultStatus::InternalFailure,
            },
            e.to_string(),
            Some(
                json!({"outcome":if e.kind()==io::ErrorKind::PermissionDenied {"refused"}else if e.kind()==io::ErrorKind::AlreadyExists {"conflict"}else{"error"}}),
            ),
        ),
    }
}
pub fn register(registry: &mut ActionRegistry) {
    register_create(registry);
    register_flow_append(registry);
    register_flow_read(registry);
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    for (op, handler) in [
        (
            "write",
            (|_, i, c| action("write", "central.files.write", i, c)) as Handler,
        ),
        (
            "history",
            (|_, i, c| action("history", "central.files.history", i, c)) as Handler,
        ),
        (
            "restore",
            (|_, i, c| action("restore", "central.files.restore", i, c)) as Handler,
        ),
        (
            "recovery_preview",
            (|_, i, c| action("recovery_preview", "central.files.recovery_preview", i, c))
                as Handler,
        ),
    ] {
        let mut inputs = vec![ActionInputDefinition {
            name: "location".into(),
            input_type: "object".into(),
            required: true,
            choices: None,
            selection: None,
        }];
        let fields: Vec<(&str, &str, bool)> = match op {
            "history" => vec![("limit", "integer", false), ("before", "integer", false)],
            "recovery_preview" => vec![
                ("expected_revision", "string", true),
                ("revision", "string", true),
            ],
            _ => vec![
                ("expected_revision", "string", true),
                (
                    if op == "write" { "content" } else { "revision" },
                    "string",
                    true,
                ),
                ("actor", "string", true),
                ("actor_kind", "string", true),
                ("agent_session_ref", "string", false),
            ],
        };
        inputs.extend(
            fields
                .into_iter()
                .map(|(name, kind, required)| ActionInputDefinition {
                    name: name.into(),
                    input_type: kind.into(),
                    required,
                    choices: None,
                    selection: None,
                }),
        );
        registry.register(ActionDescriptor{id:format!("central.files.{op}"),title:format!("Ordinary file {op}"),description:"Native ordinary-file history and CAS. Protected ground and participating SourceRefs must use their authored operations. An absent file is created only as a flow instance under Control/user/flows/ with an empty expected revision. Attribution is declared, not an authentication credential.".into(),inputs,output:ActionOutputDefinition{output_type:format!("central-file-{op}")},mutation_class:if op=="history" || op=="recovery_preview" {MutationClass::ReadOnly}else{MutationClass::LocallyMutating},preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("unique file mutation action");
    }
}

include!("file_creation.rs");
include!("flow_append.rs");

#[cfg(test)]
mod read_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Ground(PathBuf);
    impl Ground {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = scratch.join(format!("native-control-read-{}-{nonce}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("nested")).unwrap();
            fs::write(path.join("nested/source.md"), b"retained actual source").unwrap();
            Self(path)
        }
        fn reader(&self) -> NativeFileRead {
            let metadata = fs::metadata(&self.0).unwrap();
            NativeFileRead::open(&fs::canonicalize(&self.0).unwrap(), (metadata.dev(), metadata.ino()), Path::new("nested/source.md")).unwrap()
        }
    }
    impl Drop for Ground {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn actual_read_retains_bytes_inode_and_accepts_a_fresh_native_replacement() {
        let ground = Ground::new();
        let path = ground.0.join("nested/source.md");
        let before = fs::metadata(&path).unwrap();
        assert_eq!(ground.reader().read_bytes(MAX).unwrap(), b"retained actual source");
        let after = fs::metadata(&path).unwrap();
        assert_eq!((before.dev(), before.ino(), before.mtime(), before.mtime_nsec()), (after.dev(), after.ino(), after.mtime(), after.mtime_nsec()));
        let mut stale = ground.reader();
        fs::rename(&path, ground.0.join("retained.md")).unwrap();
        fs::write(&path, b"fresh actual source").unwrap();
        assert!(stale.read_bytes(MAX).is_err());
        assert_eq!(ground.reader().read_bytes(MAX).unwrap(), b"fresh actual source");
        assert_eq!(fs::read(ground.0.join("retained.md")).unwrap(), b"retained actual source");
    }

    #[test]
    fn actual_parent_replacement_final_symlink_and_fifo_cannot_redirect_reading() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let mut reader = ground.reader();
        fs::rename(ground.0.join("nested"), ground.0.join("retained")).unwrap();
        fs::create_dir(ground.0.join("nested")).unwrap();
        fs::write(ground.0.join("nested/source.md"), b"other source").unwrap();
        assert!(reader.read_bytes(MAX).is_err());
        fs::remove_file(ground.0.join("nested/source.md")).unwrap();
        symlink(ground.0.join("retained/source.md"), ground.0.join("nested/source.md")).unwrap();
        let root_metadata = fs::metadata(&ground.0).unwrap();
        let identity = (root_metadata.dev(), root_metadata.ino());
        let error = NativeFileRead::open(&ground.0, identity, Path::new("nested/source.md")).err().unwrap();
        assert_eq!(error.raw_os_error(), Some(libc::ELOOP));
        fs::remove_file(ground.0.join("nested/source.md")).unwrap();
        let fifo = std::ffi::CString::new(ground.0.join("nested/source.md").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        assert_eq!(NativeFileRead::open(&ground.0, identity, Path::new("nested/source.md")).err().unwrap().kind(), io::ErrorKind::InvalidInput);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(fs::read(ground.0.join("retained/source.md")).unwrap(), b"retained actual source");
    }

    #[test]
    fn actual_replaced_owner_root_refuses_before_material_capture_under_old_affiliation() {
        let ground = Ground::new();
        let root = ground.0.join("owner");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.md"), b"original owner source").unwrap();
        let metadata = fs::metadata(&root).unwrap();
        let identity = (metadata.dev(), metadata.ino());
        let retained = ground.0.join("retained-owner");
        fs::rename(&root, &retained).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.md"), b"another owner source").unwrap();
        assert_eq!(NativeFileRead::open(&root, identity, Path::new("source.md")).err().unwrap().kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(retained.join("source.md")).unwrap(), b"original owner source");
    }

    #[test]
    fn actual_read_descriptors_are_not_inherited_by_exec_probe() {
        let Ok(inventory) = std::env::var("CENTRAL_READ_FD_PROBE") else { return; };
        for entry in inventory.split(',') {
            let fields: Vec<_> = entry.split(':').collect();
            let fd: i32 = fields[0].parse().unwrap();
            let dev: u64 = fields[1].parse().unwrap();
            let ino: u64 = fields[2].parse().unwrap();
            let mut observed = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe { libc::fstat(fd, observed.as_mut_ptr()) } == -1 {
                assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
            } else {
                let observed = unsafe { observed.assume_init() };
                assert_ne!((observed.st_dev as u64, observed.st_ino as u64), (dev, ino), "actual native read descriptor survived exec");
            }
        }
    }

    #[test]
    fn actual_exec_does_not_inherit_held_native_root_parent_or_file() {
        use std::process::{Command, Stdio};
        let ground = Ground::new();
        let reader = ground.reader();
        let inventory = [&reader.root, &reader.parent, &reader.file].into_iter().map(|file| {
            let metadata = file.metadata().unwrap();
            format!("{}:{}:{}", file.as_raw_fd(), metadata.dev(), metadata.ino())
        }).collect::<Vec<_>>().join(",");
        let diagnostic = ground.0.join("exec-probe.log");
        let log = File::create(&diagnostic).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "file_mutation::read_tests::actual_read_descriptors_are_not_inherited_by_exec_probe", "--nocapture"])
            .env("CENTRAL_READ_FD_PROBE", inventory).stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().unwrap())).stderr(Stdio::from(log)).spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() { break status; }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let reap = std::time::Instant::now() + std::time::Duration::from_secs(1);
                while child.try_wait().unwrap().is_none() && std::time::Instant::now() < reap {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                panic!("actual native descriptor exec probe exceeded its bound");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let mut log = File::open(&diagnostic).unwrap();
        let mut output = String::new();
        (&mut log).take(MAX as u64).read_to_string(&mut output).unwrap();
        assert!(status.success(), "actual exec probe failed: {output}");
        assert!(output.contains("1 passed"), "probe did not execute actual descriptor check: {output}");
        assert_eq!(reader.file.metadata().unwrap().ino(), fs::metadata(ground.0.join("nested/source.md")).unwrap().ino());
    }
}

#[cfg(test)]
mod record_publication_tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::symlink;
    use std::sync::{Arc, Barrier};
    type PhysicalHook = Box<dyn FnOnce(&Path)>;
    thread_local! {
        static HOOK: RefCell<Option<(RecordCheckpoint, PhysicalHook)>> = RefCell::new(None);
    }
    pub(super) fn checkpoint(at: RecordCheckpoint, path: &Path) {
        let hook = HOOK.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|(expected, _)| *expected == at) { slot.take() } else { None }
        });
        if let Some((_, hook)) = hook { hook(path); }
    }
    fn at(checkpoint: RecordCheckpoint, hook: impl FnOnce(&Path) + 'static) {
        HOOK.with(|slot| assert!(slot.replace(Some((checkpoint, Box::new(hook)))).is_none()));
    }
    struct Fixture { outer: PathBuf, root: PathBuf, inode: (u64, u64) }
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let mut admitted = None;
            for _ in 0..16 {
                let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
                let path = scratch.join(format!("record-native-{}-{now}-{nonce}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => { admitted = Some(path); break; }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("Owned fixture allocation failed: {error}"),
                }
            }
            let outer = admitted.expect("fixture allocation collision budget exhausted");
            let metadata = fs::symlink_metadata(&outer).unwrap();
            let root = outer.join("Central");
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("records")).unwrap();
            Self { outer, root, inode: (metadata.dev(), metadata.ino()) }
        }
        fn publish(&self, data: &[u8], disposition: RecordDisposition) -> io::Result<()> {
            atomic_record(&self.root, Path::new("records/record.json"), data, disposition)
        }
        fn target(&self) -> PathBuf { self.root.join("records/record.json") }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            HOOK.with(|slot| { slot.borrow_mut().take(); });
            let cleanup = (|| -> io::Result<()> {
                let metadata = fs::symlink_metadata(&self.outer)?;
                if !metadata.is_dir() || (metadata.dev(), metadata.ino()) != self.inode {
                    return Err(io::Error::other("Owned fixture affiliation changed; foreign material retained"));
                }
                fs::remove_dir_all(&self.outer)
            })();
            if let Err(error) = cleanup {
                if std::thread::panicking() { eprintln!("Owned fixture cleanup failed at {}: {error}", self.outer.display()); }
                else { panic!("Owned fixture cleanup failed at {}: {error}", self.outer.display()); }
            }
        }
    }
    struct RestoreMode { path: PathBuf, mode: u32, inode: (u64, u64) }
    impl RestoreMode {
        fn new(path: PathBuf) -> Self {
            let meta = fs::symlink_metadata(&path).unwrap();
            Self { path, mode: meta.mode() & 0o7777, inode: (meta.dev(), meta.ino()) }
        }
    }
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            let result = (|| -> io::Result<()> {
                let meta = fs::symlink_metadata(&self.path)?;
                if (meta.dev(), meta.ino()) != self.inode { return Err(io::Error::other("Permission restore target changed")); }
                fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode))
            })();
            if let Err(error) = result {
                if std::thread::panicking() { eprintln!("Owned permission restore failed at {}: {error}", self.path.display()); }
                else { panic!("Owned permission restore failed at {}: {error}", self.path.display()); }
            }
        }
    }
    #[test]
    fn legacy_hardlink_and_symlink_stage_are_retained_without_touching_sentinels() {
        for link in [false, true] {
            let fixture = Fixture::new();
            let sentinel = fixture.outer.join("unselected"); fs::write(&sentinel, b"original sentinel").unwrap();
            let legacy = fixture.root.join("records/record-staging");
            if link { symlink(&sentinel, &legacy).unwrap(); } else { fs::hard_link(&sentinel, &legacy).unwrap(); }
            let before = fs::symlink_metadata(&legacy).unwrap();
            fixture.publish(b"{\"owner\":\"new\"}", RecordDisposition::CreateNew).unwrap();
            let after = fs::symlink_metadata(&legacy).unwrap();
            assert!(same_record_inode(&before, &after));
            assert_eq!(fs::read(&sentinel).unwrap(), b"original sentinel");
            assert_eq!(fs::read(fixture.target()).unwrap(), b"{\"owner\":\"new\"}");
        }
    }
    #[test]
    fn replacement_preserves_operational_readonly_mode_owner_and_bootstrap_privacy() {
        let fixture = Fixture::new();
        fixture.publish(b"old", RecordDisposition::CreateNew).unwrap();
        fs::set_permissions(fixture.target(), fs::Permissions::from_mode(0o440)).unwrap();
        let before = File::open(fixture.target()).unwrap();
        let metadata = before.metadata().unwrap();
        let privacy = crate::wiki_publication::Privacy::read(&before).unwrap();
        fixture.publish(b"next", RecordDisposition::ReplaceOrCreate).unwrap();
        let after = File::open(fixture.target()).unwrap();
        privacy.verify(&after).unwrap();
        assert_eq!(after.metadata().unwrap().mode() & 0o7777, 0o440);
        assert_eq!((metadata.uid(), metadata.gid()), (after.metadata().unwrap().uid(), after.metadata().unwrap().gid()));
        assert_eq!(fs::read(fixture.target()).unwrap(), b"next");
        assert_ne!(metadata.ino(), after.metadata().unwrap().ino());
    }
    #[test]
    fn admitted_stage_privacy_cannot_change_before_candidate_bytes() {
        let fixture = Fixture::new();
        at(RecordCheckpoint::BeforeData, |stage| fs::set_permissions(stage, fs::Permissions::from_mode(0o644)).unwrap());
        let error = fixture.publish(b"secret candidate", RecordDisposition::CreateNew).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(record_publication_observation(&error).is_none());
        assert!(!fixture.target().exists());
        let stages: Vec<_> = fs::read_dir(fixture.root.join("records")).unwrap().map(|entry| entry.unwrap().path()).collect();
        assert_eq!(stages.len(), 1);
        assert!(fs::read(&stages[0]).unwrap().is_empty(), "refuse before writing private candidate bytes");
    }
    #[test]
    fn actual_stage_replacement_survives_owner_error_and_drop() {
        for linked in [false, true] {
            let fixture = Fixture::new();
            let foreign = fixture.outer.join("foreign"); fs::write(&foreign, b"foreign retained").unwrap();
            let foreign_clone = foreign.clone();
            at(RecordCheckpoint::BeforePublish, move |stage| {
                fs::rename(stage, stage.with_extension("owned-retained")).unwrap();
                if linked { symlink(&foreign_clone, stage).unwrap(); } else { fs::write(stage, b"replacement name").unwrap(); }
            });
            let error = fixture.publish(b"owned staged bytes", RecordDisposition::CreateNew).unwrap_err();
            assert!(record_publication_observation(&error).is_none());
            assert!(!fixture.target().exists());
            assert_eq!(fs::read(&foreign).unwrap(), b"foreign retained");
            let entries: Vec<_> = fs::read_dir(fixture.root.join("records")).unwrap().map(|entry| entry.unwrap().path()).collect();
            assert_eq!(entries.len(), 2, "both owned stage and foreign replacement are retained after writer lifetime");
            assert!(entries.iter().any(|entry| fs::read(entry).unwrap() == b"owned staged bytes"));
            assert!(entries.iter().any(|entry| fs::read(entry).unwrap() == if linked { b"foreign retained".as_slice() } else { b"replacement name".as_slice() }));
        }
    }
    #[test]
    fn parent_replacement_before_publication_cannot_redirect_effect() {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        at(RecordCheckpoint::BeforePublish, move |_| {
            fs::rename(root.join("records"), root.join("retained-records")).unwrap();
            fs::create_dir(root.join("records")).unwrap();
            fs::write(root.join("records/sentinel"), b"different parent").unwrap();
        });
        let error = fixture.publish(b"private candidate", RecordDisposition::CreateNew).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(record_publication_observation(&error).is_none());
        assert!(!fixture.target().exists());
        assert_eq!(fs::read(fixture.root.join("records/sentinel")).unwrap(), b"different parent");
        assert_eq!(fs::read_dir(fixture.root.join("retained-records")).unwrap().count(), 1);
    }
    #[test]
    fn unchanged_root_alias_is_useful_and_retargeted_alias_refuses_before_effect() {
        let fixture = Fixture::new();
        let other = Fixture::new();
        let alias = fixture.outer.join("alias"); symlink(&fixture.root, &alias).unwrap();
        atomic_record(&alias, Path::new("records/record.json"), b"old", RecordDisposition::CreateNew).unwrap();
        let alias_clone = alias.clone(); let other_root = other.root.clone();
        at(RecordCheckpoint::BeforePublish, move |_| { fs::remove_file(&alias_clone).unwrap(); symlink(other_root, &alias_clone).unwrap(); });
        let error = atomic_record(&alias, Path::new("records/record.json"), b"next", RecordDisposition::ReplaceOrCreate).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(record_publication_observation(&error).is_none());
        assert_eq!(fs::read(fixture.target()).unwrap(), b"old"); assert!(!other.target().exists());
    }
    #[test]
    fn first_publication_never_overwrites_a_target_created_at_commit_checkpoint() {
        let fixture = Fixture::new(); let target = fixture.target();
        at(RecordCheckpoint::BeforePublish, move |_| fs::write(&target, b"concurrent native identity").unwrap());
        let error = fixture.publish(b"different identity", RecordDisposition::CreateNew).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(fixture.target()).unwrap(), b"concurrent native identity");
        assert!(record_publication_observation(&error).is_none());
    }
    #[test]
    fn late_owner_parent_loss_retains_actual_publication_and_original_enoent() {
        let fixture = Fixture::new(); let root = fixture.root.clone();
        at(RecordCheckpoint::AfterPublish, move |_| fs::rename(root.join("records"), root.join("retained-records")).unwrap());
        let error = fixture.publish(b"actually published", RecordDisposition::CreateNew).unwrap_err();
        let payload = error.get_ref().unwrap().downcast_ref::<RecordPublicationUncertain>().unwrap();
        let oracle = RecordDirectory::walk(&directory(&fixture.root, Path::new("")).unwrap(), Path::new("records")).err().unwrap();
        assert_eq!(payload.publication.cause.kind(), oracle.kind());
        assert_eq!(payload.publication.cause.raw_os_error(), oracle.raw_os_error());
        assert_eq!(record_publication_observation(&error).unwrap()["published"], true);
        assert_eq!(fs::read(fixture.root.join("retained-records/record.json")).unwrap(), b"actually published");
        let result = record_failure_result("native.record.fixture", &error).unwrap();
        assert_eq!(result.status, ResultStatus::PartialCompletion);
        assert_eq!(result.error.unwrap().code, "central.publication_uncertain");
    }
    #[test]
    fn actual_postpublication_eacces_retains_errno_and_material_without_retry() {
        assert_ne!(unsafe { libc::geteuid() }, 0, "nonroot OS permission prerequisite is mandatory");
        let fixture = Fixture::new();
        let restored = Arc::new(std::sync::Mutex::new(None)); let slot = restored.clone();
        at(RecordCheckpoint::AfterPublish, move |target| {
            let restore = RestoreMode::new(target.to_path_buf());
            fs::set_permissions(target, fs::Permissions::from_mode(0)).unwrap();
            *slot.lock().unwrap() = Some(restore);
        });
        let error = fixture.publish(b"retained material", RecordDisposition::CreateNew).unwrap_err();
        let oracle = File::open(fixture.target()).unwrap_err();
        let payload = error.get_ref().unwrap().downcast_ref::<RecordPublicationUncertain>().unwrap();
        assert_eq!(oracle.kind(), io::ErrorKind::PermissionDenied);
        assert!(oracle.raw_os_error().is_some());
        assert_eq!(payload.publication.cause.raw_os_error(), oracle.raw_os_error());
        assert_eq!(record_publication_observation(&error).unwrap()["published"], true);
        drop(restored.lock().unwrap().take());
        assert_eq!(fs::read(fixture.target()).unwrap(), b"retained material");
    }
    #[test]
    fn immutable_postpublication_privacy_rejects_current_inode_self_comparison() {
        let fixture = Fixture::new();
        at(RecordCheckpoint::AfterPublish, |target| fs::set_permissions(target, fs::Permissions::from_mode(0o644)).unwrap());
        let error = fixture.publish(b"actually published", RecordDisposition::CreateNew).unwrap_err();
        assert_eq!(record_publication_observation(&error).unwrap()["published"], true);
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(fixture.target()).unwrap(), b"actually published");
        assert_eq!(fs::metadata(fixture.target()).unwrap().mode() & 0o7777, 0o644);
    }
    #[test]
    fn native_record_capture_refuses_final_fifo_symlink_and_hardlink_without_effect() {
        for kind in ["fifo", "symlink", "hardlink"] {
            let fixture = Fixture::new();
            let sentinel = fixture.outer.join("unselected"); fs::write(&sentinel, b"untouched").unwrap();
            let target = fixture.target();
            match kind {
                "fifo" => {
                    let c = std::ffi::CString::new(target.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
                }
                "symlink" => symlink(&sentinel, &target).unwrap(),
                "hardlink" => fs::hard_link(&sentinel, &target).unwrap(),
                _ => unreachable!(),
            }
            let before = fs::symlink_metadata(&target).unwrap();
            let started = std::time::Instant::now();
            assert!(fixture.publish(b"refused bytes", RecordDisposition::ReplaceOrCreate).is_err());
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
            assert!(same_record_inode(&before, &fs::symlink_metadata(&target).unwrap()));
            assert_eq!(fs::read(&sentinel).unwrap(), b"untouched");
        }
    }
    #[test]
    fn cooperating_concurrent_disjoint_records_keep_exact_bytes_and_single_link() {
        let fixture = Fixture::new(); let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for n in 0..2 {
            let root = fixture.root.clone(); let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                let bytes = vec![b'a' + n; 256 * 1024]; barrier.wait();
                let member = format!("records/{n}.json");
                atomic_record(&root, Path::new(&member), &bytes, RecordDisposition::CreateNew).unwrap();
                assert_eq!(fs::read(root.join(&member)).unwrap(), bytes);
                assert_eq!(fs::metadata(root.join(member)).unwrap().nlink(), 1);
            }));
        }
        barrier.wait(); for worker in workers { worker.join().unwrap(); }
    }
    #[test]
    fn admitted_escaped_owner_material_above_eight_mib_is_not_truncated() {
        let fixture = Fixture::new();
        let body = "\n".repeat(4 * 1024 * 1024 - 256);
        let bytes = serde_json::to_vec(&json!({"basis_content":body,"proposed_content":body,"unknown":{"retained":true}})).unwrap();
        assert!(bytes.len() > 8 * 1024 * 1024);
        fixture.publish(&bytes, RecordDisposition::CreateNew).unwrap();
        assert_eq!(fs::read(fixture.target()).unwrap(), bytes);
        let read: Value = serde_json::from_slice(&fs::read(fixture.target()).unwrap()).unwrap();
        assert_eq!(read["unknown"]["retained"], true);
    }
    #[test]
    fn every_actual_held_record_authority_descriptor_is_cloexec() {
        let fixture = Fixture::new(); fixture.publish(b"basis", RecordDisposition::CreateNew).unwrap();
        let context = RecordDirectory::capture(&fixture.root, Path::new("records/record.json")).unwrap();
        let basis = RecordBasis::capture(&context.parent, std::ffi::OsStr::new("record.json")).unwrap().unwrap();
        let stage = record_open_in(&context.parent, std::ffi::OsStr::new("explicit-owned-stage"), libc::O_RDWR | libc::O_CREAT | libc::O_EXCL).unwrap();
        for file in [&context.root, &context.parent, &basis.file, &stage] {
            let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
            assert!(flags >= 0); assert_ne!(flags & libc::FD_CLOEXEC, 0);
        }
    }
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn actual_readonly_operational_record_retains_xattrs_without_new_readonly_policy() {
        let fixture = Fixture::new(); fixture.publish(b"before", RecordDisposition::CreateNew).unwrap();
        set_real_xattr(&File::open(fixture.target()).unwrap(), b"retained operational metadata");
        fs::set_permissions(fixture.target(), fs::Permissions::from_mode(0o440)).unwrap();
        let privacy = crate::wiki_publication::Privacy::read(&File::open(fixture.target()).unwrap()).unwrap();
        fixture.publish(b"after", RecordDisposition::ReplaceOrCreate).unwrap();
        privacy.verify(&File::open(fixture.target()).unwrap()).unwrap();
        assert_eq!(fs::read(fixture.target()).unwrap(), b"after");
    }
    #[test]
    fn fixture_cleanup_removes_only_its_owned_outer_and_preserves_neighbour() {
        let fixture = Fixture::new(); let neighbour = Fixture::new();
        let outer = fixture.outer.clone(); let root = fixture.root.clone();
        fs::write(neighbour.root.join("unselected"), b"neighbour retained").unwrap();
        drop(fixture); assert!(!outer.exists()); assert!(!root.exists());
        assert_eq!(fs::read(neighbour.root.join("unselected")).unwrap(), b"neighbour retained");
    }
    fn native_action(root: &Path, action: &str, input: Value) -> ActionResult {
        let mut registry = crate::action::create_core_action_registry();
        crate::projectcentral_ops::register_projectcentral_actions(&mut registry);
        let root_options = crate::root::RootOptions { explicit_root: Some(root.to_path_buf()), ..Default::default() };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = central_connector_sdk::ConnectorContext { platform: "native-record-fixture".into() };
        let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
        registry.execute(action, &input, &context)
    }
    #[test]
    fn actual_native_self_sources_converge_through_existing_owner_lock_and_keep_provenance() {
        let fixture = Fixture::new();
        crate::root::initialize_central(&fixture.root).unwrap();
        assert!(native_action(&fixture.root, "central.self.ensure", json!({})).ok);
        let barrier = Arc::new(Barrier::new(3)); let mut workers = Vec::new();
        for n in 0..2 {
            let root = fixture.root.clone(); let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                let result = native_action(&root, "central.self.source.create", json!({
                    "path":format!("native-{n}.md"),"content":format!("native payload {n}"),
                    "provenance":"generated-suggestion","standing":"agent-inference",
                    "actor_kind":"agent","agent_session_ref":"agent-session:native-publication-test","tier":n
                }));
                assert!(result.ok, "{}", serde_json::to_string(&result).unwrap());
            }));
        }
        barrier.wait(); for worker in workers { worker.join().unwrap(); }
        let bindings = crate::source_horizon::control_source_bindings(&fixture.root).unwrap();
        for n in 0..2 {
            let path = format!("Control/self/native-{n}.md");
            let matches: Vec<_> = bindings.iter().filter(|binding| binding.path == path).collect();
            assert_eq!(matches.len(), 1, "both created sources retain exactly one native binding");
            assert_eq!(matches[0].provenance, "generated-suggestion");
            assert_eq!(matches[0].standing, "agent-inference");
            let reading = crate::world_source::read_control_world_source(&fixture.root, &matches[0].source_ref).unwrap();
            assert_eq!(reading.content, format!("native payload {n}"));
            assert_eq!(reading.revision.revision, content_revision_bytes(reading.content.as_bytes()));
        }
        let field = crate::development_field::inspect_root_development_field(&fixture.root).unwrap();
        assert_eq!(field.tier_bindings.iter().filter(|binding| binding.sources.iter().any(Option::is_some)).count(), 2);
    }
    #[test]
    fn actual_native_self_first_publication_failure_retains_material_without_inventing_binding() {
        let fixture = Fixture::new(); crate::root::initialize_central(&fixture.root).unwrap();
        assert!(native_action(&fixture.root, "central.self.ensure", json!({})).ok);
        at(RecordCheckpoint::AfterPublish, |target| fs::set_permissions(target, fs::Permissions::from_mode(0o644)).unwrap());
        let result = native_action(&fixture.root, "central.self.source.create", json!({
            "path":"retained.md","content":"actual unpublished semantic source body",
            "provenance":"generated-suggestion","standing":"agent-inference","actor_kind":"agent"
        }));
        assert!(!result.ok); assert_eq!(result.status, ResultStatus::PartialCompletion);
        let error = result.error.unwrap(); assert_eq!(error.code, "central.publication_uncertain");
        assert_eq!(error.details.as_ref().unwrap()["record_publication"]["published"], true);
        assert_eq!(fs::read_to_string(fixture.root.join("Control/self/retained.md")).unwrap(), "actual unpublished semantic source body");
        let bindings = crate::source_horizon::control_source_bindings(&fixture.root).unwrap();
        assert!(!bindings.iter().any(|binding| binding.path == "Control/self/retained.md"));
        assert!(!serde_json::to_string(&error).unwrap().contains("actual unpublished semantic source body"));
    }
    fn fail_actual_creation_before_pending(restored: Arc<std::sync::Mutex<Option<RestoreMode>>>) {
        at(RecordCheckpoint::AfterPublish, move |target| {
            if target.file_name().is_some_and(|name| name == "creation.json") {
                let parent = target.parent().unwrap();
                let restore = RestoreMode::new(parent.to_path_buf());
                fs::set_permissions(parent, fs::Permissions::from_mode(0o555)).unwrap();
                *restored.lock().unwrap() = Some(restore);
            } else { fail_actual_creation_before_pending(restored); }
        });
    }
    #[test]
    fn actual_first_save_interruption_replays_only_same_native_request_and_keeps_creation_identity() {
        assert_ne!(unsafe { libc::geteuid() }, 0, "nonroot native permission prerequisite is mandatory");
        let fixture = Fixture::new(); crate::root::initialize_central(&fixture.root).unwrap();
        fs::create_dir(fixture.root.join("notes")).unwrap();
        let listing = native_action(&fixture.root, "central.files.list", json!({"path":"notes"}));
        assert!(listing.ok); let parent = listing.data.unwrap()["location"].clone();
        let input = json!({"parent":parent,"name":"first.md","content":"actual same-request first save",
            "expected_absent":true,"operation_ref":"return:native-first-save","actor":"human:fixture","actor_kind":"human"});
        let restored = Arc::new(std::sync::Mutex::new(None));
        fail_actual_creation_before_pending(restored.clone());
        let failure = native_action(&fixture.root, "central.files.create", input.clone());
        assert!(!failure.ok); assert_eq!(failure.status, ResultStatus::PartialCompletion);
        let details = failure.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["prior_owner_observation"]["owner_ref"], "return:native-first-save");
        assert_eq!(details["prior_owner_observation"]["source_publication"], "source_not_created_creation_intent_acknowledged");
        assert!(!fixture.root.join("notes/first.md").exists());
        let parent = restored.lock().unwrap().as_ref().unwrap().path.clone();
        let oracle = OpenOptions::new().create_new(true).write(true).open(parent.join("actual-permission-oracle")).unwrap_err();
        assert_eq!(oracle.kind(), io::ErrorKind::PermissionDenied); assert!(oracle.raw_os_error().is_some());
        assert!(details["prior_owner_observation"]["primary_cause"]["sources"].as_array().unwrap().iter()
            .any(|cause| cause["raw_os_error"] == json!(oracle.raw_os_error())));
        let original = fs::read(parent.join("creation.json")).unwrap();
        drop(restored.lock().unwrap().take());
        let mut different = input.clone(); different["operation_ref"] = json!("return:another-first-save");
        assert!(!native_action(&fixture.root, "central.files.create", different).ok);
        assert_eq!(fs::read(parent.join("creation.json")).unwrap(), original);
        let completed = native_action(&fixture.root, "central.files.create", input.clone());
        assert!(completed.ok, "{}", serde_json::to_string(&completed).unwrap());
        assert_eq!(completed.data.as_ref().unwrap()["operation_ref"], "return:native-first-save");
        assert_eq!(fs::read_to_string(fixture.root.join("notes/first.md")).unwrap(), "actual same-request first save");
        assert_eq!(fs::read(parent.join("creation.json")).unwrap(), original);
        let replay = native_action(&fixture.root, "central.files.create", input);
        assert!(replay.ok); assert_eq!(replay.data.unwrap()["outcome"], "unchanged");
        assert!(!serde_json::to_string(&failure).unwrap().contains("actual same-request first save"));
    }
    #[test]
    #[ignore = "requires the real bkmr provider in the joined native qualification job; must be explicitly selected there"]
    fn actual_native_database_backup_retains_prior_material_when_receipt_readback_fails() {
        assert_ne!(unsafe { libc::geteuid() }, 0, "nonroot actual OS permission prerequisite is mandatory");
        let fixture = Fixture::new(); let source_fixture = Fixture::new();
        crate::root::initialize_central(&fixture.root).unwrap();
        crate::root::initialize_central(&source_fixture.root).unwrap();
        let backend = crate::file_map_backend::Backend::new(&source_fixture.root);
        let version = backend.version().expect("actual native bkmr executable prerequisite");
        assert_eq!(version.trim(), format!("bkmr {}", crate::file_map_backend::VERSION));
        backend.prepare().unwrap();
        backend.run(&["add".into(), "https://example.invalid/native-backup".into(), "native".into(),
            "--title".into(), "Actual retained bookmark".into(), "--description".into(), "Native backup provenance".into(),
            "--no-web".into(), "--no-embed".into()]).unwrap();
        // A harmless actual table in the real bkmr database gives an exact
        // native backup oracle without replacing bkmr's schemas/functions.
        let database = rusqlite::Connection::open(backend.db()).unwrap();
        database.execute_batch("CREATE TABLE native_atomic_backup_proof(value TEXT); INSERT INTO native_atomic_backup_proof VALUES ('actual retained backup bytes');").unwrap();
        drop(database);
        let before = fs::read(backend.db()).unwrap();
        let inspected = native_action(&fixture.root, "central.file-map.inspect", json!({}));
        assert!(inspected.ok, "{}", serde_json::to_string(&inspected).unwrap());
        let inspected = inspected.data.unwrap();
        assert_eq!(inspected["schema"], "central.file-map/v1");
        assert_eq!(inspected["operation"], "inspect");
        let basis = inspected["result"]["revision"].as_str()
            .expect("actual native inspect result must carry a string revision").to_owned();
        let restored = Arc::new(std::sync::Mutex::new(None)); let slot = restored.clone();
        at(RecordCheckpoint::AfterPublish, move |target| {
            assert_eq!(target.file_name().unwrap(), "adoption.json");
            let restore = RestoreMode::new(target.to_path_buf());
            fs::set_permissions(target, fs::Permissions::from_mode(0)).unwrap();
            *slot.lock().unwrap() = Some(restore);
        });
        let failure = native_action(&fixture.root, "central.file-map.adopt-db", json!({
            "database":backend.db(),"quiesced":true,"expected_revision":basis
        }));
        assert!(!failure.ok); assert_eq!(failure.status, ResultStatus::PartialCompletion);
        let details = failure.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["record_publication"]["published"], true);
        assert_eq!(details["prior_owner_observation"]["source_publication"], "backup_read_observed_adoption_steps_may_be_partial");
        assert_eq!(details["prior_owner_observation"]["source_ref"], Value::Null);
        let receipt = fixture.root.join(".central/bkmr/adoption.json");
        let oracle = File::open(&receipt).unwrap_err();
        assert_eq!(oracle.kind(), io::ErrorKind::PermissionDenied); assert!(oracle.raw_os_error().is_some());
        assert_eq!(details["record_publication"]["cause"]["raw_os_error"], json!(oracle.raw_os_error()));
        assert!(!fixture.root.join(".central/bkmr/index.db").exists());
        assert_eq!(fs::read(backend.db()).unwrap(), before);
        let backup = rusqlite::Connection::open_with_flags(fixture.root.join(".central/bkmr/adopted-original.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let value: String = backup.query_row("SELECT value FROM native_atomic_backup_proof", [], |row| row.get(0)).unwrap();
        assert_eq!(value, "actual retained backup bytes"); drop(backup);
        drop(restored.lock().unwrap().take());
        let actual: Value = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
        assert_eq!(actual["status"], "backed-up");
        assert!(!serde_json::to_string(&failure).unwrap().contains("actual retained backup bytes"));
    }
    fn native_return_fixture(fixture: &Fixture, body: &str) -> (PathBuf, String, String) {
        crate::root::initialize_central(&fixture.root).unwrap();
        let project = fixture.root.join("Work/Proof"); fs::create_dir(&project).unwrap();
        crate::projectcentral_ops::initialize_projectcentral(&fixture.root, &project, "literal-native-id").unwrap();
        let member = "ProjectCentral/agents/wiki/notes.md";
        fs::write(project.join(member), body).unwrap();
        let bindings = crate::source_horizon::project_source_bindings(&project).unwrap();
        let source = bindings.iter().find(|binding| binding.path == member).unwrap().source_ref.clone();
        let current = crate::world_source::read_world_source(&project, &source).unwrap();
        (project, source, current.revision.revision)
    }
    #[test]
    fn actual_native_source_return_roundtrips_escaped_record_above_eight_mib() {
        let fixture = Fixture::new(); let body = "\n".repeat(4 * 1024 * 1024 - 256);
        let (project, source, revision) = native_return_fixture(&fixture, &body);
        let result = native_action(&fixture.root, "projectcentral.source.return", json!({
            "project":"Proof","source_ref":source,"expected_revision":revision,"proposed_content":body,
            "reason":"native large escaped proposal","evidence_refs":[],"agent_session_ref":"agent-session:native-large-return"
        }));
        assert!(result.ok, "{}", serde_json::to_string(&result).unwrap());
        let data = result.data.unwrap(); let reference = data["proposal"]["return_ref"].as_str().unwrap();
        let records: Vec<_> = fs::read_dir(project.join(".central/source-returns")).unwrap()
            .map(|entry| entry.unwrap().path()).filter(|path| path.extension().is_some_and(|extension| extension == "json")).collect();
        assert_eq!(records.len(), 1); assert!(fs::metadata(&records[0]).unwrap().len() > 8 * 1024 * 1024);
        let reading = native_action(&fixture.root, "projectcentral.source.return_read", json!({"project":"Proof","return_ref":reference}));
        assert!(reading.ok); let reading = reading.data.unwrap();
        assert_eq!(reading["proposal"]["basis_content"], body); assert_eq!(reading["proposal"]["proposed_content"], body);
        assert_eq!(reading["proposal"]["return_ref"], reference); assert_eq!(reading["proposal"]["status"], "pending");
        assert_eq!(fs::read_to_string(project.join("ProjectCentral/agents/wiki/notes.md")).unwrap(), body);
    }
    fn fail_actual_accepted_return(root: PathBuf, reference: String, source: String,
        restored: Arc<std::sync::Mutex<Option<RestoreMode>>>) {
        at(RecordCheckpoint::AfterPublish, move |target| {
            let is_record = target.parent() == Some(root.join(".central/source-returns").as_path());
            let accepted = if is_record {
                let actual: Value = serde_json::from_slice(&fs::read(target).unwrap()).unwrap();
                assert_eq!(actual["return_ref"], reference, "actual published record belongs to this native Return");
                assert_eq!(actual["source_ref"], source, "actual published record retains the native Source");
                if actual["status"] == "accepted" { true } else {
                    assert_eq!(actual["status"], "applying", "only the prior native applying publication may precede acceptance");
                    false
                }
            } else { false };
            if accepted {
                let restore = RestoreMode::new(target.to_path_buf());
                fs::set_permissions(target, fs::Permissions::from_mode(0)).unwrap();
                *restored.lock().unwrap() = Some(restore);
            } else { fail_actual_accepted_return(root, reference, source, restored); }
        });
    }
    #[test]
    fn actual_accepted_return_record_readback_fault_preserves_source_receipt_and_same_return() {
        assert_ne!(unsafe { libc::geteuid() }, 0, "nonroot native OS permission prerequisite is mandatory");
        let fixture = Fixture::new(); let (project, source, revision) = native_return_fixture(&fixture, "before native return");
        let proposal = native_action(&fixture.root, "projectcentral.source.return", json!({
            "project":"Proof","source_ref":source,"expected_revision":revision,"proposed_content":"after native return",
            "reason":"actual source then record readback fault","agent_session_ref":"agent-session:native-return-fault","evidence_refs":[]
        }));
        assert!(proposal.ok); let data = proposal.data.unwrap();
        let reference = data["proposal"]["return_ref"].as_str().unwrap().to_owned();
        let restored = Arc::new(std::sync::Mutex::new(None));
        // The existing physical publisher reports canonical target coordinates;
        // this fixture retains a lexical CARGO_MANIFEST_DIR/../ Run-space path.
        let physical_project = fs::canonicalize(&project).unwrap();
        fail_actual_accepted_return(physical_project.clone(), reference.clone(), source.clone(), restored.clone());
        let failed = native_action(&fixture.root, "projectcentral.source.return_accept", json!({
            "project":"Proof","return_ref":reference,"expected_revision":revision,
            "acceptance":"human-accepted","accepted_by_ref":"human:native-fixture"
        }));
        let record = restored.lock().unwrap().as_ref()
            .expect("the actual accepted-record checkpoint must have changed its owned file permissions").path.clone();
        assert_eq!(record.parent(), Some(physical_project.join(".central/source-returns").as_path()));
        assert_eq!(fs::metadata(&record).unwrap().mode() & 0o777, 0);
        let oracle = File::open(&record).unwrap_err();
        assert_eq!(oracle.kind(), io::ErrorKind::PermissionDenied);
        assert!(oracle.raw_os_error().is_some(), "genuine native OS read refusal must retain errno");
        assert!(!failed.ok); assert_eq!(failed.status, ResultStatus::PartialCompletion);
        let error = failed.error.unwrap(); assert_eq!(error.code, "central.publication_uncertain");
        let details = error.details.as_ref().unwrap(); assert_eq!(details["record_publication"]["published"], true);
        assert_eq!(details["prior_owner_observation"]["source_publication"], "acknowledged");
        assert_eq!(details["prior_owner_observation"]["source_ref"], source);
        let actual = crate::world_source::read_world_source(&project, &source).unwrap();
        assert_eq!(actual.content, "after native return");
        assert_eq!(details["prior_owner_observation"]["revision_observation"], actual.revision.revision);
        assert_eq!(details["record_publication"]["cause"]["raw_os_error"], json!(oracle.raw_os_error()));
        assert_eq!(oracle.kind(), io::ErrorKind::PermissionDenied);
        assert!(!serde_json::to_string(&error).unwrap().contains("after native return"));
        drop(restored.lock().unwrap().take());
        let read = native_action(&fixture.root, "projectcentral.source.return_read", json!({"project":"Proof","return_ref":reference}));
        assert!(read.ok); let read = read.data.unwrap();
        assert_eq!(read["proposal"]["return_ref"], reference); assert_eq!(read["proposal"]["status"], "accepted");
        assert_eq!(read["proposal"]["result_revision"], actual.revision.revision);
        assert_eq!(fs::read_to_string(project.join("ProjectCentral/agents/wiki/notes.md")).unwrap(), "after native return");
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn set_real_xattr(file: &File, value: &[u8]) {
        let name = std::ffi::CString::new("user.central-native-record-fixture").unwrap();
        #[cfg(target_os = "macos")]
        let result = unsafe { libc::fsetxattr(file.as_raw_fd(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0, 0) };
        #[cfg(target_os = "linux")]
        let result = unsafe { libc::fsetxattr(file.as_raw_fd(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0) };
        assert_eq!(result, 0, "native xattr prerequisite: {}", io::Error::last_os_error());
    }
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn actual_record_xattrs_survive_update_and_changed_stage_privacy_refuses_before_bytes() {
        let fixture = Fixture::new(); fixture.publish(b"old record", RecordDisposition::CreateNew).unwrap();
        set_real_xattr(&File::open(fixture.target()).unwrap(), b"actual privacy extension");
        let privacy = crate::wiki_publication::Privacy::read(&File::open(fixture.target()).unwrap()).unwrap();
        fixture.publish(b"new record", RecordDisposition::ReplaceOrCreate).unwrap();
        privacy.verify(&File::open(fixture.target()).unwrap()).unwrap();
        at(RecordCheckpoint::BeforeData, |stage| set_real_xattr(&File::open(stage).unwrap(), b"different stage metadata"));
        let error = fixture.publish(b"refused next record", RecordDisposition::ReplaceOrCreate).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied); assert!(record_publication_observation(&error).is_none());
        assert_eq!(fs::read(fixture.target()).unwrap(), b"new record");
        privacy.verify(&File::open(fixture.target()).unwrap()).unwrap();
        let retained: Vec<_> = fs::read_dir(fixture.root.join("records")).unwrap().map(|entry| entry.unwrap().path())
            .filter(|path| path.file_name().unwrap().as_encoded_bytes().starts_with(b".central-record-")).collect();
        assert_eq!(retained.len(), 1); assert!(fs::read(&retained[0]).unwrap().is_empty());
    }
}
