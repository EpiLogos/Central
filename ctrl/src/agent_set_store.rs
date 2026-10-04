//! Durable authored sources for AgentSets (`central.agent-set/v1`) and world
//! relations (`central.world-relations/v1`) — W10 V2.
//!
//! The domain model lives in [`crate::world`]; this module is its disk
//! carrier, under the same discipline as the AgentProfile store: the stable
//! ref lives in the document (never inferred from the filename), saves run
//! under compare-and-swap revision discipline, writes are atomic, and
//! symlinked containers are refused. Root register:
//! `Control/agents/agent-sets/` + `Control/relations/worlds/`; Project
//! register: `ProjectCentral/agents/agent-sets/` +
//! `ProjectCentral/relations/worlds/`. Agent-sets sit with the agent profiles
//! under `agents/`; only world relations remain under `relations/`.

use serde::Serialize;
use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::world::{AgentSetRecord, WorldRecord, AGENT_SET_SCHEMA, WORLD_RELATION_SCHEMA};

/// Finite eager metadata profile, not a World/AgentSet domain size limit.
/// Larger retained records remain untouched and unavailable under this profile;
/// an explicit larger finite native owner profile is not yet exposed.
pub const RELATION_RECORD_METADATA_LIMIT: usize = 8 * 1024 * 1024;

