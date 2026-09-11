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
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::world::{AgentSetRecord, WorldRecord, AGENT_SET_SCHEMA, WORLD_RELATION_SCHEMA};

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
        let record = self.read_record(&path, ref_)?;
        Ok(record)
    }

    pub fn list(&self) -> Result<Vec<RelationRecordReading>, RelationRecordStoreError> {
        self.validate_root()?;
        let dir = self.source_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        ensure_directory_not_symlink(&dir)?;
        let mut readings = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(RelationRecordStoreError::UnsafeSource(path));
            }
            if !metadata.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let record = self.parse_record(&path)?;
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

        let mut bytes = serde_json::to_vec_pretty(record)
            .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))?;
        bytes.push(b'\n');
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
        Ok(self
            .list()?
            .into_iter()
            .map(|reading| {
                serde_json::from_value(reading.record)
                    .map_err(|error| RelationRecordStoreError::InvalidRecord(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?)
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
    ) -> Result<RelationRecordReading, RelationRecordStoreError> {
        let record = self.parse_record(path)?;
        let ref_ = record
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
        let revision = record
            .get("revision")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned();
        Ok(RelationRecordReading {
            kind: self.kind,
            ref_: ref_.to_owned(),
            revision,
            source_path: relative(&self.owner_root, path),
            record,
        })
    }

    fn parse_record(&self, path: &Path) -> Result<serde_json::Value, RelationRecordStoreError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RelationRecordStoreError::NotFound(path.to_path_buf())
            } else {
                RelationRecordStoreError::Io(error.to_string())
            }
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RelationRecordStoreError::UnsafeSource(path.to_path_buf()));
        }
        let record: serde_json::Value = serde_json::from_slice(&fs::read(path)?)
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
        Ok(record)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationRecordStoreError {
    Io(String),
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

impl Error for RelationRecordStoreError {}

impl From<std::io::Error> for RelationRecordStoreError {
    fn from(error: std::io::Error) -> Self {
        RelationRecordStoreError::Io(error.to_string())
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
                        {"kind": "agent", "agent_ref": "agent:hermes"}
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
                        {"kind": "agent", "agent_ref": "agent:hermes"},
                        {"kind": "agent", "agent_ref": "agent:picker"}
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
        assert!(!project_store.is_project_scope() == false);
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
}
