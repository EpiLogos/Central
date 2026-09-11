//! Filesystem source owner for remembered notes (wave 4 cell W4-A). The note
//! lands as durable owner ground under the register's agent area — never a
//! side store — and is create-only: revision, mutation and removal are not
//! machine-owned, and recognition (promotion) is the human owner's separate
//! act outside every Central Action.
use crate::remember::{RememberError, RememberedNote, REMEMBERED_NOTE_SCHEMA};
use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const ROOT_REMEMBERED_DIR: &str = "Control/agents/remembered";
pub const PROJECT_REMEMBERED_DIR: &str = "ProjectCentral/agents/remembered";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RememberedNoteReading {
    pub note: RememberedNote,
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RememberedNoteReceipt {
    pub note_ref: String,
    pub source_path: String,
    pub created: bool,
}

/// Filesystem source owner for one Central register. `owner_root` is the
/// Central root for root-register notes and the Project root for Project
/// notes. The note's stable ref is stored in the document and never inferred
/// from the filename alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RememberStore {
    owner_root: PathBuf,
    source_dir: &'static str,
}

impl RememberStore {
    pub fn root(central_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: central_root.into(),
            source_dir: ROOT_REMEMBERED_DIR,
        }
    }

    pub fn project(project_root: impl Into<PathBuf>) -> Self {
        Self {
            owner_root: project_root.into(),
            source_dir: PROJECT_REMEMBERED_DIR,
        }
    }

    pub fn source_dir(&self) -> PathBuf {
        self.owner_root.join(self.source_dir)
    }

    pub fn source_path(&self, note_ref: &str) -> Result<PathBuf, RememberStoreError> {
        validate_note_ref(note_ref)?;
        Ok(self
            .source_dir()
            .join(format!("note-{}.json", note_key(note_ref))))
    }

    pub fn read(&self, note_ref: &str) -> Result<RememberedNoteReading, RememberStoreError> {
        self.validate_root()?;
        let path = self.source_path(note_ref)?;
        let note = read_note_file(&path)?;
        self.validate_loaded(note_ref, &note)?;
        Ok(RememberedNoteReading {
            source_path: relative(&self.owner_root, &path),
            note,
        })
    }

    /// Create-only save of a generated-proposal note under the store's
    /// register ground. Duplicate note identity is an explicit store refusal,
    /// never an overwrite; there is no machine update path.
    pub fn save(&self, note: &RememberedNote) -> Result<RememberedNoteReceipt, RememberStoreError> {
        self.validate_root()?;
        validate_note_ref(&note.note_ref)?;
        if note.schema != REMEMBERED_NOTE_SCHEMA {
            return Err(RememberStoreError::InvalidNote(format!(
                "unsupported RememberedNote schema {}",
                note.schema
            )));
        }
        note.validate_shape()
            .map_err(|error| RememberStoreError::InvalidNote(error.to_string()))?;

        let dir = self.source_dir();
        ensure_directory_path(&self.owner_root, &dir)?;
        let path = self.source_path(&note.note_ref)?;
        if path.exists() {
            let current = read_note_file(&path)?;
            return Err(RememberStoreError::AlreadyExists {
                note_ref: note.note_ref.clone(),
                recorded_at_unix_seconds: current.provenance.recorded_at_unix_seconds,
            });
        }

        let mut bytes = serde_json::to_vec_pretty(note)
            .map_err(|error| RememberStoreError::InvalidNote(error.to_string()))?;
        bytes.push(b'\n');
        atomic_write(&path, &bytes)?;

        Ok(RememberedNoteReceipt {
            note_ref: note.note_ref.clone(),
            source_path: relative(&self.owner_root, &path),
            created: true,
        })
    }

    fn validate_root(&self) -> Result<(), RememberStoreError> {
        let metadata = match fs::symlink_metadata(&self.owner_root) {
            Ok(metadata) => metadata,
            // An absent owner root is unsafe ground, not an opaque IO error:
            // the Action maps it to the explicit absent/unwritable state.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(RememberStoreError::UnsafeRoot(self.owner_root.clone()));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RememberStoreError::UnsafeRoot(self.owner_root.clone()));
        }
        Ok(())
    }

    fn validate_loaded(
        &self,
        requested_ref: &str,
        note: &RememberedNote,
    ) -> Result<(), RememberStoreError> {
        if note.schema != REMEMBERED_NOTE_SCHEMA {
            return Err(RememberStoreError::InvalidNote(format!(
                "unsupported RememberedNote schema {}",
                note.schema
            )));
        }
        if note.note_ref != requested_ref {
            return Err(RememberStoreError::RefMismatch {
                requested: requested_ref.to_owned(),
                actual: note.note_ref.clone(),
            });
        }
        validate_note_ref(&note.note_ref)?;
        note.validate_shape()
            .map_err(|error| RememberStoreError::InvalidNote(error.to_string()))?;
        Ok(())
    }
}