pub const ROOT_RELATIONS_DIR: &str = "Control/relations";
pub const ROOT_AGENT_SET_DIR: &str = "Control/agents/agent-sets";
pub const ROOT_WORLD_DIR: &str = "Control/relations/worlds";
pub const PROJECT_AGENT_SET_DIR: &str = "ProjectCentral/agents/agent-sets";
pub const PROJECT_WORLD_DIR: &str = "ProjectCentral/relations/worlds";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationRecordReading {
    /// The kind tag of the stored record: `agent-set` or `world`.
    pub kind: RelationRecordKind,
    /// The stable ref carried inside the document.
    #[serde(rename = "ref")]
    pub ref_: String,
    pub revision: String,
    pub source_path: String,
    /// The full record, JSON-shaped for Action outputs.
    pub record: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationRecordKind {
    AgentSet,
    World,
}

impl RelationRecordKind {
    fn dir_name(self) -> &'static str {
        match self {
            RelationRecordKind::AgentSet => "agent-sets",
            RelationRecordKind::World => "worlds",
        }
    }

    fn schema(self) -> &'static str {
        match self {
            RelationRecordKind::AgentSet => AGENT_SET_SCHEMA,
            RelationRecordKind::World => WORLD_RELATION_SCHEMA,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationRecordWriteReceipt {
    pub kind: RelationRecordKind,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub previous_revision: Option<String>,
    pub revision: String,
    pub source_path: String,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationRecordStore {
    owner_root: PathBuf,
    project_scope: bool,
    kind: RelationRecordKind,
}

impl RelationRecordStore {
    pub fn agent_sets_at_root(central_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: central_root.into(),
            project_scope: false,
            kind: RelationRecordKind::AgentSet,
        }
    }

    pub fn agent_sets_in_project(project_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: project_root.into(),
            project_scope: true,
            kind: RelationRecordKind::AgentSet,
        }
    }

    pub fn worlds_at_root(central_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: central_root.into(),
            project_scope: false,
            kind: RelationRecordKind::World,
        }
    }

    pub fn worlds_in_project(project_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: project_root.into(),
            project_scope: true,
            kind: RelationRecordKind::World,
        }
    }

    pub fn kind(&self) -> RelationRecordKind {
        self.kind
    }

    pub fn is_project_scope(&self) -> bool {
        self.project_scope
    }

    pub fn source_dir(&self) -> PathBuf {
        let (root_dir, project_dir) = match self.kind {
            RelationRecordKind::AgentSet => (ROOT_AGENT_SET_DIR, PROJECT_AGENT_SET_DIR),
            RelationRecordKind::World => (ROOT_WORLD_DIR, PROJECT_WORLD_DIR),
        };
        self.owner_root.join(if self.project_scope {
            project_dir
        } else {
            root_dir
        })
    }

    pub fn source_path(&self, ref_: &str) -> Result<PathBuf, RelationRecordStoreError> {
        validate_ref(ref_)?;
        Ok(self.source_dir().join(format!(
            "{}-{}.json",
            self.kind.dir_name().trim_end_matches('s'),
            record_key(ref_)
        )))
    }

    pub fn read(&self, ref_: &str) -> Result<RelationRecordReading, RelationRecordStoreError> {
        self.validate_root()?;
        let path = self.source_path(ref_)?;
        let directory = ReadDirectory::capture(self)?;
        read_checkpoint(&self.source_dir());
        if directory.missing.is_some() {
            directory.validate()?;
            return Err(RelationRecordStoreError::NotFound(path));
        }
        let (record, basis) = match self.read_record(&path, ref_) {
            Ok(record) => record,
            Err(error @ RelationRecordStoreError::NotFound(_)) => {
                // A missing member cannot disguise a changed owner/container.
                directory.validate()?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        material_checkpoint(&path);
        directory.validate()?;
        let qualification = self.qualify_record(&path, &basis);
        directory.validate()?;
        qualification?;
        Ok(record)
    }

    pub fn list(&self) -> Result<Vec<RelationRecordReading>, RelationRecordStoreError> {
        self.validate_root()?;
        let directory = ReadDirectory::capture(self)?;
        let dir = self.source_dir();
        read_checkpoint(&dir);
        if directory.missing.is_some() {
            directory.validate()?;
            return Ok(Vec::new());
        }
        directory.validate()?;
        let membership = record_membership(&dir)?;
        directory.validate()?;
        let mut readings = Vec::new();
        let mut bases = Vec::new();
        for member in &membership {
            let path = dir.join(member);
            let captured = self.capture_record(&path)?;
            let record = captured.record;
            let ref_ = record
                .get("ref")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    RelationRecordStoreError::InvalidRecord(format!(
                        "{} carries no ref",
                        path.display()
                    ))
                })?
                .to_owned();
            let expected = self.source_path(&ref_)?;
            if expected != path {
                return Err(RelationRecordStoreError::SourcePathMismatch {
                    ref_: ref_.clone(),
                    expected,
                    actual: path,
                });
            }
            readings.push(self.reading(record, &ref_, &expected));
            bases.push((expected, captured.basis));
        }
        material_checkpoint(&dir);
        directory.validate()?;
        // Reopen one material at a time after every collection checkpoint.
        // These bases are observations, not semantic identity or a tree lease.
        for (path, basis) in &bases {
            let qualification = self.qualify_record(path, basis);
            directory.validate()?;
            qualification?;
        }
        directory.validate()?;
        // A complete list must qualify new/removed native members too, even
        // when its initially observed container was empty. Unknown ordinary
        // non-JSON entries remain outside the native record membership.
        let current_membership = record_membership(&dir)?;
        directory.validate()?;
        if current_membership != membership {
            return Err(std::io::Error::other(
                "Native relation record membership changed before acknowledgement",
            )
            .into());
        }
        readings.sort_by(|left, right| left.ref_.cmp(&right.ref_));
        Ok(readings)
    }

    /// Save authored source under compare-and-swap revision discipline.
    ///
    /// - create: `expected_revision = None`, source must not already exist;
    /// - update: `expected_revision = Some(current)`, source must exist and match;
    /// - every update must advance the authored revision;
    /// - the stable ref never changes across revisions.
    pub fn save(
        &self,
        record: &serde_json::Value,
        expected_revision: Option<&str>,
    ) -> Result<RelationRecordWriteReceipt, RelationRecordStoreError> {
        self.validate_root()?;
        if record.get("schema").and_then(|value| value.as_str()) != Some(self.kind.schema()) {
            return Err(RelationRecordStoreError::InvalidRecord(format!(
                "unsupported schema for a {} record",
                self.kind.dir_name().trim_end_matches('s')
            )));
        }
        let ref_ = record
            .get("ref")
            .and_then(|value| value.as_str())
            .ok_or_else(|| RelationRecordStoreError::InvalidRecord("record carries no ref".into()))?
            .to_owned();
        validate_ref(&ref_)?;
        let revision = record
            .get("revision")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                RelationRecordStoreError::InvalidRecord("record carries no revision".into())
            })?;
        if revision.trim().is_empty() {
            return Err(RelationRecordStoreError::InvalidRecord(
                "revision cannot be empty".into(),
            ));
        }
        self.validate_typed(record)?;

        let mut bytes = serde_json::to_vec_pretty(record)
            .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))?;
        bytes.push(b'\n');
        if bytes.len() > RELATION_RECORD_METADATA_LIMIT {
            return Err(RelationRecordStoreError::RecordBudget {
                byte_len: bytes.len() as u64,
                limit: RELATION_RECORD_METADATA_LIMIT as u64,
            });
        }
        let dir = self.source_dir();
        ensure_directory_path(&self.owner_root, &dir)?;
        let path = self.source_path(&ref_)?;
        let existing = if path.exists() {
            Some(self.parse_record(&path)?)
        } else {
            None
        };

        let (previous_revision, created) = match (existing.as_ref(), expected_revision) {
            (None, None) => (None, true),
            (None, Some(expected)) => {
                return Err(RelationRecordStoreError::MissingForUpdate {
                    ref_: ref_.clone(),
                    expected: expected.to_owned(),
                })
            }
            (Some(current), None) => {
                return Err(RelationRecordStoreError::AlreadyExists {
                    ref_: ref_.clone(),
                    revision: current
                        .get("revision")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                })
            }
            (Some(current), Some(expected)) => {
                let current_revision = current
                    .get("revision")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                if current_revision != expected {
                    return Err(RelationRecordStoreError::RevisionConflict {
                        ref_: ref_.clone(),
                        expected: expected.to_owned(),
                        actual: current_revision.to_owned(),
                    });
                }
                if current_revision == revision {
                    return Err(RelationRecordStoreError::RevisionNotAdvanced {
                        ref_: ref_.clone(),
                        revision: revision.to_owned(),
                    });
                }
                (Some(current_revision.to_owned()), false)
            }
        };

        atomic_write(&path, &bytes)?;

        Ok(RelationRecordWriteReceipt {
            kind: self.kind,
            ref_,
            previous_revision,
            revision: revision.to_owned(),
            source_path: relative(&self.owner_root, &path),
            created,
        })
    }

    /// Remove only the authored record. Referenced agents, sets or worlds
    /// live in their own sources and are untouched.
    pub fn remove(
        &self,
        ref_: &str,
        expected_revision: &str,
    ) -> Result<RelationRecordReading, RelationRecordStoreError> {
        let reading = self.read(ref_)?;
        if reading.revision != expected_revision {
            return Err(RelationRecordStoreError::RevisionConflict {
                ref_: ref_.to_owned(),
                expected: expected_revision.to_owned(),
                actual: reading.revision,
            });
        }
        let path = self.source_path(ref_)?;
        fs::remove_file(path)?;
        Ok(reading)
    }

    /// Load every record of this kind into the domain model (`T` is
    /// [`AgentSetRecord`] or [`WorldRecord`]).
    pub fn load_typed<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Result<Vec<T>, RelationRecordStoreError> {
        self.list()?
            .into_iter()
            .map(|reading| {
                serde_json::from_value(reading.record)
                    .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn validate_typed(&self, record: &serde_json::Value) -> Result<(), RelationRecordStoreError> {
        match self.kind {
            RelationRecordKind::AgentSet => {
                let typed: AgentSetRecord = serde_json::from_value(record.clone())
                    .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))?;
                AgentSetRegistryProbe::insert(typed)?;
            }
            RelationRecordKind::World => {
                let typed: WorldRecord = serde_json::from_value(record.clone())
                    .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))?;
                WorldGraphProbe::insert(typed)?;
            }
        }
        Ok(())
    }

    fn read_record(
        &self,
        path: &Path,
        requested_ref: &str,
    ) -> Result<(RelationRecordReading, RecordBasis), RelationRecordStoreError> {
        let captured = self.capture_record(path)?;
        let ref_ = captured
            .record
            .get("ref")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                RelationRecordStoreError::InvalidRecord("record carries no ref".into())
            })?;
        if ref_ != requested_ref {
            return Err(RelationRecordStoreError::RefMismatch {
                requested: requested_ref.to_owned(),
                actual: ref_.to_owned(),
            });
        }
        Ok((
            self.reading(captured.record, requested_ref, path),
            captured.basis,
        ))
    }

    fn parse_record(&self, path: &Path) -> Result<serde_json::Value, RelationRecordStoreError> {
        Ok(self.capture_record(path)?.record)
    }

    fn capture_record(&self, path: &Path) -> Result<CapturedRecord, RelationRecordStoreError> {
        let directory = ReadDirectory::capture(self)?;
        if directory.missing.is_some() {
            directory.validate()?;
            return Err(RelationRecordStoreError::NotFound(path.to_path_buf()));
        }
        let relative = path
            .strip_prefix(&self.owner_root)
            .map_err(|_| RelationRecordStoreError::UnsafeSource(path.to_path_buf()))?;
        let admitted = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                directory.validate()?;
                return Err(RelationRecordStoreError::NotFound(path.to_path_buf()));
            }
            Err(error) => return Err(error.into()),
        };
        if admitted.file_type().is_symlink() || !admitted.is_file() {
            return Err(RelationRecordStoreError::UnsafeSource(path.to_path_buf()));
        }
        record_open_checkpoint(path);
        let root_metadata = &directory.held[0].2;
        let mut reader = crate::file_mutation::NativeFileRead::open(
            &self.owner_root,
            (root_metadata.dev(), root_metadata.ino()),
            relative,
        )?;
        // The opened native reader qualifies the same material before this
        // capacity observation. A changed path cannot masquerade as a budget.
        let opened = fs::symlink_metadata(path)?;
        reader.validate()?;
        directory.validate()?;
        if !opened.is_file() || physical_basis(&opened) != physical_basis(&admitted) {
            return Err(std::io::Error::other(
                "Native relation record changed before capacity observation",
            )
            .into());
        }
        if opened.len() > RELATION_RECORD_METADATA_LIMIT as u64 {
            return Err(RelationRecordStoreError::RecordBudget {
                byte_len: opened.len(),
                limit: RELATION_RECORD_METADATA_LIMIT as u64,
            });
        }
        let bytes = reader.read_metadata_bytes(RELATION_RECORD_METADATA_LIMIT)?;
        reader.validate()?;
        directory.validate()?;
        let current = fs::symlink_metadata(path)?;
        if !current.is_file() || physical_basis(&current) != physical_basis(&admitted) {
            return Err(
                std::io::Error::other("Native relation record changed during capture").into(),
            );
        }
        let record: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))?;
        if record.get("schema").and_then(|value| value.as_str()) != Some(self.kind.schema()) {
            return Err(RelationRecordStoreError::InvalidRecord(format!(
                "unsupported schema {}",
                record
                    .get("schema")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
            )));
        }
        Ok(CapturedRecord {
            record,
            basis: RecordBasis {
                physical: physical_basis(&current),
                content_revision: crate::source_safety::content_revision_bytes(&bytes),
            },
        })
    }

    fn qualify_record(
        &self,
        path: &Path,
        basis: &RecordBasis,
    ) -> Result<(), RelationRecordStoreError> {
        let current = self.capture_record(path)?;
        if &current.basis != basis {
            return Err(std::io::Error::other(
                "Native relation record material basis changed before acknowledgement",
            )
            .into());
        }
        Ok(())
    }

    fn reading(&self, record: serde_json::Value, ref_: &str, path: &Path) -> RelationRecordReading {
        RelationRecordReading {
            kind: self.kind,
            ref_: ref_.to_owned(),
            revision: record
                .get("revision")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            source_path: relative(&self.owner_root, path),
            record,
        }
    }

    fn validate_root(&self) -> Result<(), RelationRecordStoreError> {
        let metadata = fs::symlink_metadata(&self.owner_root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RelationRecordStoreError::UnsafeRoot(
                self.owner_root.clone(),
            ));
        }
        Ok(())
    }
}

