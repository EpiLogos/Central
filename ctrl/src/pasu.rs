//! central.pasu/v1 — the versioned bounded-entity identity grammar.
//!
//! Paśu is the general notion of a bounded entity that can be an objective
//! internality named by the world; its forms here are `nara` (the human),
//! `agent` and `agent-set`. The grammar is one participation vocabulary for
//! all three forms — no subtype hierarchy, no second identity ontology. The
//! human identity source anchor is the authored `Control/user/identity/`
//! folder, given a versioned manifest carrier by this module (owner directive
//! 2026-09-07).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const PASU_SCHEMA: &str = "central.pasu/v1";
pub const PASU_IDENTITY_MANIFEST_SCHEMA: &str = "central.pasu.identity-manifest/v1";
/// The stipulated anchor of the human (nara-form) identity source.
pub const PASU_IDENTITY_SOURCE_DIR: &str = "Control/user/identity";
pub const PASU_IDENTITY_MANIFEST_PATH: &str = "Control/user/identity/manifest.json";
pub const PASU_REF_PREFIX: &str = "central:pasu:";

/// The form of a bounded entity. `nara` is the human form of paśu, not a
/// synonym for it; agents and agent-sets are the other constituted forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PasuForm {
    Nara,
    Agent,
    AgentSet,
}

impl PasuForm {
    pub fn as_str(&self) -> &'static str {
        match self {
            PasuForm::Nara => "nara",
            PasuForm::Agent => "agent",
            PasuForm::AgentSet => "agent-set",
        }
    }

    pub fn parse(raw: &str) -> Option<PasuForm> {
        match raw {
            "nara" => Some(PasuForm::Nara),
            "agent" => Some(PasuForm::Agent),
            "agent-set" => Some(PasuForm::AgentSet),
            _ => None,
        }
    }
}

/// A bounded-entity ref in the `central.pasu/v1` grammar:
/// `central:pasu:<form>:<id>`. The id is opaque to the grammar — Central owns
/// the subject; consumers (Factory, Actuation, AIKit) reference it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PasuRef(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasuRefError {
    Empty,
    NotPasu,
    UnknownForm(String),
    MissingId,
}

impl std::fmt::Display for PasuRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasuRefError::Empty => write!(f, "pasu ref is empty"),
            PasuRefError::NotPasu => {
                write!(f, "pasu ref must be {PASU_REF_PREFIX}<form>:<id>")
            }
            PasuRefError::UnknownForm(form) => {
                write!(
                    f,
                    "pasu ref has unknown form `{form}` (nara, agent, agent-set)"
                )
            }
            PasuRefError::MissingId => write!(f, "pasu ref is missing its subject id"),
        }
    }
}

impl PasuRef {
    pub fn parse(raw: &str) -> Result<PasuRef, PasuRefError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(PasuRefError::Empty);
        }
        let rest = raw
            .strip_prefix(PASU_REF_PREFIX)
            .ok_or(PasuRefError::NotPasu)?;
        let (form, id) = rest.split_once(':').ok_or(PasuRefError::MissingId)?;
        PasuForm::parse(form).ok_or_else(|| PasuRefError::UnknownForm(form.to_owned()))?;
        if id.trim().is_empty() {
            return Err(PasuRefError::MissingId);
        }
        Ok(PasuRef(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn form(&self) -> PasuForm {
        let rest = self.0.strip_prefix(PASU_REF_PREFIX).unwrap_or_default();
        let (form, _) = rest.split_once(':').unwrap_or(("", ""));
        PasuForm::parse(form).unwrap_or(PasuForm::Nara)
    }

    pub fn subject_id(&self) -> &str {
        let rest = self.0.strip_prefix(PASU_REF_PREFIX).unwrap_or_default();
        rest.split_once(':').map(|(_, id)| id).unwrap_or_default()
    }

    /// The pasu ref of the agent form for an existing agent ref.
    pub fn for_agent(agent_ref: &str) -> Result<PasuRef, PasuRefError> {
        pasu_ref_for(PasuForm::Agent, agent_ref)
    }

    /// The pasu ref of the agent-set form for an existing agent-set ref.
    pub fn for_agent_set(agent_set_ref: &str) -> Result<PasuRef, PasuRefError> {
        pasu_ref_for(PasuForm::AgentSet, agent_set_ref)
    }

    /// The nara-form subject ref of this Central world.
    pub fn local_nara() -> PasuRef {
        PasuRef(format!("{PASU_REF_PREFIX}nara:local"))
    }
}

fn pasu_ref_for(form: PasuForm, id: &str) -> Result<PasuRef, PasuRefError> {
    let id = id.trim();
    if id.is_empty() {
        return Err(PasuRefError::MissingId);
    }
    PasuRef::parse(&format!("{PASU_REF_PREFIX}{}:{id}", form.as_str()))
}

/// The subject named by the nara identity manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasuSubject {
    #[serde(rename = "ref")]
    pub ref_: PasuRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// One sourced file of the identity source. `revision` is the declared
/// revision of the promoted copy, if the source carries one; consumers derive
/// content revisions from the files themselves for freshness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasuSourcedFile {
    pub path: String,
    pub standing: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasuIdentitySource {
    pub path: String,
    pub provenance_law: String,
    #[serde(default)]
    pub sources: Vec<PasuSourcedFile>,
}

/// The versioned nara identity-source manifest anchored at
/// `Control/user/identity/manifest.json` (W10 V1, owner directive
/// 2026-09-07). The authored folder stays the content; the manifest carries
/// the subject ref, the sourced files and the provenance law.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasuIdentityManifest {
    pub schema: String,
    pub revision: String,
    pub subject: PasuSubject,
    pub identity_source: PasuIdentitySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasuManifestError {
    Io(String),
    UnsupportedSchema,
    SubjectRef(String),
    SourceOutsideAnchor(String),
}

impl std::fmt::Display for PasuManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasuManifestError::Io(error) => write!(f, "identity manifest cannot be read: {error}"),
            PasuManifestError::UnsupportedSchema => {
                write!(f, "identity manifest has an unsupported schema")
            }
            PasuManifestError::SubjectRef(reason) => {
                write!(f, "identity manifest subject is invalid: {reason}")
            }
            PasuManifestError::SourceOutsideAnchor(path) => write!(
                f,
                "identity manifest source `{path}` is outside the identity source anchor"
            ),
        }
    }
}