fn read_note_file(path: &Path) -> Result<RememberedNote, RememberStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RememberStoreError::NotFound(path.to_path_buf())
        } else {
            RememberStoreError::Io(error.to_string())
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RememberStoreError::UnsafeSource(path.to_path_buf()));
    }
    let note: RememberedNote = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| RememberStoreError::InvalidNote(error.to_string()))?;
    // Parse succeeded: the typed record already proves recognition was not
    // claimed, because the recognised state cannot parse.
    note.provenance
        .validate()
        .map_err(|error| RememberStoreError::InvalidNote(error.to_string()))?;
    Ok(note)
}

fn validate_note_ref(note_ref: &str) -> Result<(), RememberStoreError> {
    if note_ref.trim().is_empty() || note_ref != note_ref.trim() || note_ref.contains('\0') {
        Err(RememberStoreError::InvalidNoteRef(note_ref.to_owned()))
    } else {
        Ok(())
    }
}

fn note_key(note_ref: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in note_ref.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn ensure_directory_path(owner_root: &Path, dir: &Path) -> Result<(), RememberStoreError> {
    let relative = dir
        .strip_prefix(owner_root)
        .map_err(|_| RememberStoreError::UnsafeSource(dir.to_path_buf()))?;
    let mut current = owner_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(RememberStoreError::UnsafeSource(dir.to_path_buf()));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(RememberStoreError::UnsafeSource(current));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_directory_not_symlink(path: &Path) -> Result<(), RememberStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(RememberStoreError::UnsafeSource(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RememberStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| RememberStoreError::UnsafeSource(path.to_path_buf()))?;
    ensure_directory_not_symlink(parent)?;
    let tmp = parent.join(format!(
        ".remember-{}.tmp",
        note_key(&path.to_string_lossy())
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RememberStoreError {
    Io(String),
    UnsafeRoot(PathBuf),
    UnsafeSource(PathBuf),
    InvalidNote(String),
    InvalidNoteRef(String),
    NotFound(PathBuf),
    RefMismatch {
        requested: String,
        actual: String,
    },
    AlreadyExists {
        note_ref: String,
        recorded_at_unix_seconds: u64,
    },
}

impl From<std::io::Error> for RememberStoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<RememberError> for RememberStoreError {
    fn from(value: RememberError) -> Self {
        Self::InvalidNote(value.to_string())
    }
}

impl fmt::Display for RememberStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => formatter.write_str(error),
            Self::UnsafeRoot(path) => write!(
                formatter,
                "RememberedNote owner root is unsafe: {}",
                path.display()
            ),
            Self::UnsafeSource(path) => write!(
                formatter,
                "RememberedNote source path is unsafe: {}",
                path.display()
            ),
            Self::InvalidNote(error) => write!(formatter, "invalid RememberedNote source: {error}"),
            Self::InvalidNoteRef(value) => {
                write!(formatter, "invalid RememberedNote ref {value:?}")
            }
            Self::NotFound(path) => write!(
                formatter,
                "RememberedNote source not found: {}",
                path.display()
            ),
            Self::RefMismatch { requested, actual } => write!(
                formatter,
                "RememberedNote source contains ref {actual}, requested {requested}"
            ),
            Self::AlreadyExists {
                note_ref,
                recorded_at_unix_seconds,
            } => write!(
                formatter,
                "RememberedNote {note_ref} already exists (recorded at unix second {recorded_at_unix_seconds})"
            ),
        }
    }
}

impl Error for RememberStoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remember::REMEMBERED_DESTINATION;

    const SELECTION: &str = "A horizon with no field lands everywhere.";
    const SOURCE: &str = "Control/agents/governance/field-and-now/session-work-placement.md";

    fn note(origin: &str) -> RememberedNote {
        RememberedNote::generated_proposal(SELECTION, SOURCE, origin, 11).unwrap()
    }

    #[test]
    fn root_store_lands_durable_ground_and_reads_it_back() {
        let root = crate::tempdir().unwrap();
        let store = RememberStore::root(root.path());
        let note = note("central.remember");
        let receipt = store.save(&note).unwrap();
        assert!(receipt.created);
        assert!(receipt.source_path.starts_with(ROOT_REMEMBERED_DIR));
        assert!(root.path().join(&receipt.source_path).is_file());

        let reading = store.read(&note.note_ref).unwrap();
        assert_eq!(reading.note, note);
        assert_eq!(reading.note.destination, REMEMBERED_DESTINATION);
    }

    #[test]
    fn project_store_is_a_fractal_sibling_under_projectcentral_agents() {
        let root = crate::tempdir().unwrap();
        let store = RememberStore::project(root.path());
        let note = note("projectcentral.remember");
        let receipt = store.save(&note).unwrap();
        assert!(receipt.source_path.starts_with(PROJECT_REMEMBERED_DIR));
        assert_eq!(store.read(&note.note_ref).unwrap().note, note);
    }

    #[test]
    fn duplicate_note_identity_is_an_explicit_refusal() {
        let root = crate::tempdir().unwrap();
        let store = RememberStore::root(root.path());
        let note = note("central.remember");
        store.save(&note).unwrap();
        let error = store.save(&note).unwrap_err();
        assert_eq!(
            error,
            RememberStoreError::AlreadyExists {
                note_ref: note.note_ref.clone(),
                recorded_at_unix_seconds: 11,
            }
        );
    }

    #[test]
    fn absent_owner_root_is_unsafe_ground_not_a_silent_noop() {
        let root = crate::tempdir().unwrap();
        let missing = root.path().join("missing");
        let store = RememberStore::root(&missing);
        assert!(matches!(
            store.save(&note("central.remember")),
            Err(RememberStoreError::UnsafeRoot(_))
        ));
    }

    #[test]
    fn file_blocking_the_remembered_dir_is_unsafe_ground() {
        let root = crate::tempdir().unwrap();
        let blocked = root.path().join(ROOT_REMEMBERED_DIR);
        fs::create_dir_all(blocked.parent().unwrap()).unwrap();
        fs::write(&blocked, "not a directory").unwrap();
        let store = RememberStore::root(root.path());
        assert!(matches!(
            store.save(&note("central.remember")),
            Err(RememberStoreError::UnsafeSource(_))
        ));
    }

    #[test]
    fn tampered_recognition_claim_fails_the_typed_read() {
        let root = crate::tempdir().unwrap();
        let store = RememberStore::root(root.path());
        let note = note("central.remember");
        store.save(&note).unwrap();
        let path = store.source_path(&note.note_ref).unwrap();
        let mut tampered: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        tampered["provenance"]["recognition"] = serde_json::json!("recognised");
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(matches!(
            store.read(&note.note_ref),
            Err(RememberStoreError::InvalidNote(_))
        ));
    }
}
