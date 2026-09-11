//! Shared temporal source operations. Authored files and source-relations remain
//! authoritative; the existing Source Change Horizon remains their index.
use crate::projectcentral_flow::{content_revision_bytes, relative_member};
use crate::source_horizon::{SourceBinding, SourceRevision, SourceWriteAttribution};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
pub(crate) fn conflict(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, message.into())
}
pub(crate) fn denied(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message.into())
}
pub(crate) fn text<'a>(input: &'a Value, field: &str) -> io::Result<&'a str> {
    input.get(field).and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 4096)
        .ok_or_else(|| invalid(format!("{field} requires non-empty text of at most 4096 bytes")))
}
pub(crate) fn encoded<T: Serialize>(value: &T) -> io::Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}
pub(crate) fn revision(content: &str) -> String {
    content_revision_bytes(content.as_bytes())
}
pub(crate) fn key(content: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

#[derive(Debug, Clone, Serialize)]
pub struct Scope {
    pub central_root: PathBuf,
    pub root: PathBuf,
    pub world_ref: String,
    pub project: Option<String>,
    pub prefix: String,
    pub relations_path: String,
    pub relations_schema: String,
    pub relations_id: String,
}
impl Scope {
    pub fn resolve(central: &Path, project: Option<&str>) -> io::Result<Self> {
        let central = fs::canonicalize(central)?;
        crate::file_mutation::directory(&central, Path::new("Control"))?;
        crate::file_mutation::directory(&central, Path::new("Work"))?;
        if let Some(project) = project {
            let member = relative_member(project)?;
            if member.components().count() != 1 {
                return Err(invalid("project is one existing Central/Work member"));
            }
            crate::file_mutation::directory(&central, &Path::new("Work").join(&member))?;
            let root = central.join("Work").join(member);
            crate::file_mutation::open_native_file(&root, "ProjectCentral/project.json")?;
            let manifest = crate::projectcentral::read_project_manifest(&root)?;
            let validation = manifest.validate();
            if !validation.valid { return Err(invalid(validation.errors.join("; "))); }
            Ok(Self {
                central_root: central, root,
                world_ref: format!("project:{}", manifest.project_id),
                project: Some(project.into()), prefix: "ProjectCentral".into(),
                relations_path: crate::source_horizon::GROUND_RELATIONS_SOURCE.into(),
                relations_schema: crate::source_horizon::GROUND_RELATIONS_SCHEMA.into(),
                relations_id: manifest.project_id,
            })
        } else {
            Ok(Self {
                central_root: central.clone(), root: central,
                world_ref: crate::source_horizon::CONTROL_WORLD_REF.into(),
                project: None, prefix: "Control".into(),
                relations_path: crate::source_horizon::CONTROL_GROUND_RELATIONS_SOURCE.into(),
                relations_schema: crate::source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA.into(),
                relations_id: crate::source_horizon::CONTROL_WORLD_REF.into(),
            })
        }
    }
    pub fn now_dir(&self) -> String { format!("{}/agents/now/clearings", self.prefix) }
    pub fn day_dir(&self) -> String { format!("{}/user/day", self.prefix) }
    pub fn source_ref(&self, path: &str) -> String {
        crate::source_horizon::source_ref(&self.world_ref, path)
    }
    pub fn relations(&self) -> io::Result<(Value, String)> {
        let raw = match crate::source_safety::read(&self.root, &self.relations_path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok((json!({"schema":self.relations_schema,"project_id":self.relations_id,"relations":[]}), "absent".into()));
            }
            Err(e) => return Err(e),
        };
        let value: Value = serde_json::from_str(&raw)?;
        if value["schema"] != self.relations_schema || value["project_id"] != self.relations_id {
            return Err(invalid("source relations have changed World identity or schema"));
        }
        let entries = value["relations"].as_array().ok_or_else(|| invalid("relations must be an array"))?;
        let mut refs = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        for entry in entries {
            relative_member(text(entry, "path")?)?;
            if !refs.insert(text(entry, "ref")?) || !paths.insert(text(entry, "path")?) {
                return Err(invalid("ambiguous duplicate source relation; reconcile before mutation"));
            }
        }
        Ok((value, revision(&raw)))
    }
    pub fn bindings(&self) -> io::Result<Vec<SourceBinding>> {
        self.relations()?;
        if self.project.is_some() {
            crate::source_horizon::project_source_bindings(&self.root)
        } else {
            crate::source_horizon::control_source_bindings(&self.root)
        }
    }
    pub fn read(&self, reference: &str) -> io::Result<SourceReading> {
        let source = self.bindings()?.into_iter().find(|b| b.source_ref == reference)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "SourceRef is not in this World"))?;
        if !source.agent_retrieval_allowed { return Err(denied("SourceRef is excluded from Agent retrieval")); }
        let content = crate::source_safety::read(&self.root, &source.path)?;
        let revision = SourceRevision { revision: revision(&content), byte_len: content.len() as u64 };
        Ok(SourceReading { source, revision, content })
    }
    pub fn reconcile(&self, actor: Option<(&str, &str, Option<String>)>, refs: &[String]) -> io::Result<Value> {
        let mut attributions = BTreeMap::new();
        if let Some((actor, kind, session)) = actor {
            for reference in refs {
                attributions.insert(reference.clone(), SourceWriteAttribution {
                    actor: actor.into(), actor_kind: kind.into(), agent_session_ref: session.clone(),
                });
            }
        }
        let report = if self.project.is_some() {
            crate::source_horizon::reconcile_project_source_writes(&self.root, &attributions)?
        } else {
            crate::source_horizon::reconcile_control_source_writes(&self.root, &attributions)?
        };
        Ok(json!({"world_ref":report.horizon.world_ref,"cursor":report.horizon.cursor,"changes":report.new_changes}))
    }
    /// Caller holds source-mutation.lock. Existing identities and unrelated
    /// JSON fields are retained, including subject and Recognition metadata.
    pub fn bind(&self, binding: &SourceBinding, recorded_at: u64) -> io::Result<()> {
        relative_member(&binding.path)?;
        let (mut relations, basis) = self.relations()?;
        let entries = relations["relations"].as_array_mut().ok_or_else(|| invalid("invalid relations"))?;
        for old in entries.iter() {
            if old["ref"] == binding.source_ref || old["path"] == binding.path {
                if old["ref"] == binding.source_ref && old["path"] == binding.path {
                    return Ok(());
                }
                return Err(conflict("SourceRef/path is already bound; use migration rather than replacing identity"));
            }
        }
        entries.push(json!({
            "ref":binding.source_ref,"path":binding.path,"roles":binding.roles,
            "provenance":binding.provenance,"standing":binding.standing,"treatment":binding.treatment,
            "recognition":"owner-recorded-source-relation-not-human-recognition",
            "recorded_at_unix_seconds":recorded_at
        }));
        let content = encoded(&relations)?;
        if basis == "absent" { put_new(&self.root, &self.relations_path, &content)?; }
        else { crate::source_safety::replace(&self.root, &self.relations_path, &basis, &content)?; }
        Ok(())
    }
    pub fn create_agent_source(&self, path: &str, content: &str, role: &str, at: u64) -> io::Result<SourceReading> {
        if !Path::new(path).starts_with(format!("{}/agents", self.prefix)) {
            return Err(denied("Agent source creation must remain in this register's agents aperture"));
        }
        if !crate::source_horizon::retrieval_allowed(&self.root, &self.root.join(path)) {
            return Err(denied("destination is protected from Agent retrieval"));
        }
        put_new(&self.root, path, content)?;
        let binding = SourceBinding {
            source_ref: self.source_ref(path), path: path.into(), roles: vec![role.into()],
            provenance: "agent-maintained".into(), standing: "current-development-state".into(),
            treatment: "generated-derived".into(), agent_retrieval_allowed: true,
        };
        self.bind(&binding, at)?;
        self.read(&binding.source_ref)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReading {
    pub source: SourceBinding,
    pub revision: SourceRevision,
    pub content: String,
}

pub(crate) struct MutationLocks {
    _root: crate::source_safety::SourceLock,
    _project: Option<crate::source_safety::SourceLock>,
}
pub(crate) fn lock(scope: &Scope) -> io::Result<MutationLocks> {
    // One lock order for root policy + scoped sources. Never re-enter through
    // write_world_source while these guards are held.
    let root = crate::source_safety::lock(&scope.central_root, "source-mutation.lock")?;
    let project = if scope.project.is_some() {
        Some(crate::source_safety::lock(&scope.root, "source-mutation.lock")?)
    } else { None };
    Ok(MutationLocks { _root: root, _project: project })
}

/// Descriptor-relative mkdir/open, sharing the native no-follow filesystem seam.
/// Symlinks and a substituted ancestor are not followed, even on creation.
pub(crate) fn directories(root: &Path, relative: &Path) -> io::Result<File> {
    let mut directory = crate::file_mutation::directory(root, Path::new(""))?;
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else { return Err(invalid("invalid directory member")); };
        let name = CString::new(part.as_encoded_bytes()).map_err(io::Error::other)?;
        let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
        if result != 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
            return Err(io::Error::last_os_error());
        }
        directory.sync_all()?;
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW) };
        if fd < 0 { return Err(io::Error::last_os_error()); }
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory)
}
fn unlink(parent: &File, name: &str) -> io::Result<()> {
    let name = CString::new(name).map_err(io::Error::other)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// Publish a complete source without replacing an existing file. Replay only
/// accepts identical bytes, and completes a link/unlink interrupted publication.
pub(crate) fn put_new(root: &Path, relative: &str, content: &str) -> io::Result<bool> {
    if content.len() > crate::source_safety::MAX_SOURCE || content.contains('\0') {
        return Err(invalid("source must be bounded UTF-8 text without NUL"));
    }
    let path = relative_member(relative)?;
    let parent_path = path.parent().ok_or_else(|| invalid("source parent required"))?;
    let parent = directories(root, parent_path)?;
    let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| invalid("UTF-8 source name required"))?;
    let staging = format!(".central-new-{}", key(&format!("{relative}\n{content}")));
    let staging_path = parent_path.join(&staging).to_string_lossy().into_owned();
    match crate::source_safety::read(root, relative) {
        Ok(existing) => {
            if existing != content { return Err(conflict("existing source bytes differ; no overwrite performed")); }
            if let Ok(stage) = crate::file_mutation::open_native_file(root, &staging_path) {
                let current = crate::file_mutation::open_native_file(root, relative)?;
                if stage.metadata()?.ino() == current.metadata()?.ino() && stage.metadata()?.dev() == current.metadata()?.dev() {
                    unlink(&parent, &staging)?;
                    parent.sync_all()?;
                }
            }
            return Ok(false);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    match crate::file_mutation::create_in(&parent, &staging, 0o600) {
        Ok(mut file) => { file.write_all(content.as_bytes())?; file.sync_all()?; }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if crate::source_safety::read(root, &staging_path)? != content {
                return Err(conflict("incomplete source staging requires explicit recovery; authored bytes were not replaced"));
            }
        }
        Err(e) => return Err(e),
    }
    let held = parent.metadata()?;
    let current = crate::file_mutation::directory(root, parent_path)?.metadata()?;
    if held.dev() != current.dev() || held.ino() != current.ino() {
        return Err(conflict("destination parent changed before source publication"));
    }
    let from = CString::new(staging.as_str()).map_err(io::Error::other)?;
    let to = CString::new(name).map_err(io::Error::other)?;
    if unsafe { libc::linkat(parent.as_raw_fd(), from.as_ptr(), parent.as_raw_fd(), to.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    parent.sync_all()?;
    unlink(&parent, &staging)?;
    parent.sync_all()?;
    Ok(true)
}