impl PasuIdentityManifest {
    pub fn new(subject: PasuSubject, identity_source: PasuIdentitySource) -> PasuIdentityManifest {
        PasuIdentityManifest {
            schema: PASU_IDENTITY_MANIFEST_SCHEMA.to_owned(),
            revision: "1".to_owned(),
            subject,
            identity_source,
        }
    }

    /// Grammar validation: schema, subject-ref form agreement, and the
    /// stipulated anchor. Sourced files must stay inside the anchor; nothing
    /// else is policed — the authored folder is the content.
    pub fn validate(&self) -> Result<(), PasuManifestError> {
        if self.schema != PASU_IDENTITY_MANIFEST_SCHEMA {
            return Err(PasuManifestError::UnsupportedSchema);
        }
        let parsed = PasuRef::parse(self.subject.ref_.as_str())
            .map_err(|error| PasuManifestError::SubjectRef(error.to_string()))?;
        // The nara identity-source manifest names the nara form.
        if parsed.form() != PasuForm::Nara {
            return Err(PasuManifestError::SubjectRef(
                "the nara identity manifest must name a nara-form subject".to_owned(),
            ));
        }
        if self.identity_source.path != PASU_IDENTITY_SOURCE_DIR {
            return Err(PasuManifestError::SourceOutsideAnchor(
                self.identity_source.path.clone(),
            ));
        }
        for source in &self.identity_source.sources {
            let relative = Path::new(&source.path);
            if relative.is_absolute() || !under_anchor(&source.path) {
                return Err(PasuManifestError::SourceOutsideAnchor(source.path.clone()));
            }
        }
        Ok(())
    }

