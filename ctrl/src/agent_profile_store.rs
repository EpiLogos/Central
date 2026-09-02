use crate::agent_profile::{AgentProfile, AgentProfileScope, AGENT_PROFILE_SCHEMA};
use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const ROOT_AGENT_PROFILE_DIR: &str = "Control/agents/profiles";
pub const PROJECT_AGENT_PROFILE_DIR: &str = "ProjectCentral/agents/profiles";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentProfileReading {
    pub profile: AgentProfile,
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentProfileWriteReceipt {
    pub profile_ref: String,
    pub previous_revision: Option<String>,
    pub revision: String,
    pub source_path: String,
    pub created: bool,
}

/// Filesystem source owner for one Central scope. `owner_root` is the Central root
/// for personal profiles and the Project root for Project profiles. The profile's
/// stable ref is stored in the document and never inferred from the filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfileStore {
    owner_root: PathBuf,
    scope: AgentProfileScope,
}

impl AgentProfileStore {
    pub fn personal(central_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: central_root.into(),
            scope: AgentProfileScope::Personal,
        }
    }

    pub fn project(project_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: project_root.into(),
            scope: AgentProfileScope::Project,
        }
    }

    pub fn scope(&self) -> AgentProfileScope {
        self.scope
    }

    pub fn source_dir(&self) -> PathBuf {
        self.owner_root.join(match self.scope {
            AgentProfileScope::Personal => ROOT_AGENT_PROFILE_DIR,
            AgentProfileScope::Project => PROJECT_AGENT_PROFILE_DIR,
        })
    }

    pub fn source_path(&self, profile_ref: &str) -> Result<PathBuf, AgentProfileStoreError> {
        validate_ref(profile_ref)?;
        Ok(self
            .source_dir()
            .join(format!("profile-{}.json", profile_key(profile_ref))))
    }

    pub fn read(&self, profile_ref: &str) -> Result<AgentProfileReading, AgentProfileStoreError> {
        self.validate_root()?;
        let path = self.source_path(profile_ref)?;
        let profile = read_profile_file(&path)?;
        self.validate_loaded(profile_ref, &profile)?;
        Ok(AgentProfileReading {
            source_path: relative(&self.owner_root, &path),
            profile,
        })
    }

    pub fn list(&self) -> Result<Vec<AgentProfileReading>, AgentProfileStoreError> {
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
                return Err(AgentProfileStoreError::UnsafeSource(path));
            }
            if !metadata.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let profile = read_profile_file(&path)?;
            self.validate_loaded(&profile.profile_ref, &profile)?;
            let expected = self.source_path(&profile.profile_ref)?;
            if expected != path {
                return Err(AgentProfileStoreError::SourcePathMismatch {
                    profile_ref: profile.profile_ref,
                    expected,
                    actual: path,
                });
            }
            readings.push(AgentProfileReading {
                source_path: relative(&self.owner_root, &expected),
                profile,
            });
        }
        readings.sort_by(|left, right| left.profile.profile_ref.cmp(&right.profile.profile_ref));
        Ok(readings)
    }

    /// Save authored source under compare-and-swap revision discipline.
    ///
    /// - create: `expected_revision = None`, source must not already exist;
    /// - update: `expected_revision = Some(current)`, source must exist and match;
    /// - every update must advance the authored profile revision.
    pub fn save(
        &self,
        profile: &AgentProfile,
        expected_revision: Option<&str>,
    ) -> Result<AgentProfileWriteReceipt, AgentProfileStoreError> {
        self.validate_root()?;
        if profile.scope != self.scope {
            return Err(AgentProfileStoreError::ScopeMismatch {
                store: self.scope,
                profile: profile.scope,
            });
        }
        validate_ref(&profile.profile_ref)?;
        if profile.schema != AGENT_PROFILE_SCHEMA {
            return Err(AgentProfileStoreError::InvalidProfile(format!(
                "unsupported AgentProfile schema {}",
                profile.schema
            )));
        }
        if profile.revision.trim().is_empty() {
            return Err(AgentProfileStoreError::InvalidProfile(
                "AgentProfile revision cannot be empty".into(),
            ));
        }

        let dir = self.source_dir();
        ensure_directory_path(&self.owner_root, &dir)?;
        let path = self.source_path(&profile.profile_ref)?;
        let existing = if path.exists() {
            Some(read_profile_file(&path)?)
        } else {
            None
        };

        let (previous_revision, created) = match (existing.as_ref(), expected_revision) {
            (None, None) => (None, true),
            (None, Some(expected)) => {
                return Err(AgentProfileStoreError::MissingForUpdate {
                    profile_ref: profile.profile_ref.clone(),
                    expected: expected.to_owned(),
                })
            }
            (Some(current), None) => {
                return Err(AgentProfileStoreError::AlreadyExists {
                    profile_ref: profile.profile_ref.clone(),
                    revision: current.revision.clone(),
                })
            }
            (Some(current), Some(expected)) if current.revision != expected => {
                return Err(AgentProfileStoreError::RevisionConflict {
                    profile_ref: profile.profile_ref.clone(),
                    expected: expected.to_owned(),
                    actual: current.revision.clone(),
                })
            }
            (Some(current), Some(_)) => {
                self.validate_loaded(&profile.profile_ref, current)?;
                if current.agent_ref != profile.agent_ref {
                    return Err(AgentProfileStoreError::AgentIdentityChanged {
                        profile_ref: profile.profile_ref.clone(),
                        expected_agent: current.agent_ref.clone(),
                        actual_agent: profile.agent_ref.clone(),
                    });
                }
                if current.revision == profile.revision {
                    return Err(AgentProfileStoreError::RevisionNotAdvanced {
                        profile_ref: profile.profile_ref.clone(),
                        revision: profile.revision.clone(),
                    });
                }
                (Some(current.revision.clone()), false)
            }
        };

        let mut bytes = serde_json::to_vec_pretty(profile)
            .map_err(|error| AgentProfileStoreError::InvalidProfile(error.to_string()))?;
        bytes.push(b'\n');
        atomic_write(&path, &bytes)?;

        Ok(AgentProfileWriteReceipt {
            profile_ref: profile.profile_ref.clone(),
            previous_revision,
            revision: profile.revision.clone(),
            source_path: relative(&self.owner_root, &path),
            created,
        })
    }

    /// Remove only the authored profile relation. The referenced Agent identity and
    /// any AIKit/Actuation/Workcell state are outside this store and untouched.
    pub fn remove(
        &self,
        profile_ref: &str,
        expected_revision: &str,
    ) -> Result<AgentProfileReading, AgentProfileStoreError> {
        let reading = self.read(profile_ref)?;
        if reading.profile.revision != expected_revision {
            return Err(AgentProfileStoreError::RevisionConflict {
                profile_ref: profile_ref.to_owned(),
                expected: expected_revision.to_owned(),
                actual: reading.profile.revision.clone(),
            });
        }
        let path = self.source_path(profile_ref)?;
        fs::remove_file(path)?;
        Ok(reading)
    }

    fn validate_root(&self) -> Result<(), AgentProfileStoreError> {
        let metadata = fs::symlink_metadata(&self.owner_root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AgentProfileStoreError::UnsafeRoot(self.owner_root.clone()));
        }
        Ok(())
    }

    fn validate_loaded(
        &self,
        requested_ref: &str,
        profile: &AgentProfile,
    ) -> Result<(), AgentProfileStoreError> {
        if profile.schema != AGENT_PROFILE_SCHEMA {
            return Err(AgentProfileStoreError::InvalidProfile(format!(
                "unsupported AgentProfile schema {}",
                profile.schema
            )));
        }
        if profile.scope != self.scope {
            return Err(AgentProfileStoreError::ScopeMismatch {
                store: self.scope,
                profile: profile.scope,
            });
        }
        if profile.profile_ref != requested_ref {
            return Err(AgentProfileStoreError::RefMismatch {
                requested: requested_ref.to_owned(),
                actual: profile.profile_ref.clone(),
            });
        }
        validate_ref(&profile.profile_ref)?;
        Ok(())
    }
}

