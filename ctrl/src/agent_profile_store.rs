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

    pub(crate) fn owner_root(&self) -> &Path {
        &self.owner_root
    }
    /// Serialize writes and acceptance for the same profile identity.
    /// This lock is not permission to author or accept source.
    pub(crate) fn mutation_lock(
        &self,
        reference: &str,
    ) -> Result<crate::source_safety::SourceLock, AgentProfileStoreError> {
        self.validate_root()?;
        validate_ref(reference)?;
        crate::source_safety::lock(
            &self.owner_root,
            &format!("agent-profile-{}.lock", profile_key(reference)),
        )
        .map_err(Into::into)
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
        crate::source_safety::reject_symlink_components(
            &self.owner_root,
            path.strip_prefix(&self.owner_root)
                .map_err(|_| AgentProfileStoreError::UnsafeSource(path.clone()))?,
        )?;
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
            if !metadata.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
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
    ///
    /// The profile is one a reader gives back: `agent-profile.read` shows a kept name without the characters that draw nothing,
    /// and whoever read the profile, changed something else and saves it gives the name as it was shown. That name is the kept
    /// one, and its bytes stay (be2j.c2 item 1, DLW build ruling 2026-11-10).
    pub fn save(
        &self,
        profile: &AgentProfile,
        expected_revision: Option<&str>,
    ) -> Result<AgentProfileWriteReceipt, AgentProfileStoreError> {
        self.save_words(profile, expected_revision, true)
    }

    /// Save a profile whose every word is as given: Central's own edit of the profile it read from this store (a chain edit,
    /// `agent-profile.rename`), where a word that differs from the kept one is a changed word, whatever a read shows for the
    /// kept one. Renaming an agent to its name as shown is how a person keeps it so, and it replaces the kept bytes.
    pub fn save_given(
        &self,
        profile: &AgentProfile,
        expected_revision: Option<&str>,
    ) -> Result<AgentProfileWriteReceipt, AgentProfileStoreError> {
        self.save_words(profile, expected_revision, false)
    }

    fn save_words(
        &self,
        profile: &AgentProfile,
        expected_revision: Option<&str>,
        read_back: bool,
    ) -> Result<AgentProfileWriteReceipt, AgentProfileStoreError> {
        let _lock = self.mutation_lock(&profile.profile_ref)?;
        self.validate_loaded(&profile.profile_ref, profile)?;
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
        // The read-compare-write window must be exclusive per profile file:
        // two concurrent saves of the same revision otherwise both pass the
        // CAS check and the loser's write silently wins or collides.
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
                });
            }
            (Some(current), None) => {
                return Err(AgentProfileStoreError::AlreadyExists {
                    profile_ref: profile.profile_ref.clone(),
                    revision: current.revision.clone(),
                });
            }
            (Some(current), Some(expected)) if current.revision != expected => {
                return Err(AgentProfileStoreError::RevisionConflict {
                    profile_ref: profile.profile_ref.clone(),
                    expected: expected.to_owned(),
                    actual: current.revision.clone(),
                });
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
        // A name given back as a read showed it is the kept name (be2j.c2 item 1): the read shows a kept name that holds a
        // character that draws nothing without it, or as an unnamed agent when nothing else is left (`shown_agent_name`, the
        // one source of both). The kept bytes are what is written, so reading a profile and saving it changes nothing; any
        // other spelling is a changed name. The refs are read as they are kept, so a ref is unchanged only as the same bytes.
        let read_back_name = existing.as_ref().filter(|_| read_back).and_then(|kept| {
            match crate::agent_profile_actions::shown_agent_name(kept) {
                (Some(shown), Some(_)) if profile.name.as_deref() == Some(shown.as_str()) => {
                    kept.name.clone()
                }
                _ => None,
            }
        });
        let written;
        let profile = match read_back_name {
            Some(name) => {
                let mut kept_so = profile.clone();
                kept_so.name = Some(name);
                written = kept_so;
                &written
            }
            None => profile,
        };
        // What a person gives now is refused with the whole set (be2i.1; be2j for the refs): the name, the expressive character
        // ref and every other ref, where it is new here or changed, holds no invisible character. One kept before be2i and kept as
        // it was stays as it stands.
        let kept = existing.as_ref().map(given_words);
        for (at, (field, given)) in given_words(profile).into_iter().enumerate() {
            let before = kept.as_ref().map_or(&[][..], |kept| kept[at].1.as_slice());
            if given
                .iter()
                .any(|value| !before.contains(value) && value.chars().any(crate::names::unplain))
            {
                return Err(AgentProfileStoreError::InvalidProfile(format!(
                    "{field} must hold no control character and no invisible one"
                )));
            }
        }

        let mut bytes = serde_json::to_vec_pretty(profile)
            .map_err(|error| AgentProfileStoreError::InvalidProfile(error.to_string()))?;
        bytes.push(b'\n');
        atomic_write(&self.owner_root, &path, &bytes)?;

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
        let _lock = self.mutation_lock(profile_ref)?;
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
        profile
            .validate_shape()
            .map_err(|e| AgentProfileStoreError::InvalidProfile(e.to_string()))?;
        Ok(())
    }
}