// Content and material observations are not native record identities. Retain
// only this small basis per list member; held file descriptors are sequential.
#[derive(Debug, PartialEq, Eq)]
struct RecordBasis {
    physical: (u64, u64, u64, i64, i64, i64, i64),
    content_revision: String,
}
struct CapturedRecord {
    record: serde_json::Value,
    basis: RecordBasis,
}
fn physical_basis(metadata: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

/// Observe the native record namespace through the same admission at initial
/// collection and final acknowledgement. Names are exact OS observations, not
/// record identities. This neither reads newly observed bodies nor leases the
/// directory against arbitrary external writers.
fn record_membership(dir: &Path) -> Result<BTreeSet<OsString>, RelationRecordStoreError> {
    let mut members = BTreeSet::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        // Preserve the existing refusal of any symbolic entry, including one
        // whose name is not JSON; unrelated ordinary entries stay ignored.
        if metadata.file_type().is_symlink() {
            return Err(RelationRecordStoreError::UnsafeSource(path));
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if !metadata.is_file() {
            return Err(RelationRecordStoreError::UnsafeSource(path));
        }
        members.insert(entry.file_name());
    }
    Ok(members)
}

#[cfg(test)]
type ReadCheckpointHook = Box<dyn FnOnce(&Path)>;
#[cfg(test)]
thread_local! {
    static READ_CHECKPOINT: std::cell::RefCell<Option<ReadCheckpointHook>> = const { std::cell::RefCell::new(None) };
    static MATERIAL_CHECKPOINT: std::cell::RefCell<Option<ReadCheckpointHook>> = const { std::cell::RefCell::new(None) };
    static RECORD_OPEN_CHECKPOINT: std::cell::RefCell<Option<ReadCheckpointHook>> = const { std::cell::RefCell::new(None) };
}
fn read_checkpoint(path: &Path) {
    #[cfg(test)]
    READ_CHECKPOINT.with(|checkpoint| {
        if let Some(checkpoint) = checkpoint.borrow_mut().take() {
            checkpoint(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

fn record_open_checkpoint(path: &Path) {
    #[cfg(test)]
    RECORD_OPEN_CHECKPOINT.with(|checkpoint| {
        if let Some(checkpoint) = checkpoint.borrow_mut().take() {
            checkpoint(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

fn material_checkpoint(path: &Path) {
    #[cfg(test)]
    MATERIAL_CHECKPOINT.with(|checkpoint| {
        if let Some(checkpoint) = checkpoint.borrow_mut().take() {
            checkpoint(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

/// Fixed-depth read observations of the existing native container. An absent
/// suffix is optional store absence, never a failed observation of an existing
/// root/ancestor. Held native descriptors verify named affiliation at output;
/// they do not lease the directory against arbitrary external writers.
struct ReadDirectory {
    held: Vec<(PathBuf, fs::File, fs::Metadata)>,
    missing: Option<PathBuf>,
}
impl ReadDirectory {
    fn capture(store: &RelationRecordStore) -> Result<Self, RelationRecordStoreError> {
        let root = &store.owner_root;
        let directory = store.source_dir();
        let relative = directory
            .strip_prefix(root)
            .map_err(|_| RelationRecordStoreError::UnsafeSource(directory.clone()))?;
        let root_metadata = fs::symlink_metadata(root)?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(RelationRecordStoreError::UnsafeRoot(root.clone()));
        }
        let root_file = crate::file_mutation::directory(root, Path::new(""))?;
        let mut reading = Self {
            held: vec![(root.clone(), root_file, root_metadata)],
            missing: None,
        };
        let mut member = PathBuf::new();
        for component in relative.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(RelationRecordStoreError::UnsafeSource(directory.clone()));
            }
            member.push(component);
            let path = root.join(&member);
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(RelationRecordStoreError::UnsafeSource(path));
                    }
                    let file = crate::file_mutation::directory(root, &member)?;
                    reading.held.push((path, file, metadata));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    reading.missing = Some(path);
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        reading.validate()?;
        Ok(reading)
    }
    fn validate(&self) -> Result<(), RelationRecordStoreError> {
        for (path, file, admitted) in &self.held {
            let named = fs::symlink_metadata(path)?;
            let held = file.metadata()?;
            if named.file_type().is_symlink()
                || !named.is_dir()
                || !held.is_dir()
                || named.dev() != admitted.dev()
                || named.ino() != admitted.ino()
                || held.dev() != admitted.dev()
                || held.ino() != admitted.ino()
            {
                return Err(RelationRecordStoreError::UnsafeSource(path.clone()));
            }
        }
        if let Some(path) = &self.missing {
            match fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => return Err(RelationRecordStoreError::UnsafeSource(path.clone())),
            }
            // Qualify ancestors again after the actual absence observation.
            for (path, file, admitted) in &self.held {
                let named = fs::symlink_metadata(path)?;
                let held = file.metadata()?;
                if named.file_type().is_symlink()
                    || !named.is_dir()
                    || named.dev() != admitted.dev()
                    || named.ino() != admitted.ino()
                    || held.dev() != admitted.dev()
                    || held.ino() != admitted.ino()
                {
                    return Err(RelationRecordStoreError::UnsafeSource(path.clone()));
                }
            }
        }
        Ok(())
    }
}

/// The domain validator for agent-set records, borrowed from
/// [`crate::world::AgentSetRegistry`] by inserting into a fresh registry —
/// schema, membership shape and nothing more; cycle detection is a resolve-
/// time property across records and is asserted by the Actions that resolve.
struct AgentSetRegistryProbe;

impl AgentSetRegistryProbe {
    fn insert(record: AgentSetRecord) -> Result<(), RelationRecordStoreError> {
        let mut registry = crate::world::AgentSetRegistry::default();
        registry
            .insert(record)
            .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))
    }
}

/// The domain validator for world records, borrowed from
/// [`crate::world::WorldGraph`] by inserting into a fresh graph (self-parent
/// rejection and shape; cross-record ancestry cycles are a load-time property).
struct WorldGraphProbe;

impl WorldGraphProbe {
    fn insert(record: WorldRecord) -> Result<(), RelationRecordStoreError> {
        let mut graph = crate::world::WorldGraph::default();
        graph
            .insert(record)
            .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))
    }
}

/// Retains the actual native IO error. Clones share its original object;
/// equality compares diagnostics, not Source identity or effect authority.
/// The public `RelationRecordStoreError::Io` constructor now takes this carrier
/// rather than a String; use `io_error.into()` to preserve a real cause.
#[derive(Debug, Clone)]
pub struct RelationRecordIoError {
    source: Arc<std::io::Error>,
}

impl RelationRecordIoError {
    pub fn io_error(&self) -> &std::io::Error {
        self.source.as_ref()
    }
}
impl From<std::io::Error> for RelationRecordIoError {
    fn from(error: std::io::Error) -> Self {
        Self {
            source: Arc::new(error),
        }
    }
}
impl fmt::Display for RelationRecordIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.io_error(), f)
    }
}
impl PartialEq for RelationRecordIoError {
    fn eq(&self, other: &Self) -> bool {
        self.io_error().kind() == other.io_error().kind()
            && self.io_error().raw_os_error() == other.io_error().raw_os_error()
            && self.io_error().to_string() == other.io_error().to_string()
    }
}
impl Eq for RelationRecordIoError {}
impl Error for RelationRecordIoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.io_error())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationRecordStoreError {
    Io(RelationRecordIoError),
    /// Mechanical eager-metadata capacity; no fabricated IO cause or absence.
    RecordBudget {
        byte_len: u64,
        limit: u64,
    },
    UnsafeRoot(PathBuf),
    UnsafeSource(PathBuf),
    InvalidRecord(String),
    InvalidRef(String),
    NotFound(PathBuf),
    RefMismatch {
        requested: String,
        actual: String,
    },
    SourcePathMismatch {
        ref_: String,
        expected: PathBuf,
        actual: PathBuf,
    },
    AlreadyExists {
        ref_: String,
        revision: String,
    },
    MissingForUpdate {
        ref_: String,
        expected: String,
    },
    RevisionConflict {
        ref_: String,
        expected: String,
        actual: String,
    },
    RevisionNotAdvanced {
        ref_: String,
        revision: String,
    },
}

impl fmt::Display for RelationRecordStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelationRecordStoreError::Io(error) => write!(f, "{error}"),
            RelationRecordStoreError::RecordBudget { byte_len, limit } => {
                write!(f, "Native relation record is {byte_len} bytes, beyond the {limit}-byte eager metadata profile; retained data requires an explicit larger finite owner profile")
            }
            RelationRecordStoreError::UnsafeRoot(path) => {
                write!(f, "unsafe store root {}", path.display())
            }
            RelationRecordStoreError::UnsafeSource(path) => {
                write!(f, "unsafe source path {}", path.display())
            }
            RelationRecordStoreError::InvalidRecord(reason) => write!(f, "{reason}"),
            RelationRecordStoreError::InvalidRef(ref_) => {
                write!(f, "invalid ref `{ref_}`")
            }
            RelationRecordStoreError::NotFound(path) => {
                write!(f, "record not found at {}", path.display())
            }
            RelationRecordStoreError::RefMismatch { requested, actual } => {
                write!(
                    f,
                    "record ref mismatch: requested {requested}, found {actual}"
                )
            }
            RelationRecordStoreError::SourcePathMismatch {
                ref_,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "record {ref_} is stored at {} but its ref resolves to {}",
                    actual.display(),
                    expected.display()
                )
            }
            RelationRecordStoreError::AlreadyExists { ref_, revision } => {
                write!(f, "record {ref_} already exists at revision {revision}; updates require the current revision")
            }
            RelationRecordStoreError::MissingForUpdate { ref_, expected } => {
                write!(
                    f,
                    "record {ref_} is absent; cannot update expected revision {expected}"
                )
            }
            RelationRecordStoreError::RevisionConflict {
                ref_,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "record {ref_} revision conflict: expected {expected}, current {actual}"
                )
            }
            RelationRecordStoreError::RevisionNotAdvanced { ref_, revision } => {
                write!(f, "record {ref_} revision must advance past {revision}")
            }
        }
    }
}