fn read_profile_file(path: &Path) -> Result<AgentProfile, AgentProfileStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AgentProfileStoreError::NotFound(path.to_path_buf())
        } else {
            AgentProfileStoreError::Io(error.to_string())
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AgentProfileStoreError::UnsafeSource(path.to_path_buf()));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| AgentProfileStoreError::InvalidProfile(error.to_string()))
}

fn validate_ref(value: &str) -> Result<(), AgentProfileStoreError> {
    if value.trim().is_empty() || value != value.trim() || value.contains('\0') {
        Err(AgentProfileStoreError::InvalidProfileRef(value.to_owned()))
    } else {
        Ok(())
    }
}

fn profile_key(profile_ref: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in profile_ref.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn ensure_directory_path(owner_root: &Path, dir: &Path) -> Result<(), AgentProfileStoreError> {
    let relative = dir
        .strip_prefix(owner_root)
        .map_err(|_| AgentProfileStoreError::UnsafeSource(dir.to_path_buf()))?;
    let mut current = owner_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(AgentProfileStoreError::UnsafeSource(dir.to_path_buf()));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(AgentProfileStoreError::UnsafeSource(current));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_directory_not_symlink(path: &Path) -> Result<(), AgentProfileStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(AgentProfileStoreError::UnsafeSource(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AgentProfileStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| AgentProfileStoreError::UnsafeSource(path.to_path_buf()))?;
    ensure_directory_not_symlink(parent)?;
    let tmp = parent.join(format!(".agent-profile-{}.tmp", profile_key(&path.to_string_lossy())));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProfileStoreError {
    Io(String),
    UnsafeRoot(PathBuf),
    UnsafeSource(PathBuf),
    InvalidProfile(String),
    InvalidProfileRef(String),
    ScopeMismatch {
        store: AgentProfileScope,
        profile: AgentProfileScope,
    },
    NotFound(PathBuf),
    RefMismatch {
        requested: String,
        actual: String,
    },
    SourcePathMismatch {
        profile_ref: String,
        expected: PathBuf,
        actual: PathBuf,
    },
    AlreadyExists {
        profile_ref: String,
        revision: String,
    },
    MissingForUpdate {
        profile_ref: String,
        expected: String,
    },
    RevisionConflict {
        profile_ref: String,
        expected: String,
        actual: String,
    },
    RevisionNotAdvanced {
        profile_ref: String,
        revision: String,
    },
    AgentIdentityChanged {
        profile_ref: String,
        expected_agent: String,
        actual_agent: String,
    },
}

impl From<std::io::Error> for AgentProfileStoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl fmt::Display for AgentProfileStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => formatter.write_str(error),
            Self::UnsafeRoot(path) => write!(formatter, "AgentProfile owner root is unsafe: {}", path.display()),
            Self::UnsafeSource(path) => write!(formatter, "AgentProfile source path is unsafe: {}", path.display()),
            Self::InvalidProfile(error) => write!(formatter, "invalid AgentProfile source: {error}"),
            Self::InvalidProfileRef(value) => write!(formatter, "invalid AgentProfile ref {value:?}"),
            Self::ScopeMismatch { store, profile } => write!(formatter, "AgentProfile scope {profile:?} does not match store {store:?}"),
            Self::NotFound(path) => write!(formatter, "AgentProfile source not found: {}", path.display()),
            Self::RefMismatch { requested, actual } => write!(formatter, "AgentProfile source contains ref {actual}, requested {requested}"),
            Self::SourcePathMismatch { profile_ref, expected, actual } => write!(formatter, "AgentProfile {profile_ref} is stored at {}, expected {}", actual.display(), expected.display()),
            Self::AlreadyExists { profile_ref, revision } => write!(formatter, "AgentProfile {profile_ref} already exists at revision {revision}"),
            Self::MissingForUpdate { profile_ref, expected } => write!(formatter, "AgentProfile {profile_ref} is absent; cannot update expected revision {expected}"),
            Self::RevisionConflict { profile_ref, expected, actual } => write!(formatter, "AgentProfile {profile_ref} revision conflict: expected {expected}, actual {actual}"),
            Self::RevisionNotAdvanced { profile_ref, revision } => write!(formatter, "AgentProfile {profile_ref} update did not advance revision {revision}"),
            Self::AgentIdentityChanged { profile_ref, expected_agent, actual_agent } => write!(formatter, "AgentProfile {profile_ref} cannot change semantic Agent from {expected_agent} to {actual_agent}"),
        }
    }
}