    pub fn load(central_root: &Path) -> Result<PasuIdentityManifest, PasuManifestError> {
        let path = central_root.join(PASU_IDENTITY_MANIFEST_PATH);
        if !path.is_file() {
            return Err(PasuManifestError::Io("manifest file is absent".to_owned()));
        }
        let bytes = fs::read(&path).map_err(|error| PasuManifestError::Io(error.to_string()))?;
        let manifest: PasuIdentityManifest = serde_json::from_slice(&bytes)
            .map_err(|error| PasuManifestError::Io(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Atomic write: temp file beside the target, then rename.
    pub fn persist(&self, central_root: &Path) -> Result<PathBuf, PasuManifestError> {
        self.validate()?;
        let path = central_root.join(PASU_IDENTITY_MANIFEST_PATH);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| PasuManifestError::Io(error.to_string()))?;
        }
        let mut body = serde_json::to_string_pretty(self)
            .map_err(|error| PasuManifestError::Io(error.to_string()))?;
        body.push('\n');
        let temp = path.with_file_name(format!(
            ".{}.tmp",
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
        fs::write(&temp, body.as_bytes())
            .map_err(|error| PasuManifestError::Io(error.to_string()))?;
        fs::rename(&temp, &path).map_err(|error| PasuManifestError::Io(error.to_string()))?;
        Ok(path)
    }
}

fn under_anchor(path: &str) -> bool {
    let anchor = format!("{PASU_IDENTITY_SOURCE_DIR}/");
    path.starts_with(&anchor) && !path.contains("/../") && !path.ends_with("/..")
}

/// FNV-1a 64-bit content revision, hex — the same derivation the profile
/// store uses for CAS keys, applied to identity-source bytes.
pub fn content_revision(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The read-model state of the identity anchor, exposed by `central.world`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PasuIdentityState {
    pub manifest_path: String,
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_source_path: Option<String>,
    pub sourced_files: Vec<PasuSourcedFileState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground_relations_subject_ref: Option<String>,
    /// None when no side of the agreement is declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_ref_consistent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PasuSourcedFileState {
    pub path: String,
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_revision: Option<String>,
}

impl PasuIdentityState {
    /// Absent-manifest state; absence is data, not an error.
    pub fn absent() -> PasuIdentityState {
        PasuIdentityState {
            manifest_path: PASU_IDENTITY_MANIFEST_PATH.to_owned(),
            present: false,
            subject_ref: None,
            form: None,
            manifest_revision: None,
            identity_source_path: None,
            sourced_files: Vec::new(),
            ground_relations_subject_ref: None,
            subject_ref_consistent: None,
            error: None,
        }
    }

    pub fn read(
        central_root: &Path,
        ground_relations_subject_ref: Option<String>,
    ) -> PasuIdentityState {
        let manifest_path = PASU_IDENTITY_MANIFEST_PATH.to_owned();
        let manifest = match PasuIdentityManifest::load(central_root) {
            Ok(manifest) => manifest,
            Err(error @ PasuManifestError::Io(_)) => {
                let present = central_root.join(&manifest_path).is_file();
                return PasuIdentityState {
                    present,
                    error: Some(error.to_string()),
                    ground_relations_subject_ref,
                    ..PasuIdentityState::absent()
                };
            }
            Err(error) => {
                return PasuIdentityState {
                    present: true,
                    error: Some(error.to_string()),
                    ground_relations_subject_ref,
                    ..PasuIdentityState::absent()
                };
            }
        };

        let sourced_files = manifest
            .identity_source
            .sources
            .iter()
            .map(|source| {
                let path = central_root.join(&source.path);
                let present = path.is_file();
                let content_revision = if present {
                    fs::read(&path).ok().map(|bytes| content_revision(&bytes))
                } else {
                    None
                };
                PasuSourcedFileState {
                    path: source.path.clone(),
                    present,
                    content_revision,
                }
            })
            .collect();

        let subject_ref_consistent = match (&ground_relations_subject_ref, &manifest.subject.ref_) {
            (Some(declared), _) => Some(
                PasuRef::parse(declared)
                    .map(|declared| declared == manifest.subject.ref_)
                    .unwrap_or(false),
            ),
            (None, _) => None,
        };

        PasuIdentityState {
            manifest_path,
            present: true,
            subject_ref: Some(manifest.subject.ref_.0.clone()),
            form: Some(manifest.subject.ref_.form().as_str().to_owned()),
            manifest_revision: Some(manifest.revision),
            identity_source_path: Some(manifest.identity_source.path.clone()),
            sourced_files,
            ground_relations_subject_ref,
            subject_ref_consistent,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempdir;

    fn sample_manifest() -> PasuIdentityManifest {
        PasuIdentityManifest::new(
            PasuSubject {
                ref_: PasuRef::local_nara(),
                title: Some("Nara identity source".to_owned()),
            },
            PasuIdentitySource {
                path: PASU_IDENTITY_SOURCE_DIR.to_owned(),
                provenance_law: "vault-first; promote by recognised promotion".to_owned(),
                sources: vec![
                    PasuSourcedFile {
                        path: "Control/user/identity/present.md".to_owned(),
                        standing: "authored-ground".to_owned(),
                        promoted: None,
                        revision: None,
                    },
                    PasuSourcedFile {
                        path: "Control/user/identity/sources/natal-chart.md".to_owned(),
                        standing: "promoted-source".to_owned(),
                        promoted: Some("2026-09-03".to_owned()),
                        revision: Some("1".to_owned()),
                    },
                ],
            },
        )
    }

    #[test]
    fn pasu_ref_grammar_parses_all_three_forms_and_rejects_the_rest() {
        for (raw, form, id) in [
            ("central:pasu:nara:local", PasuForm::Nara, "local"),
            (
                "central:pasu:agent:actuation/session-7",
                PasuForm::Agent,
                "actuation/session-7",
            ),
            (
                "central:pasu:agent-set:control-operators",
                PasuForm::AgentSet,
                "control-operators",
            ),
        ] {
            let parsed = PasuRef::parse(raw).expect("valid ref");
            assert_eq!(parsed.form(), form);
            assert_eq!(parsed.subject_id(), id);
            assert_eq!(parsed.0, raw);
        }
        assert!(matches!(PasuRef::parse(""), Err(PasuRefError::Empty)));
        assert!(matches!(
            PasuRef::parse("central:user"),
            Err(PasuRefError::NotPasu)
        ));
        assert!(matches!(
            PasuRef::parse("central:pasu:human:local"),
            Err(PasuRefError::UnknownForm(_))
        ));
        assert!(matches!(
            PasuRef::parse("central:pasu:agent:"),
            Err(PasuRefError::MissingId)
        ));
        assert!(matches!(
            PasuRef::parse("central:pasu:agent"),
            Err(PasuRefError::MissingId)
        ));
    }

    #[test]
    fn pasu_ref_round_trips_through_serde() {
        let reference = PasuRef::local_nara();
        let encoded = serde_json::to_string(&reference).unwrap();
        assert_eq!(encoded, "\"central:pasu:nara:local\"");
        let decoded: PasuRef = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, reference);
    }

    #[test]
    fn agent_and_agent_set_forms_derive_from_existing_refs() {
        assert_eq!(
            PasuRef::for_agent("session-runner").unwrap().as_str(),
            "central:pasu:agent:session-runner"
        );
        assert_eq!(
            PasuRef::for_agent_set("control-operators")
                .unwrap()
                .as_str(),
            "central:pasu:agent-set:control-operators"
        );
        assert!(matches!(
            PasuRef::for_agent("  "),
            Err(PasuRefError::MissingId)
        ));
    }

    #[test]
    fn manifest_round_trips_through_disk_and_validates_grammar() {
        let root = tempdir().unwrap();
        let root = root.path().to_path_buf();
        let manifest = sample_manifest();
        manifest.persist(&root).unwrap();
        let loaded = PasuIdentityManifest::load(&root).unwrap();
        assert_eq!(loaded, manifest);
        assert_eq!(loaded.schema, PASU_IDENTITY_MANIFEST_SCHEMA);
    }

    #[test]
    fn manifest_rejects_non_nara_subjects_and_sources_outside_the_anchor() {
        let mut manifest = sample_manifest();
        manifest.subject.ref_ = PasuRef::for_agent("session-runner").unwrap();
        assert!(matches!(
            manifest.validate(),
            Err(PasuManifestError::SubjectRef(_))
        ));

        let mut manifest = sample_manifest();
        manifest.identity_source.sources[0].path = "Control/user/context/other.md".to_owned();
        assert!(matches!(
            manifest.validate(),
            Err(PasuManifestError::SourceOutsideAnchor(_))
        ));

        let mut manifest = sample_manifest();
        manifest.identity_source.path = "Control/user/other".to_owned();
        assert!(matches!(
            manifest.validate(),
            Err(PasuManifestError::SourceOutsideAnchor(_))
        ));

        let mut manifest = sample_manifest();
        manifest.schema = "central.pasu/v0".to_owned();
        assert!(matches!(
            manifest.validate(),
            Err(PasuManifestError::UnsupportedSchema)
        ));
    }

    #[test]
    fn identity_state_reads_the_real_carrier_and_reports_absence_as_data() {
        let root = tempdir().unwrap();
        let root = root.path().to_path_buf();
        let state = PasuIdentityState::read(&root, None);
        assert!(!state.present);
        assert!(state.error.is_some());

        fs::create_dir_all(root.join(PASU_IDENTITY_SOURCE_DIR).join("sources")).unwrap();
        fs::write(
            root.join("Control/user/identity/present.md"),
            "who I am now",
        )
        .unwrap();
        fs::write(
            root.join("Control/user/identity/sources/natal-chart.md"),
            "promoted copy",
        )
        .unwrap();
        sample_manifest().persist(&root).unwrap();

        let state = PasuIdentityState::read(&root, Some("central:pasu:nara:local".to_owned()));
        assert!(state.present);
        assert_eq!(
            state.subject_ref.as_deref(),
            Some("central:pasu:nara:local")
        );
        assert_eq!(state.form.as_deref(), Some("nara"));
        assert_eq!(state.subject_ref_consistent, Some(true));
        assert_eq!(state.sourced_files.len(), 2);
        assert!(state.sourced_files[0].present);
        assert!(state.sourced_files[0].content_revision.is_some());
    }

    #[test]
    fn identity_state_flags_a_subject_ref_disagreement() {
        let root = tempdir().unwrap();
        let root = root.path().to_path_buf();
        fs::create_dir_all(root.join(PASU_IDENTITY_SOURCE_DIR)).unwrap();
        sample_manifest().persist(&root).unwrap();
        let state = PasuIdentityState::read(&root, Some("central:pasu:agent:other".to_owned()));
        assert_eq!(state.subject_ref_consistent, Some(false));
    }

    #[test]
    fn content_revision_is_deterministic_and_sensitive() {
        let a = content_revision(b"who I am now");
        let b = content_revision(b"who I am now");
        let c = content_revision(b"who I am now, revised");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