impl RelationRecordStoreError {
    pub fn io_error(&self) -> Option<&std::io::Error> {
        match self {
            Self::Io(error) => Some(error.io_error()),
            _ => None,
        }
    }
}
impl Error for RelationRecordStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.io_error().map(|error| error as &(dyn Error + 'static))
    }
}

impl From<std::io::Error> for RelationRecordStoreError {
    fn from(error: std::io::Error) -> Self {
        RelationRecordStoreError::Io(error.into())
    }
}

fn validate_ref(value: &str) -> Result<(), RelationRecordStoreError> {
    if value.trim().is_empty() || value != value.trim() || value.contains('\0') {
        Err(RelationRecordStoreError::InvalidRef(value.to_owned()))
    } else {
        Ok(())
    }
}

/// FNV-1a 64-bit, hex — the same derivation the AgentProfile store uses.
fn record_key(ref_: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in ref_.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn ensure_directory_path(owner_root: &Path, dir: &Path) -> Result<(), RelationRecordStoreError> {
    let relative = dir
        .strip_prefix(owner_root)
        .map_err(|_| RelationRecordStoreError::UnsafeSource(dir.to_path_buf()))?;
    let mut current = owner_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(RelationRecordStoreError::UnsafeSource(dir.to_path_buf()));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(RelationRecordStoreError::UnsafeSource(current));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_directory_not_symlink(path: &Path) -> Result<(), RelationRecordStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(RelationRecordStoreError::UnsafeSource(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RelationRecordStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| RelationRecordStoreError::UnsafeSource(path.to_path_buf()))?;
    ensure_directory_not_symlink(parent)?;
    let tmp = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("record"),
        record_key(&path.to_string_lossy())
    ));
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempdir;
    use serde_json::{json, Value};

    fn agent_set_record(ref_: &str, revision: &str, members: Value) -> Value {
        json!({
            "schema": AGENT_SET_SCHEMA,
            "ref": ref_,
            "revision": revision,
            "members": members
        })
    }

    #[test]
    fn agent_set_records_round_trip_with_cas_discipline_and_stable_refs() {
        let root = tempdir().unwrap();
        let root = root.path().to_path_buf();
        let store = RelationRecordStore::agent_sets_at_root(root.clone());

        let created = store
            .save(
                &agent_set_record(
                    "control-operators",
                    "r1",
                    json!([
                        {"kind": "agent", "agent_ref": "agent/hermes"}
                    ]),
                ),
                None,
            )
            .unwrap();
        assert!(created.created);
        assert_eq!(created.revision, "r1");
        assert!(created
            .source_path
            .starts_with("Control/agents/agent-sets/"));

        // Create-without-conflict is refused.
        assert!(matches!(
            store.save(
                &agent_set_record("control-operators", "r2", json!([])),
                None
            ),
            Err(RelationRecordStoreError::AlreadyExists { .. })
        ));
        // Update requires the exact current revision.
        assert!(matches!(
            store.save(
                &agent_set_record("control-operators", "r2", json!([])),
                Some("wrong")
            ),
            Err(RelationRecordStoreError::RevisionConflict { .. })
        ));
        // Update must advance the revision and never change the ref.
        let updated = store
            .save(
                &agent_set_record(
                    "control-operators",
                    "r2",
                    json!([
                        {"kind": "agent", "agent_ref": "agent/hermes"},
                        {"kind": "agent", "agent_ref": "agent/picker"}
                    ]),
                ),
                Some("r1"),
            )
            .unwrap();
        assert_eq!(updated.previous_revision.as_deref(), Some("r1"));
        assert!(!updated.created);

        let reading = store.read("control-operators").unwrap();
        assert_eq!(reading.revision, "r2");
        assert_eq!(reading.record["members"].as_array().unwrap().len(), 2);

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);

        // Removal is revision-gated; the referenced agents are untouched.
        assert!(matches!(
            store.remove("control-operators", "r1"),
            Err(RelationRecordStoreError::RevisionConflict { .. })
        ));
        let removed = store.remove("control-operators", "r2").unwrap();
        assert_eq!(removed.ref_, "control-operators");
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn world_records_persist_in_their_own_container_and_validate_shape() {
        let root = tempdir().unwrap();
        let root = root.path().to_path_buf();
        let store = RelationRecordStore::worlds_at_root(root.clone());

        let record = json!({
            "schema": WORLD_RELATION_SCHEMA,
            "ref": "control:root",
            "revision": "w1",
            "sources": []
        });
        let receipt = store.save(&record, None).unwrap();
        assert!(receipt.source_path.starts_with("Control/relations/worlds/"));

        // A self-parenting world is refused through the domain validator.
        let cyclic = json!({
            "schema": WORLD_RELATION_SCHEMA,
            "ref": "project:x",
            "revision": "w1",
            "parent": "project:x",
            "sources": []
        });
        assert!(matches!(
            store.save(&cyclic, None),
            Err(RelationRecordStoreError::InvalidRecord(_))
        ));

        // A wrong schema is refused.
        let wrong = json!({
            "schema": AGENT_SET_SCHEMA,
            "ref": "project:x",
            "revision": "w1"
        });
        assert!(matches!(
            store.save(&wrong, None),
            Err(RelationRecordStoreError::InvalidRecord(_))
        ));
    }

    #[test]
    fn project_store_is_a_fractal_sibling_and_root_and_project_containers_differ() {
        let root = tempdir().unwrap();
        let root = root.path().to_path_buf();
        let project = root.join("Work/garden");
        std::fs::create_dir_all(project.join("ProjectCentral")).unwrap();

        let root_store = RelationRecordStore::agent_sets_at_root(root.clone());
        let project_store = RelationRecordStore::agent_sets_in_project(project.clone());
        assert!(project_store.is_project_scope());
        assert!(!root_store.is_project_scope());

        root_store
            .save(&agent_set_record("squad", "r1", json!([])), None)
            .unwrap();
        // The same ref can exist at both registers without collision: the
        // project variant is its own authored source.
        project_store
            .save(&agent_set_record("squad", "p1", json!([])), None)
            .unwrap();
        assert_eq!(root_store.list().unwrap().len(), 1);
        assert_eq!(project_store.list().unwrap().len(), 1);
        assert!(project_store.list().unwrap()[0]
            .source_path
            .starts_with("ProjectCentral/agents/agent-sets/"));
    }

    struct ReadFixture(PathBuf);
    impl ReadFixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = scratch.join(format!(
                "relation-store-read-{}-{nonce}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for ReadFixture {
        fn drop(&mut self) {
            READ_CHECKPOINT.with(|checkpoint| {
                checkpoint.borrow_mut().take();
            });
            MATERIAL_CHECKPOINT.with(|checkpoint| {
                checkpoint.borrow_mut().take();
            });
            RECORD_OPEN_CHECKPOINT.with(|checkpoint| {
                checkpoint.borrow_mut().take();
            });
            if let Err(error) = fs::remove_dir_all(&self.0) {
                if std::thread::panicking() {
                    eprintln!(
                        "Cannot remove owned relation-store fixture {}: {error}",
                        self.0.display()
                    );
                } else {
                    panic!(
                        "Cannot remove owned relation-store fixture {}: {error}",
                        self.0.display()
                    );
                }
            }
        }
    }
    struct ReadPermissionRestore(PathBuf, fs::Permissions);
    impl Drop for ReadPermissionRestore {
        fn drop(&mut self) {
            if let Err(error) = fs::set_permissions(&self.0, self.1.clone()) {
                if std::thread::panicking() {
                    eprintln!(
                        "Cannot restore owned relation-store permissions {}: {error}",
                        self.0.display()
                    );
                } else {
                    panic!(
                        "Cannot restore owned relation-store permissions {}: {error}",
                        self.0.display()
                    );
                }
            }
        }
    }
    fn native_world_record() -> Value {
        json!({"schema": WORLD_RELATION_SCHEMA, "ref": "control:root", "revision": "w1", "sources": [], "retained_extension": {"opaque": "preserve"}})
    }
    fn native_read_action(root: &Path, action: &str, input: &Value) -> crate::result::ActionResult {
        use crate::action::{create_core_action_registry, ActionExecutionContext};
        use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
        let mut registry = create_core_action_registry();
        crate::agent_set_actions::register_agent_set_actions(&mut registry);
        let options = crate::root::RootOptions {
            explicit_root: Some(root.into()),
            configured_root: None,
            home: None,
        };
        let connectors = ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        registry.execute(
            action,
            input,
            &ActionExecutionContext {
                root_options: &options,
                connectors: &connectors,
                connector_context: &connector_context,
            },
        )
    }

    #[test]
    fn all_native_relation_containers_distinguish_true_optional_absence_without_writes() {
        let fixture = ReadFixture::new();
        let project = fixture.0.join("project");
        fs::create_dir(&project).unwrap();
        for store in [
            RelationRecordStore::agent_sets_at_root(&fixture.0),
            RelationRecordStore::worlds_at_root(&fixture.0),
            RelationRecordStore::agent_sets_in_project(&project),
            RelationRecordStore::worlds_in_project(&project),
        ] {
            assert!(store.list().unwrap().is_empty());
            assert!(matches!(
                store.read("not-authored"),
                Err(RelationRecordStoreError::NotFound(_))
            ));
            assert!(!store.source_dir().exists());
        }
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&project).unwrap().count(), 0);
    }

    #[test]
    fn native_store_refuses_broken_final_and_foreign_ancestor_links_and_restores_exact_records() {
        use std::os::unix::fs::symlink;
        let fixture = ReadFixture::new();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        store.save(&native_world_record(), None).unwrap();
        let record = store.source_path("control:root").unwrap();
        let bytes = fs::read(&record).unwrap();
        let inode = fs::metadata(&record).unwrap().ino();
        let container = store.source_dir();
        let retained = fixture.0.join("retained-worlds");
        fs::rename(&container, &retained).unwrap();
        symlink(fixture.0.join("actually-missing"), &container).unwrap();
        let link_inode = fs::symlink_metadata(&container).unwrap().ino();
        assert!(matches!(
            store.list(),
            Err(RelationRecordStoreError::UnsafeSource(_))
        ));
        assert!(matches!(
            store.load_typed::<WorldRecord>(),
            Err(RelationRecordStoreError::UnsafeSource(_))
        ));
        assert!(matches!(
            store.read("control:root"),
            Err(RelationRecordStoreError::UnsafeSource(_))
        ));
        assert_eq!(fs::symlink_metadata(&container).unwrap().ino(), link_inode);
        fs::remove_file(&container).unwrap();
        fs::rename(&retained, &container).unwrap();
        let control = fixture.0.join("Control");
        let held_control = fixture.0.join("retained-Control");
        fs::rename(&control, &held_control).unwrap();
        symlink(&held_control, &control).unwrap();
        assert!(matches!(
            store.list(),
            Err(RelationRecordStoreError::UnsafeSource(_))
        ));
        assert!(matches!(
            store.read("control:root"),
            Err(RelationRecordStoreError::UnsafeSource(_))
        ));
        fs::remove_file(&control).unwrap();
        fs::rename(&held_control, &control).unwrap();
        let reading = store.read("control:root").unwrap();
        assert_eq!(reading.record["retained_extension"]["opaque"], "preserve");
        assert_eq!(reading.revision, "w1");
        assert_eq!(store.list().unwrap(), vec![reading]);
        assert_eq!(fs::read(&record).unwrap(), bytes);
        assert_eq!(fs::metadata(&record).unwrap().ino(), inode);
    }

    #[test]
    fn actual_native_list_cannot_acknowledge_replaced_container_as_empty() {
        let fixture = ReadFixture::new();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        store.save(&native_world_record(), None).unwrap();
        let record = store.source_path("control:root").unwrap();
        let before = fs::read(&record).unwrap();
        let retained = fixture.0.join("original-worlds");
        let moved = retained.clone();
        READ_CHECKPOINT.with(|checkpoint| {
            *checkpoint.borrow_mut() = Some(Box::new(move |container| {
                fs::rename(container, &moved).unwrap();
                fs::create_dir(container).unwrap();
            }))
        });
        assert!(matches!(
            store.list(),
            Err(RelationRecordStoreError::UnsafeSource(_))
        ));
        fs::remove_dir(store.source_dir()).unwrap();
        fs::rename(&retained, store.source_dir()).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(fs::read(&record).unwrap(), before);
    }

    #[test]
    fn native_store_owner_root_absence_is_actual_io_not_optional_store_absence() {
        let fixture = ReadFixture::new();
        let owner = fixture.0.join("owner");
        fs::create_dir(&owner).unwrap();
        let store = RelationRecordStore::worlds_at_root(&owner);
        store.save(&native_world_record(), None).unwrap();
        let retained = fixture.0.join("retained-owner");
        fs::rename(&owner, &retained).unwrap();
        let actual = fs::symlink_metadata(&owner).unwrap_err();
        let error = store.list().unwrap_err();
        assert_eq!(error.io_error().unwrap().kind(), actual.kind());
        assert_eq!(
            error.io_error().unwrap().raw_os_error(),
            actual.raw_os_error()
        );
        fs::rename(&retained, &owner).unwrap();
        assert_eq!(store.read("control:root").unwrap().revision, "w1");
    }

    #[test]
    fn native_relation_read_eacces_retains_actual_cause_through_clone_action_and_reopen() {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "Actual EACCES proof requires a non-root process"
        );
        let fixture = ReadFixture::new();
        fs::create_dir(fixture.0.join("Work")).unwrap();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        store.save(&native_world_record(), None).unwrap();
        let record = store.source_path("control:root").unwrap();
        let bytes = fs::read(&record).unwrap();
        let inode = fs::metadata(&record).unwrap().ino();
        let container = store.source_dir();
        let restore = ReadPermissionRestore(
            container.clone(),
            fs::metadata(&container).unwrap().permissions(),
        );
        fs::set_permissions(&container, fs::Permissions::from_mode(0o000)).unwrap();
        let actual = fs::read_dir(&container).unwrap_err();
        assert_eq!(actual.kind(), std::io::ErrorKind::PermissionDenied);
        let error = store.list().unwrap_err();
        let original = error.io_error().unwrap();
        assert_eq!(original.kind(), actual.kind());
        assert_eq!(original.raw_os_error(), actual.raw_os_error());
        let cloned = error.clone();
        assert!(std::ptr::eq(original, cloned.io_error().unwrap()));
        assert!(std::ptr::eq(
            original,
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
        ));
        for (action, input) in [
            (
                crate::agent_set_actions::WORLD_RELATIONS_LIST_ACTION,
                json!({"scope":"root"}),
            ),
            (
                crate::agent_set_actions::WORLD_RELATIONS_READ_ACTION,
                json!({"scope":"root", "ref":"control:root"}),
            ),
            (
                crate::agent_set_actions::WORLD_EFFECTIVE_SOURCES_ACTION,
                json!({"scope":"root", "world_ref":"control:root"}),
            ),
        ] {
            let result = native_read_action(&fixture.0, action, &input);
            assert!(!result.ok, "{result:?}");
            let details = result.error.as_ref().unwrap().details.as_ref().unwrap();
            assert_eq!(details["effects"], "none");
            assert_eq!(details["io_error"]["kind"], "PermissionDenied");
            assert_eq!(
                details["io_error"]["raw_os_error"],
                actual.raw_os_error().unwrap()
            );
        }
        // The same shared formatter must not assert effects:none for writers.
        let failed_write = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_RELATIONS_SAVE_ACTION,
            &json!({"scope":"root", "record":native_world_record(), "expected_revision":"w1"}),
        );
        assert!(!failed_write.ok, "{failed_write:?}");
        assert!(failed_write.error.as_ref().unwrap().details.is_none());
        drop(restore);
        assert_eq!(store.read("control:root").unwrap().revision, "w1");
        assert_eq!(fs::read(&record).unwrap(), bytes);
        assert_eq!(fs::metadata(&record).unwrap().ino(), inode);
    }

    #[test]
    fn actual_native_record_open_refuses_final_symlink_and_fifo_substitution_without_foreign_body()
    {
        use std::os::unix::fs::symlink;
        for list in [false, true] {
            for fifo in [false, true] {
                let fixture = ReadFixture::new();
                let store = RelationRecordStore::worlds_at_root(&fixture.0);
                store.save(&native_world_record(), None).unwrap();
                let path = store.source_path("control:root").unwrap();
                let before = fs::read(&path).unwrap();
                let original_inode = fs::metadata(&path).unwrap().ino();
                let retained = fixture.0.join("retained-record.json");
                let foreign = fixture.0.join("foreign-record.json");
                fs::write(&foreign, b"foreign private body is never a native record").unwrap();
                let foreign_before = fs::read(&foreign).unwrap();
                let moved = retained.clone();
                let target = foreign.clone();
                RECORD_OPEN_CHECKPOINT.with(|checkpoint| {
                    *checkpoint.borrow_mut() = Some(Box::new(move |member| {
                        fs::rename(member, &moved).unwrap();
                        if fifo {
                            let name =
                                std::ffi::CString::new(member.as_os_str().as_encoded_bytes())
                                    .unwrap();
                            assert_eq!(
                                unsafe { libc::mkfifo(name.as_ptr(), 0o600) },
                                0,
                                "{}",
                                std::io::Error::last_os_error()
                            );
                        } else {
                            symlink(&target, member).unwrap();
                        }
                    }))
                });
                let started = std::time::Instant::now();
                let result = if list {
                    store.list().map(|_| ())
                } else {
                    store.read("control:root").map(|_| ())
                };
                assert!(result.is_err(), "{result:?}");
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(2),
                    "Native record capture must refuse FIFO without blocking"
                );
                assert_eq!(fs::read(&retained).unwrap(), before);
                assert_eq!(fs::metadata(&retained).unwrap().ino(), original_inode);
                assert_eq!(fs::read(&foreign).unwrap(), foreign_before);
                fs::remove_file(&path).unwrap();
                fs::rename(&retained, &path).unwrap();
                assert_eq!(store.read("control:root").unwrap().revision, "w1");
                assert_eq!(fs::read(&path).unwrap(), before);
            }
        }
    }

    #[test]
    fn actual_native_record_final_acknowledgement_refuses_removal_replacement_and_same_inode_change(
    ) {
        for list in [false, true] {
            for mode in 0..3 {
                let fixture = ReadFixture::new();
                let store = RelationRecordStore::worlds_at_root(&fixture.0);
                store.save(&native_world_record(), None).unwrap();
                let path = store.source_path("control:root").unwrap();
                let before = fs::read(&path).unwrap();
                let inode = fs::metadata(&path).unwrap().ino();
                let retained = fixture.0.join("retained-record.json");
                let changed_path = path.clone();
                let moved = retained.clone();
                let replacement = before.clone();
                MATERIAL_CHECKPOINT.with(|checkpoint| {
                    *checkpoint.borrow_mut() = Some(Box::new(move |_| {
                        if mode == 2 {
                            let mut changed: Value = serde_json::from_slice(&replacement).unwrap();
                            changed["revision"] = json!("w2");
                            fs::write(&changed_path, serde_json::to_vec(&changed).unwrap())
                                .unwrap();
                        } else {
                            fs::rename(&changed_path, &moved).unwrap();
                            if mode == 1 {
                                fs::write(&changed_path, &replacement).unwrap();
                            }
                        }
                    }))
                });
                let result = if list {
                    store.list().map(|_| ())
                } else {
                    store.read("control:root").map(|_| ())
                };
                assert!(result.is_err(), "{result:?}");
                if mode == 2 {
                    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
                    assert_eq!(store.read("control:root").unwrap().revision, "w2");
                    fs::write(&path, &before).unwrap();
                } else {
                    assert_eq!(fs::read(&retained).unwrap(), before);
                    assert_eq!(fs::metadata(&retained).unwrap().ino(), inode);
                    if mode == 1 {
                        // A fresh observation can admit the legitimate replacement;
                        // the earlier captured physical basis cannot acknowledge it.
                        assert_eq!(store.read("control:root").unwrap().revision, "w1");
                        fs::remove_file(&path).unwrap();
                    }
                    fs::rename(&retained, &path).unwrap();
                }
                let fresh = store.read("control:root").unwrap();
                assert_eq!(fresh.revision, "w1");
                assert_eq!(store.list().unwrap(), vec![fresh]);
                assert_eq!(fs::read(&path).unwrap(), before);
                assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
            }
        }
    }

    #[test]
    fn actual_native_relation_metadata_profile_preserves_exact_capacity_and_larger_retained_source()
    {
        let fixture = ReadFixture::new();
        fs::create_dir(fixture.0.join("Work")).unwrap();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        store.save(&native_world_record(), None).unwrap();
        let path = store.source_path("control:root").unwrap();
        let mut record = native_world_record();
        record["retained_padding"] = json!("");
        let overhead = serde_json::to_vec(&record).unwrap().len();
        record["retained_padding"] = json!("x".repeat(RELATION_RECORD_METADATA_LIMIT - overhead));
        let mut bytes = serde_json::to_vec(&record).unwrap();
        assert_eq!(bytes.len(), RELATION_RECORD_METADATA_LIMIT);
        fs::write(&path, &bytes).unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        let reading = store.read("control:root").unwrap();
        assert_eq!(reading.record, record);
        assert_eq!(reading.revision, "w1");
        assert_eq!(store.list().unwrap(), vec![reading]);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        bytes.push(b'\n'); // Valid retained JSON, one byte over this eager profile.
        fs::write(&path, &bytes).unwrap();
        for result in [
            store.read("control:root").map(|_| ()),
            store.list().map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(RelationRecordStoreError::RecordBudget { byte_len, limit })
                if byte_len == bytes.len() as u64 && limit == RELATION_RECORD_METADATA_LIMIT as u64)
            );
        }
        let result = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_RELATIONS_READ_ACTION,
            &json!({"scope":"root","ref":"control:root"}),
        );
        assert_eq!(
            result.status,
            crate::result::ResultStatus::UnavailableCapability
        );
        let error = result.error.unwrap();
        assert_eq!(error.code, "central.relation_record_budget");
        let details = error.details.unwrap();
        assert_eq!(details["effects"], "none");
        assert!(details["io_error"].is_null());
        assert_eq!(details["capacity"]["byte_len"], bytes.len() as u64);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        record["revision"] = json!("w2");
        // Existing larger source is retained even when a candidate is within
        // the default profile: it cannot be silently parsed or rewritten.
        record["retained_padding"] = json!("");
        assert!(matches!(
            store.save(&record, Some("w1")),
            Err(RelationRecordStoreError::RecordBudget { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    }

    #[test]
    fn actual_oversized_native_save_refuses_before_any_directory_or_source_effect() {
        let fixture = ReadFixture::new();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        let mut record = native_world_record();
        record["retained_padding"] = json!("x".repeat(RELATION_RECORD_METADATA_LIMIT));
        let mut serialized = serde_json::to_vec_pretty(&record).unwrap();
        serialized.push(b'\n');
        assert!(
            matches!(store.save(&record, None), Err(RelationRecordStoreError::RecordBudget { byte_len, limit })
            if byte_len == serialized.len() as u64 && limit == RELATION_RECORD_METADATA_LIMIT as u64)
        );
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
        store.save(&native_world_record(), None).unwrap();
        let path = store.source_path("control:root").unwrap();
        let bytes = fs::read(&path).unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        record["revision"] = json!("w2");
        assert!(matches!(
            store.save(&record, Some("w1")),
            Err(RelationRecordStoreError::RecordBudget { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        fs::create_dir(fixture.0.join("Work")).unwrap();
        let refused = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_RELATIONS_SAVE_ACTION,
            &json!({"scope":"root","record":record,"expected_revision":"w1"}),
        );
        assert!(!refused.ok, "{refused:?}");
        assert_eq!(refused.status, crate::result::ResultStatus::InvalidInput);
        assert!(
            refused.error.as_ref().unwrap().details.is_none(),
            "Shared mutation failures must not gain a blanket effects:none"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn actual_native_complete_world_list_refuses_new_exclusion_and_fresh_owner_resolves_it() {
        let fixture = ReadFixture::new();
        fs::create_dir(fixture.0.join("Work")).unwrap();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        let sealed = "central:source:control:root:sealed";
        let mut root = native_world_record();
        root["sources"] = json!([{ "ref": sealed, "revision": "s1",
            "authority": "human-authored", "treatment": "canonical" }]);
        store.save(&root, None).unwrap();
        let root_path = store.source_path("control:root").unwrap();
        let root_bytes = fs::read(&root_path).unwrap();
        let root_inode = fs::metadata(&root_path).unwrap().ino();
        let mut child = native_world_record();
        child["ref"] = json!("project:sealed-garden");
        child["parent"] = json!("control:root");
        child["excluded_sources"] = json!([sealed]);
        let child_path = store.source_path("project:sealed-garden").unwrap();
        let checkpoint_store = store.clone();
        MATERIAL_CHECKPOINT.with(|checkpoint| {
            *checkpoint.borrow_mut() = Some(Box::new(move |dir| {
                assert_eq!(dir, checkpoint_store.source_dir());
                checkpoint_store.save(&child, None).unwrap();
            }))
        });
        let result = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_RELATIONS_LIST_ACTION,
            &json!({"scope":"root"}),
        );
        assert!(!result.ok, "stale complete list acknowledged: {result:?}");
        assert_eq!(result.status, crate::result::ResultStatus::InternalFailure);
        let details = result.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["effects"], "none");
        assert!(details["io_error"]["raw_os_error"].is_null());
        assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
        assert_eq!(fs::metadata(&root_path).unwrap().ino(), root_inode);
        let child_bytes = fs::read(&child_path).unwrap();
        let child_inode = fs::metadata(&child_path).unwrap().ino();
        assert_eq!(store.list().unwrap().len(), 2);
        let fresh = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_EFFECTIVE_SOURCES_ACTION,
            &json!({"scope":"root", "world_ref":"project:sealed-garden"}),
        );
        assert!(fresh.ok, "{fresh:?}");
        let actual = fresh.data.as_ref().unwrap()["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["ref"] == sealed)
            .unwrap();
        assert_eq!(actual["state"], "excluded");
        assert_eq!(actual["propagation_path"].as_array().unwrap().len(), 2);
        assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
        assert_eq!(fs::metadata(&root_path).unwrap().ino(), root_inode);
        assert_eq!(fs::read(&child_path).unwrap(), child_bytes);
        assert_eq!(fs::metadata(&child_path).unwrap().ino(), child_inode);
    }

    #[test]
    fn actual_native_empty_existing_store_cannot_acknowledge_new_member_as_empty() {
        let fixture = ReadFixture::new();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        let record = native_world_record();
        store.save(&record, None).unwrap();
        store.remove("control:root", "w1").unwrap();
        let directory = store.source_dir();
        let directory_inode = fs::metadata(&directory).unwrap().ino();
        assert!(store.list().unwrap().is_empty());
        let checkpoint_store = store.clone();
        MATERIAL_CHECKPOINT.with(|checkpoint| {
            *checkpoint.borrow_mut() = Some(Box::new(move |dir| {
                assert_eq!(dir, checkpoint_store.source_dir());
                checkpoint_store.save(&record, None).unwrap();
            }))
        });
        let error = store
            .list()
            .expect_err("new record cannot be acknowledged as empty");
        assert_eq!(error.io_error().unwrap().kind(), std::io::ErrorKind::Other);
        assert!(error.io_error().unwrap().raw_os_error().is_none());
        assert_eq!(fs::metadata(&directory).unwrap().ino(), directory_inode);
        let path = store.source_path("control:root").unwrap();
        let bytes = fs::read(&path).unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        let fresh = store.list().unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].ref_, "control:root");
        assert_eq!(fresh[0].revision, "w1");
        assert_eq!(fresh[0].record["retained_extension"]["opaque"], "preserve");
        assert_eq!(store.load_typed::<WorldRecord>().unwrap().len(), 1);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    }

    #[test]
    fn actual_native_membership_final_pass_preserves_ignored_non_json_and_refuses_new_unsafe_entries(
    ) {
        use std::os::unix::fs::symlink;
        for unsafe_entry in [false, true] {
            for form in 0..3 {
                let fixture = ReadFixture::new();
                let store = RelationRecordStore::worlds_at_root(&fixture.0);
                store.save(&native_world_record(), None).unwrap();
                let path = store.source_path("control:root").unwrap();
                let bytes = fs::read(&path).unwrap();
                let inode = fs::metadata(&path).unwrap().ino();
                let foreign = fixture.0.join("unselected-target");
                fs::write(&foreign, b"unselected body is retained").unwrap();
                let foreign_bytes = fs::read(&foreign).unwrap();
                let foreign_inode = fs::metadata(&foreign).unwrap().ino();
                let name = if unsafe_entry {
                    if form == 0 {
                        "unselected.link"
                    } else {
                        "added.json"
                    }
                } else {
                    match form {
                        0 => "README",
                        1 => "ordinary-directory",
                        _ => "ordinary.fifo",
                    }
                };
                let entry = store.source_dir().join(name);
                let changed_entry = entry.clone();
                let target = foreign.clone();
                MATERIAL_CHECKPOINT.with(|checkpoint| {
                    *checkpoint.borrow_mut() = Some(Box::new(move |_| {
                        if unsafe_entry && form == 0 {
                            symlink(&target, &changed_entry).unwrap();
                        } else if (form == 2 && !unsafe_entry) || (form == 1 && unsafe_entry) {
                            let name = std::ffi::CString::new(
                                changed_entry.as_os_str().as_encoded_bytes(),
                            )
                            .unwrap();
                            assert_eq!(
                                unsafe { libc::mkfifo(name.as_ptr(), 0o600) },
                                0,
                                "{}",
                                std::io::Error::last_os_error()
                            );
                        } else if (form == 1 && !unsafe_entry) || (form == 2 && unsafe_entry) {
                            fs::create_dir(&changed_entry).unwrap();
                        } else {
                            fs::write(&changed_entry, b"ordinary non-JSON material").unwrap();
                        }
                    }))
                });
                let started = std::time::Instant::now();
                let result = store.list();
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(2),
                    "new special entry must not block native membership observation"
                );
                if unsafe_entry {
                    assert!(
                        matches!(result, Err(RelationRecordStoreError::UnsafeSource(ref actual))
                        if actual == &entry),
                        "{result:?}"
                    );
                } else {
                    let readings = result.unwrap();
                    assert_eq!(readings.len(), 1);
                    assert_eq!(readings[0].ref_, "control:root");
                }
                assert_eq!(fs::read(&path).unwrap(), bytes);
                assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
                assert_eq!(fs::read(&foreign).unwrap(), foreign_bytes);
                assert_eq!(fs::metadata(&foreign).unwrap().ino(), foreign_inode);
                if fs::symlink_metadata(&entry).unwrap().is_dir() {
                    fs::remove_dir(&entry).unwrap();
                } else {
                    fs::remove_file(&entry).unwrap();
                }
                assert_eq!(store.list().unwrap().len(), 1);
                assert_eq!(fs::read(&path).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn actual_native_final_membership_enumeration_io_retains_original_permission_error() {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "real native directory EACCES qualification requires a nonroot process"
        );
        let fixture = ReadFixture::new();
        fs::create_dir(fixture.0.join("Work")).unwrap();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        // Native create/remove retains a real ordinary empty container. With
        // zero material bases, the failure is the actual final enumeration.
        store.save(&native_world_record(), None).unwrap();
        store.remove("control:root", "w1").unwrap();
        let directory = store.source_dir();
        let inode = fs::metadata(&directory).unwrap().ino();
        let restore = ReadPermissionRestore(
            directory.clone(),
            fs::metadata(&directory).unwrap().permissions(),
        );
        MATERIAL_CHECKPOINT.with(|checkpoint| {
            *checkpoint.borrow_mut() = Some(Box::new(move |dir| {
                fs::set_permissions(dir, fs::Permissions::from_mode(0o111)).unwrap();
            }))
        });
        let error = store.list().unwrap_err();
        let actual = fs::read_dir(&directory).unwrap_err();
        assert_eq!(actual.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(error.io_error().unwrap().kind(), actual.kind());
        assert_eq!(
            error.io_error().unwrap().raw_os_error(),
            actual.raw_os_error()
        );
        let result = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_RELATIONS_LIST_ACTION,
            &json!({"scope":"root"}),
        );
        assert!(!result.ok, "{result:?}");
        let details = result.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["effects"], "none");
        assert_eq!(details["io_error"]["kind"], "PermissionDenied");
        assert_eq!(
            details["io_error"]["raw_os_error"],
            actual.raw_os_error().unwrap()
        );
        assert_eq!(fs::metadata(&directory).unwrap().ino(), inode);
        drop(restore);
        assert!(store.list().unwrap().is_empty());
        store.save(&native_world_record(), None).unwrap();
        assert_eq!(store.read("control:root").unwrap().revision, "w1");
    }

    #[test]
    fn actual_native_new_unreadable_json_is_not_false_absence_and_fresh_read_has_actual_errno() {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "real native record EACCES qualification requires a nonroot process"
        );
        let fixture = ReadFixture::new();
        fs::create_dir(fixture.0.join("Work")).unwrap();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        store.save(&native_world_record(), None).unwrap();
        let root = store.source_path("control:root").unwrap();
        let root_bytes = fs::read(&root).unwrap();
        let root_inode = fs::metadata(&root).unwrap().ino();
        let mut added = native_world_record();
        added["ref"] = json!("project:unreadable-record");
        added["parent"] = json!("control:root");
        let path = store.source_path("project:unreadable-record").unwrap();
        let original_permissions = std::rc::Rc::new(std::cell::RefCell::new(None));
        let checkpoint_permissions = original_permissions.clone();
        let checkpoint_store = store.clone();
        let checkpoint_path = path.clone();
        MATERIAL_CHECKPOINT.with(|checkpoint| {
            *checkpoint.borrow_mut() = Some(Box::new(move |_| {
                checkpoint_store.save(&added, None).unwrap();
                *checkpoint_permissions.borrow_mut() =
                    Some(fs::metadata(&checkpoint_path).unwrap().permissions());
                fs::set_permissions(&checkpoint_path, fs::Permissions::from_mode(0o000)).unwrap();
            }))
        });
        let result = store.list();
        let restore = ReadPermissionRestore(
            path.clone(),
            original_permissions.borrow_mut().take().unwrap(),
        );
        let error = result.unwrap_err();
        assert_eq!(
            error.io_error().unwrap().kind(),
            std::io::ErrorKind::Other,
            "observed membership change is not a fictional read of the new body"
        );
        assert!(error.io_error().unwrap().raw_os_error().is_none());
        let actual = fs::File::open(&path).unwrap_err();
        assert_eq!(actual.kind(), std::io::ErrorKind::PermissionDenied);
        let fresh = store.list().unwrap_err();
        assert_eq!(fresh.io_error().unwrap().kind(), actual.kind());
        assert_eq!(
            fresh.io_error().unwrap().raw_os_error(),
            actual.raw_os_error()
        );
        let action = native_read_action(
            &fixture.0,
            crate::agent_set_actions::WORLD_RELATIONS_READ_ACTION,
            &json!({"scope":"root", "ref":"project:unreadable-record"}),
        );
        assert!(!action.ok, "{action:?}");
        let details = action.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["effects"], "none");
        assert_eq!(
            details["io_error"]["raw_os_error"],
            actual.raw_os_error().unwrap()
        );
        assert_eq!(fs::read(&root).unwrap(), root_bytes);
        assert_eq!(fs::metadata(&root).unwrap().ino(), root_inode);
        drop(restore);
        let bytes = fs::read(&path).unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        assert_eq!(store.list().unwrap().len(), 2);
        assert_eq!(
            store.read("project:unreadable-record").unwrap().revision,
            "w1"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        assert_eq!(fs::read(&root).unwrap(), root_bytes);
        assert_eq!(fs::metadata(&root).unwrap().ino(), root_inode);
    }

    #[test]
    fn actual_native_unchanged_complete_list_preserves_records_and_unknown_extensions() {
        let fixture = ReadFixture::new();
        let store = RelationRecordStore::worlds_at_root(&fixture.0);
        store.save(&native_world_record(), None).unwrap();
        let mut child = native_world_record();
        child["ref"] = json!("project:stable-garden");
        child["parent"] = json!("control:root");
        child["retained_extension"] = json!({"opaque":"child-preserve", "inner":[2,1]});
        store.save(&child, None).unwrap();
        let root_path = store.source_path("control:root").unwrap();
        let child_path = store.source_path("project:stable-garden").unwrap();
        let root_bytes = fs::read(&root_path).unwrap();
        let child_bytes = fs::read(&child_path).unwrap();
        let root_inode = fs::metadata(&root_path).unwrap().ino();
        let child_inode = fs::metadata(&child_path).unwrap().ino();
        let first = store.list().unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].ref_, "control:root");
        assert_eq!(first[1].ref_, "project:stable-garden");
        assert_eq!(first[0].record["retained_extension"]["opaque"], "preserve");
        assert_eq!(
            first[1].record["retained_extension"]["inner"],
            json!([2, 1])
        );
        assert_eq!(first[0], store.read("control:root").unwrap());
        assert_eq!(first[1], store.read("project:stable-garden").unwrap());
        assert_eq!(store.list().unwrap(), first);
        assert_eq!(store.load_typed::<WorldRecord>().unwrap().len(), 2);
        assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
        assert_eq!(fs::read(&child_path).unwrap(), child_bytes);
        assert_eq!(fs::metadata(&root_path).unwrap().ino(), root_inode);
        assert_eq!(fs::metadata(&child_path).unwrap().ino(), child_inode);
    }
}