impl Error for AgentProfileStoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldRef;

    fn personal_profile(revision: &str) -> AgentProfile {
        let mut profile = AgentProfile::new(
            "agent-profile:guardian",
            revision,
            "agent:guardian",
            AgentProfileScope::Personal,
            WorldRef::new("world:personal").unwrap(),
        )
        .unwrap();
        profile.skill_set_refs = vec!["skill-set:personal".into()];
        profile.method_refs = vec!["method:orientation".into()];
        profile.operative_requirement_refs = vec!["routine:daily-orientation".into()];
        profile
    }

    #[test]
    fn personal_profile_round_trips_as_inspectable_source() {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        let profile = personal_profile("p1");
        let receipt = store.save(&profile, None).unwrap();
        assert!(receipt.created);
        assert_eq!(receipt.previous_revision, None);
        assert!(receipt.source_path.starts_with(ROOT_AGENT_PROFILE_DIR));

        let reading = store.read("agent-profile:guardian").unwrap();
        assert_eq!(reading.profile, profile);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn update_requires_exact_current_revision_and_advances_it() {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        store.save(&personal_profile("p1"), None).unwrap();

        let mut next = personal_profile("p2");
        next.method_refs.push("method:return-review".into());
        assert!(matches!(
            store.save(&next, Some("stale")),
            Err(AgentProfileStoreError::RevisionConflict { .. })
        ));
        let receipt = store.save(&next, Some("p1")).unwrap();
        assert!(!receipt.created);
        assert_eq!(receipt.previous_revision.as_deref(), Some("p1"));
        assert_eq!(store.read("agent-profile:guardian").unwrap().profile, next);

        assert!(matches!(
            store.save(&next, Some("p2")),
            Err(AgentProfileStoreError::RevisionNotAdvanced { .. })
        ));
    }

    #[test]
    fn same_profile_ref_cannot_be_rebound_to_another_agent() {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        store.save(&personal_profile("p1"), None).unwrap();
        let mut changed = personal_profile("p2");
        changed.agent_ref = "agent:other".into();
        assert!(matches!(
            store.save(&changed, Some("p1")),
            Err(AgentProfileStoreError::AgentIdentityChanged { .. })
        ));
    }

    #[test]
    fn project_store_is_a_fractal_sibling_under_projectcentral_agents() {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::project(root.path());
        let mut profile = AgentProfile::new(
            "agent-profile:builder:project",
            "j1",
            "agent:builder",
            AgentProfileScope::Project,
            WorldRef::new("world:project:demo").unwrap(),
        )
        .unwrap();
        profile.source_profile_ref = Some("agent-profile:builder:personal".into());
        profile.provenance_refs.push("agent-profile:builder:personal".into());
        store.save(&profile, None).unwrap();
        let reading = store.read(&profile.profile_ref).unwrap();
        assert!(reading.source_path.starts_with(PROJECT_AGENT_PROFILE_DIR));
        assert_eq!(reading.profile.source_profile_ref, profile.source_profile_ref);
    }

    #[test]
    fn remove_deletes_profile_relation_not_agent_identity() {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        let profile = personal_profile("p1");
        store.save(&profile, None).unwrap();
        let removed = store.remove(&profile.profile_ref, "p1").unwrap();
        assert_eq!(removed.profile.agent_ref, "agent:guardian");
        assert!(matches!(
            store.read(&profile.profile_ref),
            Err(AgentProfileStoreError::NotFound(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_profile_source_container_is_refused() {
        use std::os::unix::fs::symlink;
        let root = crate::tempdir().unwrap();
        let outside = crate::tempdir().unwrap();
        let agent_root = root.path().join("Control/agents");
        fs::create_dir_all(&agent_root).unwrap();
        symlink(outside.path(), agent_root.join("profiles")).unwrap();
        let store = AgentProfileStore::personal(root.path());
        assert!(matches!(
            store.save(&personal_profile("p1"), None),
            Err(AgentProfileStoreError::UnsafeSource(_))
        ));
    }
}