/// The words a profile gives, field by field in one order: its name, its expressive character ref and each of its refs, as
/// `AgentProfile::validate_shape` names them. `AgentProfileStore::save` refuses the set (`names::unplain`) in what is new.
fn given_words(profile: &AgentProfile) -> [(&'static str, Vec<&str>); 14] {
    fn one(value: &Option<String>) -> Vec<&str> {
        value.as_deref().into_iter().collect()
    }
    fn many(values: &[String]) -> Vec<&str> {
        values.iter().map(String::as_str).collect()
    }
    [
        ("Agent Profile name", one(&profile.name)),
        (
            "Agent Profile expressive character ref",
            one(&profile.expressive_character_ref),
        ),
        ("Agent Profile ref", vec![profile.profile_ref.as_str()]),
        ("Agent ref", vec![profile.agent_ref.as_str()]),
        ("source Agent Profile ref", one(&profile.source_profile_ref)),
        ("governance refs", many(&profile.governance_refs)),
        ("Skill refs", many(&profile.skill_refs)),
        ("SkillSet refs", many(&profile.skill_set_refs)),
        ("Method refs", many(&profile.method_refs)),
        ("Routine refs", many(&profile.routine_refs)),
        (
            "Knowledge source refs",
            many(&profile.knowledge_source_refs),
        ),
        (
            "Central Computer access intent refs",
            many(&profile.computer_access_intent_refs),
        ),
        (
            "placement intent refs",
            many(&profile.placement_intent_refs),
        ),
        ("provenance refs", many(&profile.provenance_refs)),
    ]
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

fn atomic_write(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), AgentProfileStoreError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AgentProfileStoreError::UnsafeSource(path.to_path_buf()))?
        .to_str()
        .ok_or_else(|| AgentProfileStoreError::UnsafeSource(path.to_path_buf()))?;
    let content = std::str::from_utf8(bytes)
        .map_err(|e| AgentProfileStoreError::InvalidProfile(e.to_string()))?;
    match crate::source_safety::read(root, relative) {
        Ok(current) => crate::source_safety::replace(
            root,
            relative,
            &crate::source_safety::content_revision_bytes(current.as_bytes()),
            content,
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::continuous_work::source::put_new(root, relative, content)?;
        }
        Err(error) => return Err(error.into()),
    }
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
            Self::UnsafeRoot(path) => write!(
                formatter,
                "AgentProfile owner root is unsafe: {}",
                path.display()
            ),
            Self::UnsafeSource(path) => write!(
                formatter,
                "AgentProfile source path is unsafe: {}",
                path.display()
            ),
            Self::InvalidProfile(error) => {
                write!(formatter, "invalid AgentProfile source: {error}")
            }
            Self::InvalidProfileRef(value) => {
                write!(formatter, "invalid AgentProfile ref {value:?}")
            }
            Self::ScopeMismatch { store, profile } => write!(
                formatter,
                "AgentProfile scope {profile:?} does not match store {store:?}"
            ),
            Self::NotFound(path) => write!(
                formatter,
                "AgentProfile source not found: {}",
                path.display()
            ),
            Self::RefMismatch { requested, actual } => write!(
                formatter,
                "AgentProfile source contains ref {actual}, requested {requested}"
            ),
            Self::SourcePathMismatch {
                profile_ref,
                expected,
                actual,
            } => write!(
                formatter,
                "AgentProfile {profile_ref} is stored at {}, expected {}",
                actual.display(),
                expected.display()
            ),
            Self::AlreadyExists {
                profile_ref,
                revision,
            } => write!(
                formatter,
                "AgentProfile {profile_ref} already exists at revision {revision}"
            ),
            Self::MissingForUpdate {
                profile_ref,
                expected,
            } => write!(
                formatter,
                "AgentProfile {profile_ref} is absent; cannot update expected revision {expected}"
            ),
            Self::RevisionConflict {
                profile_ref,
                expected,
                actual,
            } => write!(
                formatter,
                "AgentProfile {profile_ref} revision conflict: expected {expected}, actual {actual}"
            ),
            Self::RevisionNotAdvanced {
                profile_ref,
                revision,
            } => write!(
                formatter,
                "AgentProfile {profile_ref} update did not advance revision {revision}"
            ),
            Self::AgentIdentityChanged {
                profile_ref,
                expected_agent,
                actual_agent,
            } => write!(
                formatter,
                "AgentProfile {profile_ref} cannot change semantic Agent from {expected_agent} to {actual_agent}"
            ),
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

    /// be2i.1: a profile kept before be2i whose name holds a character that draws nothing still reads and lists, and a save that
    /// keeps that name as it was kept is admitted. A name or an expressive character ref given now with one is refused, new or
    /// changed.
    #[test]
    fn a_profile_kept_before_be2i_with_an_invisible_name_reads_and_saves_and_a_given_one_is_refused(
    ) {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        store.save(&personal_profile("p1"), None).unwrap();
        let path = store.source_path("agent-profile:guardian").unwrap();
        let mut kept: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        kept["name"] = serde_json::json!("Guar\u{200D}dian");
        std::fs::write(&path, serde_json::to_vec_pretty(&kept).unwrap()).unwrap();
        let reading = store.read("agent-profile:guardian").unwrap();
        assert_eq!(reading.profile.name.as_deref(), Some("Guar\u{200D}dian"));
        assert_eq!(store.list().unwrap().len(), 1);
        let mut next = reading.profile.clone();
        next.revision = "p2".into();
        next.method_refs.push("method:return-review".into());
        store.save(&next, Some("p1")).unwrap();

        let refused = |profile: &AgentProfile, expected: Option<&str>, field: &str| {
            let why = match store.save(profile, expected) {
                Err(AgentProfileStoreError::InvalidProfile(why)) => why,
                other => panic!("{field}: {other:?}"),
            };
            assert_eq!(
                why,
                format!("{field} must hold no control character and no invisible one")
            );
        };
        let mut renamed = next.clone();
        renamed.revision = "p3".into();
        renamed.name = Some("Keep\u{200D}er".into());
        refused(&renamed, Some("p2"), "Agent Profile name");
        let mut expressive = next.clone();
        expressive.revision = "p3".into();
        expressive.expressive_character_ref = Some(
            "central:Control/agents/expressive-material/character/guardian\u{2066}.expression.json"
                .into(),
        );
        refused(
            &expressive,
            Some("p2"),
            "Agent Profile expressive character ref",
        );
        let mut fresh = AgentProfile::new(
            "agent-profile:keeper",
            "k1",
            "agent:keeper",
            AgentProfileScope::Personal,
            WorldRef::new("world:personal").unwrap(),
        )
        .unwrap();
        fresh.name = Some("Keep\u{200D}er".into());
        refused(&fresh, None, "Agent Profile name");
        fresh.name = Some("Keeper".into());
        store.save(&fresh, None).unwrap();

        // A name changed only in what draws nothing is a changed name, and refused (be2j item 17).
        let mut respelt = next.clone();
        respelt.revision = "p3".into();
        respelt.name = Some("Guar\u{200B}dian".into());
        refused(&respelt, Some("p2"), "Agent Profile name");

        // be2j items 11 and 17 (DLW build ruling 2026-11-10): every other ref is checked with the set where it is given, too. A ref
        // or an expressive character ref kept before be2i, and kept as it was, stays.
        let mut kept: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        kept["skill_refs"] = serde_json::json!(["skill:wayf\u{200B}inder"]);
        kept["expressive_character_ref"] = serde_json::json!(
            "central:Control/agents/expressive-material/character/guar\u{200D}dian.expression.json"
        );
        std::fs::write(&path, serde_json::to_vec_pretty(&kept).unwrap()).unwrap();
        let mut carried = store.read("agent-profile:guardian").unwrap().profile;
        carried.revision = "p4".into();
        carried.purpose = Some("Keeps the ground.".into());
        store.save(&carried, Some("p2")).unwrap();
        type Change = (&'static str, fn(&mut AgentProfile));
        let changes: [Change; 5] = [
            ("Skill refs", |p| {
                p.skill_refs.push("skill:re\u{202E}view".into())
            }),
            ("Method refs", |p| {
                p.method_refs.push("method:\u{2066}close".into())
            }),
            ("provenance refs", |p| {
                p.provenance_refs.push("central:\u{3164}".into())
            }),
            ("source Agent Profile ref", |p| {
                p.source_profile_ref = Some("agent-profile:\u{200D}source".into())
            }),
            ("Knowledge source refs", |p| {
                p.knowledge_source_refs.push("wiki:\u{E0041}".into())
            }),
        ];
        for (field, change) in changes {
            let mut given = carried.clone();
            given.revision = "p5".into();
            change(&mut given);
            refused(&given, Some("p4"), field);
        }
        let mut agent = personal_profile("a1");
        agent.profile_ref = "agent-profile:\u{200B}ward".into();
        refused(&agent, None, "Agent Profile ref");
    }

    /// be2j.c2 item 1 (DLW build ruling 2026-11-10): a name given back as a read showed it is the kept name, and its bytes stay;
    /// a name that really changed is checked as given; and Central's own edit (`save_given`) takes every word as given, so a
    /// rename to the shown name replaces the kept bytes.
    #[test]
    fn a_name_given_back_as_a_read_showed_it_keeps_its_bytes_and_an_edit_of_the_record_replaces_them(
    ) {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        store.save(&personal_profile("p1"), None).unwrap();
        let path = store.source_path("agent-profile:guardian").unwrap();
        let keep = |name: &str| {
            let mut kept: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            kept["name"] = serde_json::json!(name);
            std::fs::write(&path, serde_json::to_vec_pretty(&kept).unwrap()).unwrap();
            store.read("agent-profile:guardian").unwrap().profile
        };
        let stored = || {
            let kept: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            kept["name"].as_str().map(str::to_owned)
        };
        let mut revision = 1;
        let mut next = |mut profile: AgentProfile, name: &str| {
            revision += 1;
            let expected = std::mem::replace(&mut profile.revision, format!("p{revision}"));
            profile.name = Some(name.into());
            (profile, expected)
        };
        // The name as a read shows it, with something else changed: the kept bytes stay.
        for (kept, shown) in [
            ("Guar\u{200D}dian", "Guardian"),
            ("\u{200B}\u{3164}", "unnamed agent 8p9z"),
        ] {
            let (mut given, expected) = next(keep(kept), shown);
            given.purpose = Some(format!("Keeps the ground, {}.", given.revision));
            store.save(&given, Some(&expected)).unwrap();
            assert_eq!(stored().as_deref(), Some(kept), "given back as {shown:?}");
            let read = store.read("agent-profile:guardian").unwrap().profile;
            assert_eq!(read.purpose, given.purpose, "the rest is saved");
        }
        // Any other spelling is a changed name: a plain one replaces the kept bytes, and one that holds an invisible character
        // is refused.
        let (given, expected) = next(keep("Guar\u{200D}dian"), "Warden");
        store.save(&given, Some(&expected)).unwrap();
        assert_eq!(stored().as_deref(), Some("Warden"));
        let (given, expected) = next(keep("Guar\u{200D}dian"), "Guard\u{200D}ian");
        assert!(matches!(
            store.save(&given, Some(&expected)),
            Err(AgentProfileStoreError::InvalidProfile(why))
                if why == "Agent Profile name must hold no control character and no invisible one"
        ));
        assert_eq!(stored().as_deref(), Some("Guar\u{200D}dian"));
        // A kept name that holds nothing invisible is read as it is kept, so its trimmed spelling is a changed name.
        let (given, expected) = next(keep(" Guardian"), "Guardian");
        store.save(&given, Some(&expected)).unwrap();
        assert_eq!(stored().as_deref(), Some("Guardian"));
        // Central's own edit of the record: every word is as given, so the shown name replaces the kept one.
        let (given, expected) = next(keep("Guar\u{200D}dian"), "Guardian");
        store.save_given(&given, Some(&expected)).unwrap();
        assert_eq!(stored().as_deref(), Some("Guardian"));
    }

    /// be2j.c2 item 3 (DLW build ruling 2026-11-10): a Routine ref given with a character that draws nothing is refused, and one
    /// kept so before be2i, kept as it was, stays.
    #[test]
    fn a_routine_ref_given_with_an_invisible_character_is_refused_and_a_kept_one_stays() {
        ref_list_case("Routine refs", "routine_refs", |profile| {
            &mut profile.routine_refs
        });
    }

    /// be2j.c2 item 3: the same for a governance ref.
    #[test]
    fn a_governance_ref_given_with_an_invisible_character_is_refused_and_a_kept_one_stays() {
        ref_list_case("governance refs", "governance_refs", |profile| {
            &mut profile.governance_refs
        });
    }

    fn ref_list_case(field: &str, key: &str, refs: fn(&mut AgentProfile) -> &mut Vec<String>) {
        let root = crate::tempdir().unwrap();
        let store = AgentProfileStore::personal(root.path());
        store.save(&personal_profile("p1"), None).unwrap();
        let mut given = personal_profile("p2");
        refs(&mut given).push("central:gi\u{200B}ven".into());
        assert!(
            matches!(
                store.save(&given, Some("p1")),
                Err(AgentProfileStoreError::InvalidProfile(why))
                    if why == format!("{field} must hold no control character and no invisible one")
            ),
            "{field}"
        );
        let path = store.source_path("agent-profile:guardian").unwrap();
        let mut kept: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        kept[key] = serde_json::json!(["central:ke\u{200D}pt"]);
        std::fs::write(&path, serde_json::to_vec_pretty(&kept).unwrap()).unwrap();
        let mut carried = store.read("agent-profile:guardian").unwrap().profile;
        assert_eq!(*refs(&mut carried), ["central:ke\u{200D}pt"]);
        carried.revision = "p2".into();
        carried.purpose = Some("Keeps the ground.".into());
        store.save(&carried, Some("p1")).unwrap();
        let mut read = store.read("agent-profile:guardian").unwrap().profile;
        assert_eq!(*refs(&mut read), ["central:ke\u{200D}pt"]);
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
        profile
            .provenance_refs
            .push("agent-profile:builder:personal".into());
        store.save(&profile, None).unwrap();
        let reading = store.read(&profile.profile_ref).unwrap();
        assert!(reading.source_path.starts_with(PROJECT_AGENT_PROFILE_DIR));
        assert_eq!(
            reading.profile.source_profile_ref,
            profile.source_profile_ref
        );
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
