use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{
    projectcentral_paths, read_project_manifest, ProjectCentralManifest, PROJECTCENTRAL_DIR,
    PROJECT_MANIFEST, ROOT_WIKI_SOURCE, WIKI_PROFILE, WIKI_SOURCE,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::wiki_publication::Publication;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ROOT_WIKI_REF: &str = "central:wiki:root";
pub const PROJECT_PROVENANCE: &str = "ProjectCentral/provenance.json";
const MAX_WIKI_SCAN_DEPTH: usize = 5;
const MAX_WIKI_BYTES: u64 = 8 * 1024 * 1024;

/// In-memory progress of this native operation, never another journal or
/// identity. A later refusal cannot erase already-acknowledged source writes.
#[derive(Default)]
pub(crate) struct MutationProgress {
    completed_sources: Vec<PathBuf>,
}
#[derive(Debug)]
struct MutationIncomplete {
    completed_sources: Vec<PathBuf>,
    failed_source: PathBuf,
    cause: io::Error,
}
impl std::fmt::Display for MutationIncomplete {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter,
            "Native mutation retained {} completed source publications before {} failed: {}; inspect the retained sources before retrying",
            self.completed_sources.len(), self.failed_source.display(), self.cause)
    }
}
impl std::error::Error for MutationIncomplete {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
impl MutationProgress {
    pub(crate) fn publish(
        &mut self,
        source: &Path,
        operation: impl FnOnce() -> io::Result<bool>,
    ) -> io::Result<()> {
        if self.check(source, operation())? {
            self.completed_sources.push(source.to_path_buf());
        }
        Ok(())
    }
    pub(crate) fn check<T>(&self, source: &Path, result: io::Result<T>) -> io::Result<T> {
        result.map_err(|cause| self.failure(source, cause))
    }
    fn failure(&self, source: &Path, cause: io::Error) -> io::Error {
        if self.completed_sources.is_empty() {
            cause
        } else {
            io::Error::new(
                cause.kind(),
                MutationIncomplete {
                    completed_sources: self.completed_sources.clone(),
                    failed_source: source.to_path_buf(),
                    cause,
                },
            )
        }
    }
}

/// Preserve native effect facts before a generic IO-kind formatter handles a
/// refusal. No Action here defines an operation_ref; the actual Action and
/// selected source remain the identity available to the caller.
pub(crate) fn mutation_failure_result(action: &str, error: &io::Error) -> Option<ActionResult> {
    let incomplete = error
        .get_ref()
        .and_then(|payload| payload.downcast_ref::<MutationIncomplete>());
    let cause = incomplete.map_or(error, |failure| &failure.cause);
    let uncertain = crate::wiki_publication::uncertainty(cause);
    if incomplete.is_none() && uncertain.is_none() {
        return None;
    }
    let original = uncertain.map_or(cause, |failure| &failure.cause);
    let code = if uncertain.is_some() {
        "central.publication_uncertain"
    } else {
        "central.mutation_incomplete"
    };
    let source = uncertain
        .map(|failure| &failure.source_path)
        .or_else(|| incomplete.map(|failure| &failure.failed_source));
    Some(ActionResult::failure_coded(
        Some(action),
        ResultStatus::PartialCompletion,
        code,
        error.to_string(),
        Some(json!({
            "published": uncertain.map_or(true, |failure| failure.published),
            "outcome": if uncertain.is_some() { "unknown" } else { "partial" },
            "source_path": source,
            "completed_sources": incomplete.map(|failure| &failure.completed_sources).cloned().unwrap_or_default(),
            "failed_source": incomplete.map(|failure| &failure.failed_source),
            "cause": { "kind": format!("{:?}", original.kind()), "raw_os_error": original.raw_os_error(), "message": original.to_string() },
            "automatic_retry": false,
        })),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCentralOutcome {
    AlreadyConformant,
    BindExistingWikiInPlace,
    CreateProjectCentral,
    MigrateSelectedMaterial,
    UnresolvedHumanDecisionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiCandidate {
    pub source: String,
    pub space_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSignal {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectCentralInspection {
    pub project_root: PathBuf,
    pub outcome: ProjectCentralOutcome,
    pub manifest: Option<ProjectCentralManifest>,
    pub manifest_errors: Vec<String>,
    pub wiki_candidates: Vec<WikiCandidate>,
    pub source_signals: Vec<SourceSignal>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub valid: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectCentralDoctor {
    pub project_root: PathBuf,
    pub valid: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MutationPlan {
    pub outcome: ProjectCentralOutcome,
    pub project_root: PathBuf,
    pub operations: Vec<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub preserves_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectCentralMutation {
    pub outcome: ProjectCentralOutcome,
    pub project_root: PathBuf,
    pub project_id: String,
    pub human_source: String,
    pub wiki_source: String,
    pub wiki_space_ref: String,
    pub adopted_sources: Vec<String>,
    pub root_wiki: PathBuf,
    pub provenance: PathBuf,
}

pub fn inspect_projectcentral(project_root: &Path) -> io::Result<ProjectCentralInspection> {
    let source_signals = discover_source_signals(project_root);
    let manifest_path = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);

    if manifest_path.exists() {
        let manifest = match read_project_manifest(project_root) {
            Ok(manifest) => manifest,
            Err(error) => {
                return Ok(ProjectCentralInspection {
                    project_root: project_root.to_path_buf(),
                    outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                    manifest: None,
                    manifest_errors: vec![error.to_string()],
                    wiki_candidates: vec![],
                    source_signals,
                    reason: "ProjectCentral manifest exists but cannot be read; mutation requires a human decision.".into(),
                });
            }
        };
        let validation = manifest.validate();
        if !validation.valid {
            return Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                manifest: Some(manifest),
                manifest_errors: validation.errors,
                wiki_candidates: vec![],
                source_signals,
                reason: "ProjectCentral manifest is present but invalid.".into(),
            });
        }

        let paths = projectcentral_paths(project_root, &manifest);
        let Some(space_ref) = compatible_wiki(&paths.wiki_source)? else {
            return Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                manifest: Some(manifest),
                manifest_errors: vec![],
                wiki_candidates: vec![],
                source_signals,
                reason: "ProjectCentral manifest is valid but the canonical Agent Wiki is missing or incompatible.".into(),
            });
        };

        let mut wiki_candidates = vec![WikiCandidate {
            source: manifest.wiki.source.clone(),
            space_ref,
        }];
        for source in &manifest.wiki.adopted_sources {
            match compatible_wiki(&project_root.join(source))? {
                Some(space_ref) => wiki_candidates.push(WikiCandidate {
                    source: source.clone(),
                    space_ref,
                }),
                None => {
                    return Ok(ProjectCentralInspection {
                        project_root: project_root.to_path_buf(),
                        outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                        manifest: Some(manifest.clone()),
                        manifest_errors: vec![],
                        wiki_candidates,
                        source_signals,
                        reason: format!("ProjectCentral adopted Wiki source is missing or incompatible: {source}"),
                    });
                }
            }
        }

        let fractal_dirs_present = paths.human_source.is_dir()
            && paths.agent_governance.is_dir()
            && paths.wiki_root.is_dir();
        return Ok(ProjectCentralInspection {
            project_root: project_root.to_path_buf(),
            outcome: if fractal_dirs_present {
                ProjectCentralOutcome::AlreadyConformant
            } else {
                ProjectCentralOutcome::UnresolvedHumanDecisionRequired
            },
            manifest: Some(manifest),
            manifest_errors: vec![],
            wiki_candidates,
            source_signals,
            reason: if fractal_dirs_present {
                "ProjectCentral human-source and Agent-Wiki fractal is present and readable.".into()
            } else {
                "ProjectCentral metadata exists but the canonical user/agents directory relation is incomplete.".into()
            },
        });
    }

    let wiki_candidates = discover_wiki_candidates(project_root)?;
    let (outcome, reason) = match wiki_candidates.len() {
        0 => (
            ProjectCentralOutcome::CreateProjectCentral,
            "No compatible OKF Wiki source was found; the canonical human-source/Agent-Wiki fractal can be created around the native Project.",
        ),
        1 => (
            ProjectCentralOutcome::BindExistingWikiInPlace,
            "One compatible OKF Wiki source was found; it can remain in place as a participating source while ProjectCentral receives its canonical Agent Wiki.",
        ),
        _ => (
            ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
            "Multiple compatible Wiki sources were found; Central will not guess which one should participate as the adopted source.",
        ),
    };
    Ok(ProjectCentralInspection {
        project_root: project_root.to_path_buf(),
        outcome,
        manifest: None,
        manifest_errors: vec![],
        wiki_candidates,
        source_signals,
        reason: reason.into(),
    })
}

pub fn doctor_projectcentral(
    central_root: &Path,
    project_root: &Path,
) -> io::Result<ProjectCentralDoctor> {
    let mut checks = vec![];
    let manifest = match read_project_manifest(project_root) {
        Ok(manifest) => {
            let validation = manifest.validate();
            checks.push(DoctorCheck {
                name: "manifest".into(),
                valid: validation.valid,
                detail: if validation.valid {
                    "central.project/v1 manifest is valid".into()
                } else {
                    validation.errors.join("; ")
                },
            });
            Some(manifest)
        }
        Err(error) => {
            checks.push(DoctorCheck {
                name: "manifest".into(),
                valid: false,
                detail: error.to_string(),
            });
            None
        }
    };

    if let Some(manifest) = manifest {
        let paths = projectcentral_paths(project_root, &manifest);
        checks.push(DoctorCheck {
            name: "human_source".into(),
            valid: paths.human_source.is_dir(),
            detail: paths.human_source.display().to_string(),
        });
        checks.push(DoctorCheck {
            name: "agent_governance".into(),
            valid: paths.agent_governance.is_dir(),
            detail: paths.agent_governance.display().to_string(),
        });
        checks.push(DoctorCheck {
            name: "agent_wiki_root".into(),
            valid: paths.wiki_root.is_dir(),
            detail: paths.wiki_root.display().to_string(),
        });
        let space_ref = compatible_wiki(&paths.wiki_source)?;
        checks.push(DoctorCheck {
            name: "agent_wiki_source".into(),
            valid: space_ref.is_some(),
            detail: paths.wiki_source.display().to_string(),
        });

        for source in &manifest.wiki.adopted_sources {
            checks.push(DoctorCheck {
                name: format!("adopted_wiki:{source}"),
                valid: compatible_wiki(&project_root.join(source))?.is_some(),
                detail: project_root.join(source).display().to_string(),
            });
        }

        if let Some(space_ref) = space_ref {
            let root_source = central_root.join(ROOT_WIKI_SOURCE);
            checks.push(DoctorCheck {
                name: "root_federation".into(),
                valid: root_contains_child(&root_source, &space_ref)?,
                detail: format!("{} -> {space_ref}", root_source.display()),
            });
        }
    }

    Ok(ProjectCentralDoctor {
        project_root: project_root.to_path_buf(),
        valid: checks.iter().all(|check| check.valid),
        checks,
    })
}

pub fn initialize_projectcentral(
    central_root: &Path,
    project_root: &Path,
    project_id: &str,
) -> io::Result<ProjectCentralMutation> {
    initialize_projectcentral_with_progress(central_root, project_root, project_id)
        .map(|(mutation, _)| mutation)
}
fn initialize_projectcentral_with_progress(
    central_root: &Path,
    project_root: &Path,
    project_id: &str,
) -> io::Result<(ProjectCentralMutation, MutationProgress)> {
    ensure_project_directory(project_root)?;
    ensure_unbound(project_root)?;
    let manifest = ProjectCentralManifest::new(project_id);
    validate_manifest(&manifest)?;
    let paths = projectcentral_paths(project_root, &manifest);
    create_fractal_dirs(&paths)?;
    let mut progress = MutationProgress::default();
    progress.publish(&paths.manifest, || {
        write_json_new(
            &paths.manifest,
            &serde_json::to_value(&manifest).expect("manifest serializes"),
        )?;
        Ok(true)
    })?;

    let space_ref = project_space_ref(project_id);
    progress.publish(&paths.wiki_source, || {
        write_json_new(
            &paths.wiki_source,
            &project_wiki_value(&space_ref, project_id, &[]),
        )?;
        Ok(true)
    })?;
    progress.publish(&central_root.join(ROOT_WIKI_SOURCE), || {
        ensure_root_federation_status(central_root, Some(&space_ref)).map(|(_, changed)| changed)
    })?;
    let provenance = project_root.join(PROJECT_PROVENANCE);
    progress.publish(&provenance, || {
        append_provenance(
            project_root,
            "initialize",
            None,
            Some(&manifest.wiki.source),
        )?;
        Ok(true)
    })?;
    Ok((
        mutation_result(
            ProjectCentralOutcome::CreateProjectCentral,
            project_root,
            project_id,
            &manifest,
            space_ref,
            central_root,
            provenance,
        ),
        progress,
    ))
}

pub fn preview_adopt(project_root: &Path, source: &str) -> io::Result<MutationPlan> {
    ensure_project_member(source)?;
    if compatible_wiki(&project_root.join(source))?.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "selected source is not a compatible okf-wiki/v1 object collection",
        ));
    }
    Ok(MutationPlan {
        outcome: ProjectCentralOutcome::BindExistingWikiInPlace,
        project_root: project_root.to_path_buf(),
        operations: vec![
            "create ProjectCentral/user and ProjectCentral/agents/{governance,wiki}".into(),
            "create canonical ProjectCentral Agent Wiki".into(),
            format!("retain adopted Wiki source in place: {source}"),
            "record the in-place source as a participating Wiki source".into(),
            "federate the canonical Project WikiSpace from the Central root".into(),
        ],
        source: Some(source.into()),
        target: Some(WIKI_SOURCE.into()),
        preserves_source: true,
    })
}

pub fn adopt_in_place(
    central_root: &Path,
    project_root: &Path,
    project_id: &str,
    source: &str,
) -> io::Result<ProjectCentralMutation> {
    preview_adopt(project_root, source)?;
    ensure_unbound(project_root)?;
    let adopted_space_ref = compatible_wiki(&project_root.join(source))?
        .expect("preview established Wiki compatibility");
    let mut manifest = ProjectCentralManifest::new(project_id);
    manifest.wiki.adopted_sources.push(source.into());
    validate_manifest(&manifest)?;
    let paths = projectcentral_paths(project_root, &manifest);
    create_fractal_dirs(&paths)?;
    let mut progress = MutationProgress::default();
    progress.publish(&paths.manifest, || {
        write_json_new(
            &paths.manifest,
            &serde_json::to_value(&manifest).expect("manifest serializes"),
        )?;
        Ok(true)
    })?;

    let space_ref = project_space_ref(project_id);
    progress.publish(&paths.wiki_source, || {
        write_json_new(
            &paths.wiki_source,
            &project_wiki_value(&space_ref, project_id, &[adopted_space_ref]),
        )?;
        Ok(true)
    })?;
    progress.publish(&central_root.join(ROOT_WIKI_SOURCE), || {
        ensure_root_federation_status(central_root, Some(&space_ref)).map(|(_, changed)| changed)
    })?;
    let provenance = progress.check(
        &project_root.join(PROJECT_PROVENANCE),
        append_provenance(
            project_root,
            "adopt_in_place",
            Some(source),
            Some(&manifest.wiki.source),
        ),
    )?;
    Ok(mutation_result(
        ProjectCentralOutcome::BindExistingWikiInPlace,
        project_root,
        project_id,
        &manifest,
        space_ref,
        central_root,
        provenance,
    ))
}

pub fn preview_migrate(project_root: &Path, source: &str) -> io::Result<MutationPlan> {
    ensure_project_member(source)?;
    if compatible_wiki(&project_root.join(source))?.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "selected source is not a compatible okf-wiki/v1 object collection",
        ));
    }
    if project_root.join(WIKI_SOURCE).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("migration target already exists: {WIKI_SOURCE}"),
        ));
    }
    Ok(MutationPlan {
        outcome: ProjectCentralOutcome::MigrateSelectedMaterial,
        project_root: project_root.to_path_buf(),
        operations: vec![
            "create ProjectCentral/user and ProjectCentral/agents/{governance,wiki}".into(),
            format!("copy selected Wiki source: {source} -> {WIKI_SOURCE}"),
            "preserve the original source in place".into(),
            "bind ProjectCentral to the copied Agent Wiki".into(),
            "record migration provenance".into(),
            "federate the migrated WikiSpace from the Central root".into(),
        ],
        source: Some(source.into()),
        target: Some(WIKI_SOURCE.into()),
        preserves_source: true,
    })
}

pub fn migrate_selected(
    central_root: &Path,
    project_root: &Path,
    project_id: &str,
    source: &str,
) -> io::Result<ProjectCentralMutation> {
    preview_migrate(project_root, source)?;
    ensure_unbound(project_root)?;
    let manifest = ProjectCentralManifest::new(project_id);
    validate_manifest(&manifest)?;
    let paths = projectcentral_paths(project_root, &manifest);
    create_fractal_dirs(&paths)?;
    let mut progress = MutationProgress::default();
    progress.publish(&paths.wiki_source, || {
        Publication::acquire(&paths.wiki_source)?.copy_new(&project_root.join(source))?;
        Ok(true)
    })?;
    let space_ref = progress.check(
        &paths.wiki_source,
        compatible_wiki(&paths.wiki_source).and_then(|value| {
            value.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "copied Wiki failed compatibility verification",
                )
            })
        }),
    )?;
    progress.publish(&paths.manifest, || {
        write_json_new(
            &paths.manifest,
            &serde_json::to_value(&manifest).expect("manifest serializes"),
        )?;
        Ok(true)
    })?;
    progress.publish(&central_root.join(ROOT_WIKI_SOURCE), || {
        ensure_root_federation_status(central_root, Some(&space_ref)).map(|(_, changed)| changed)
    })?;
    let provenance = progress.check(
        &project_root.join(PROJECT_PROVENANCE),
        append_provenance(
            project_root,
            "migrate_copy",
            Some(source),
            Some(WIKI_SOURCE),
        ),
    )?;
    Ok(mutation_result(
        ProjectCentralOutcome::MigrateSelectedMaterial,
        project_root,
        project_id,
        &manifest,
        space_ref,
        central_root,
        provenance,
    ))
}

pub fn ensure_root_federation(central_root: &Path, child_ref: Option<&str>) -> io::Result<PathBuf> {
    ensure_root_federation_status(central_root, child_ref).map(|(path, _)| path)
}
fn ensure_root_federation_status(
    central_root: &Path,
    child_ref: Option<&str>,
) -> io::Result<(PathBuf, bool)> {
    let path = central_root.join(ROOT_WIKI_SOURCE);
    fs::create_dir_all(path.parent().expect("root Wiki parent"))?;
    let publication = Publication::acquire(&path)?;
    let mut changed = publication.bytes().is_none();
    let mut value = if let Some(bytes) = publication.bytes() {
        serde_json::from_slice::<Value>(bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not valid Wiki JSON: {error}", path.display()),
            )
        })?
    } else {
        json!({"objects": [root_space_value()]})
    };

    let objects = value
        .get_mut("objects")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "root Wiki requires an objects list",
            )
        })?;
    let root_index = objects.iter().position(|object| {
        object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
            && object.get("object").and_then(Value::as_str) == Some("space")
            && object.get("ref").and_then(Value::as_str) == Some(ROOT_WIKI_REF)
    });
    let root_index = match root_index {
        Some(index) => index,
        None if objects.is_empty() => {
            objects.push(root_space_value());
            changed = true;
            0
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} does not contain the canonical Central root WikiSpace",
                    path.display()
                ),
            ));
        }
    };

    if let Some(child_ref) = child_ref {
        let object = objects[root_index].as_object_mut().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "root WikiSpace must be an object",
            )
        })?;
        let children = object
            .entry("child_space_refs")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "root child_space_refs must be an array",
                )
            })?;
        if !children
            .iter()
            .any(|entry| entry.as_str() == Some(child_ref))
        {
            children.push(Value::String(child_ref.into()));
            children.sort_by(|a, b| {
                a.as_str()
                    .unwrap_or_default()
                    .cmp(b.as_str().unwrap_or_default())
            });
            let revision = match object.get("revision") {
                None => 1,
                Some(value) => {
                    value
                        .as_u64()
                        .filter(|revision| *revision > 0)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "root Wiki revision must be a positive integer",
                            )
                        })?
                }
            }
            .checked_add(1)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "root Wiki revision is exhausted",
                )
            })?;
            object.insert("revision".into(), Value::from(revision));
            changed = true;
        }
    }
    let published = if changed {
        publication.replace(&json_bytes(&value)?)?
    } else if let Some(bytes) = publication.bytes() {
        publication.replace(bytes)?
    } else {
        false
    };
    Ok((path, published))
}

fn root_contains_child(path: &Path, child_ref: &str) -> io::Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let value: Value = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(value
        .get("objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|object| {
            object.get("ref").and_then(Value::as_str) == Some(ROOT_WIKI_REF)
                && object
                    .get("child_space_refs")
                    .and_then(Value::as_array)
                    .is_some_and(|children| {
                        children
                            .iter()
                            .any(|entry| entry.as_str() == Some(child_ref))
                    })
        }))
}

fn discover_wiki_candidates(project_root: &Path) -> io::Result<Vec<WikiCandidate>> {
    let mut paths = vec![];
    collect_json(project_root, 0, &mut paths)?;
    let mut candidates = vec![];
    for path in paths {
        if let Some(space_ref) = compatible_wiki(&path)? {
            let relative = path.strip_prefix(project_root).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "discovered Wiki escaped Project root",
                )
            })?;
            candidates.push(WikiCandidate {
                source: relative.to_string_lossy().replace('\\', "/"),
                space_ref,
            });
        }
    }
    candidates.sort_by(|a, b| a.source.cmp(&b.source));
    candidates.dedup_by(|a, b| a.source == b.source);
    Ok(candidates)
}

fn collect_json(current: &Path, depth: usize, output: &mut Vec<PathBuf>) -> io::Result<()> {
    if depth > MAX_WIKI_SCAN_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if matches!(
                name.as_ref(),
                ".git"
                    | ".central"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | PROJECTCENTRAL_DIR
            ) {
                continue;
            }
            collect_json(&path, depth + 1, output)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            && fs::metadata(&path)?.len() <= MAX_WIKI_BYTES
        {
            output.push(path);
        }
    }
    Ok(())
}

/// Post-update check for a project's wiki: the manifest is valid, the wiki
/// exists at the manifest-declared source, parses, carries the declared
/// profile on at least its space object, and presents an objects array.
/// Read-only; every failed check names the file it examined.
fn wiki_check(project_root: &Path) -> io::Result<Value> {
    wiki_check_with_source(project_root).map_err(|(_, cause)| cause)
}
fn wiki_check_with_source(project_root: &Path) -> Result<Value, (PathBuf, io::Error)> {
    let manifest_path = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
    let manifest = crate::projectcentral::read_project_manifest(project_root)
        .map_err(|cause| (manifest_path, cause))?;
    let validation = manifest.validate();
    let mut checks = vec![json!({"check": "manifest-valid", "ok": validation.valid})];
    if !validation.valid {
        checks[0]["errors"] = json!(validation.errors);
    }
    let wiki_path = project_root.join(&manifest.wiki.source);
    let present = wiki_path.is_file();
    checks.push(json!({
        "check": "wiki-present",
        "ok": present,
        "path": manifest.wiki.source,
    }));
    let mut parsed = json!({"check": "wiki-parses", "ok": false, "path": manifest.wiki.source});
    let mut profile_ok = false;
    if present {
        match serde_json::from_slice::<Value>(
            &fs::read(&wiki_path).map_err(|cause| (wiki_path.clone(), cause))?,
        ) {
            Ok(value) => {
                parsed["ok"] = json!(true);
                parsed["objects"] = json!(value
                    .get("objects")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len));
                checks.push(parsed);
                // This reuses Central's existing source-compatibility
                // recognition. Semantic Wiki validation remains AIKit-owned.
                profile_ok = compatible_wiki_value(&value, &manifest.wiki.profile).is_some();
                checks.push(json!({
                    "check": "wiki-profile",
                    "ok": profile_ok,
                    "profile": manifest.wiki.profile,
                }));
            }
            Err(error) => {
                parsed["error"] = json!(error.to_string());
                checks.push(parsed);
            }
        }
    }
    let ok = validation.valid && present && profile_ok;
    Ok(json!({
        "ok": ok,
        "project_root": project_root.display().to_string(),
        "wiki_source": manifest.wiki.source,
        "checks": checks,
    }))
}

fn compatible_wiki(path: &Path) -> io::Result<Option<String>> {
    if !path.is_file() || fs::metadata(path)?.len() > MAX_WIKI_BYTES {
        return Ok(None);
    }
    let value: Value = match serde_json::from_slice(&fs::read(path)?) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(compatible_wiki_value(&value, WIKI_PROFILE))
}
fn compatible_wiki_value(value: &Value, profile: &str) -> Option<String> {
    let objects = value.get("objects").and_then(Value::as_array)?;
    objects.iter().find_map(|object| {
        if object.get("profile").and_then(Value::as_str) == Some(profile)
            && object.get("object").and_then(Value::as_str) == Some("space")
        {
            object
                .get("ref")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        } else {
            None
        }
    })
}

fn discover_source_signals(project_root: &Path) -> Vec<SourceSignal> {
    [
        ("readme", "README.md"),
        ("docs", "docs"),
        ("design", "design"),
        ("obsidian", ".obsidian"),
        ("wiki", "Wiki"),
        ("wiki", "wiki"),
    ]
    .into_iter()
    .filter(|(_, relative)| project_root.join(relative).exists())
    .map(|(kind, path)| SourceSignal {
        kind: kind.into(),
        path: path.into(),
    })
    .collect()
}

fn validate_manifest(manifest: &ProjectCentralManifest) -> io::Result<()> {
    let validation = manifest.validate();
    if validation.valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            validation.errors.join("; "),
        ))
    }
}

pub(crate) fn project_space_ref(project_id: &str) -> String {
    format!("central:wiki:project:{project_id}")
}

pub(crate) fn project_wiki_value(
    space_ref: &str,
    title: &str,
    child_space_refs: &[String],
) -> Value {
    json!({"objects":[{
        "profile":WIKI_PROFILE,
        "object":"space",
        "ref":space_ref,
        "revision":1,
        "provenance":[],
        "title":title,
        "parent_space_refs":[ROOT_WIKI_REF],
        "child_space_refs":child_space_refs,
        "node_refs":[]
    }]})
}

fn root_space_value() -> Value {
    json!({
        "profile":WIKI_PROFILE,
        "object":"space",
        "ref":ROOT_WIKI_REF,
        "revision":1,
        "provenance":[],
        "title":"Central",
        "parent_space_refs":[],
        "child_space_refs":[],
        "node_refs":[]
    })
}

fn create_fractal_dirs(paths: &crate::projectcentral::ProjectCentralPaths) -> io::Result<()> {
    fs::create_dir_all(&paths.human_source)?;
    fs::create_dir_all(&paths.agent_governance)?;
    fs::create_dir_all(&paths.wiki_root)?;
    Ok(())
}

fn mutation_result(
    outcome: ProjectCentralOutcome,
    project_root: &Path,
    project_id: &str,
    manifest: &ProjectCentralManifest,
    wiki_space_ref: String,
    central_root: &Path,
    provenance: PathBuf,
) -> ProjectCentralMutation {
    ProjectCentralMutation {
        outcome,
        project_root: project_root.to_path_buf(),
        project_id: project_id.into(),
        human_source: manifest.human_source.clone(),
        wiki_source: manifest.wiki.source.clone(),
        wiki_space_ref,
        adopted_sources: manifest.wiki.adopted_sources.clone(),
        root_wiki: central_root.join(ROOT_WIKI_SOURCE),
        provenance,
    }
}

fn ensure_project_directory(project_root: &Path) -> io::Result<()> {
    if project_root.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Project root does not exist as a directory: {}",
                project_root.display()
            ),
        ))
    }
}

fn ensure_unbound(project_root: &Path) -> io::Result<()> {
    ensure_project_directory(project_root)?;
    if project_root
        .join(PROJECTCENTRAL_DIR)
        .join(PROJECT_MANIFEST)
        .exists()
    {
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "ProjectCentral is already bound; inspect/doctor it rather than replacing its source implicitly.",
        ))
    } else {
        Ok(())
    }
}

fn ensure_project_member(raw: &str) -> io::Result<()> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source must be a non-empty project-root-relative path",
        ));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source must remain inside the Project and may not contain parent/root components",
        ));
    }
    Ok(())
}

pub(crate) fn write_json_new(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Publication::acquire(path)?.create_new(&json_bytes(value)?)
}

fn json_bytes(value: &Value) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
fn write_json_replace(path: &Path, value: &Value) -> io::Result<()> {
    fs::create_dir_all(path.parent().expect("JSON source parent"))?;
    Publication::acquire(path)?.replace(&json_bytes(value)?)?;
    Ok(())
}

fn append_provenance(
    project_root: &Path,
    action: &str,
    source: Option<&str>,
    target: Option<&str>,
) -> io::Result<PathBuf> {
    let path = project_root.join(PROJECT_PROVENANCE);
    let publication = Publication::acquire(&path)?;
    let mut value = if let Some(bytes) = publication.bytes() {
        serde_json::from_slice::<Value>(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    } else {
        json!({"schema":"central.project.provenance/v1","entries":[]})
    };
    let entries = value
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "ProjectCentral provenance entries must be an array",
            )
        })?;
    entries.push(json!({
        "action":action,
        "source":source,
        "target":target,
        "recorded_at_unix_seconds":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "source_preserved":source.is_some()
    }));
    publication.replace(&json_bytes(&value)?)?;
    Ok(path)
}

fn action_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.into(),
        input_type: "string".into(),
        required: true,
        choices: None,
        selection: None,
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
    inputs: &[&str],
    preview_supported: bool,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        inputs: inputs.iter().map(|name| action_input(name)).collect(),
        output: ActionOutputDefinition {
            output_type: output_type.into(),
        },
        mutation_class,
        preview_supported,
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn required(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires {field}."),
                None,
            )
        })
}

fn project_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<(PathBuf, PathBuf), ActionResult> {
    let project = required(input, "project", action)?;
    ensure_project_member(&project).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    let project_root = root.join("Work").join(project);
    ensure_project_directory(&project_root).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    Ok((root, project_root))
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    if let Some(result) = mutation_failure_result(action, &error) {
        return result;
    }
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists => {
            ResultStatus::InvalidInput
        }
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn wiki_check_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.wiki.check";
    let (_, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    wiki_check(&project_root)
        .map(|value| {
            ActionResult::success(
                action,
                json!({"automatic_agent_or_model_invocation": false, "result": value}),
            )
        })
        .unwrap_or_else(|error| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                error.to_string(),
                Some(json!({"project": input.get("project")})),
            )
        })
}

fn inspect_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.inspect";
    let (_, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    inspect_projectcentral(&project_root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("inspection serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn doctor_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.doctor";
    let (root, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match doctor_projectcentral(&root, &project_root) {
        Ok(report) if report.valid => ActionResult::success(
            action,
            serde_json::to_value(report).expect("doctor serializes"),
        ),
        Ok(report) => ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            "ProjectCentral verification failed.",
            Some(serde_json::to_value(report).expect("doctor serializes")),
        ),
        Err(error) => io_failure(action, error),
    }
}

fn init_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.init";
    let (root, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let project_id = match required(input, "project_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    initialize_projectcentral_with_progress(&root, &project_root, &project_id)
        .map(|(value, progress)| {
            let mut envelope = serde_json::to_value(value).expect("mutation serializes");
            // Post-update check: the wiki the init just wrote must parse and
            // carry the declared profile before the action reports success.
            let check = wiki_check_with_source(&project_root);
            if let Ok(check) = &check {
                if check.get("ok") == Some(&Value::Bool(true)) {
                    envelope["post_update_check"] = check.clone();
                    return ActionResult::success(action, envelope);
                }
            }
            let (failed_source, cause, check) = match check {
                Ok(check) => {
                    let manifest_failed = check["checks"].as_array().is_some_and(|checks| {
                        checks
                            .iter()
                            .any(|check| check["check"] == "manifest-valid" && check["ok"] == false)
                    });
                    let source = if manifest_failed {
                        project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST)
                    } else {
                        project_root.join(WIKI_SOURCE)
                    };
                    (
                        source,
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "ProjectCentral post-update source verification failed",
                        ),
                        check,
                    )
                }
                Err((source, cause)) => {
                    let check = json!({"ok":false,"path":source,"error":cause.to_string()});
                    (source, cause, check)
                }
            };
            let mut failed = io_failure(action, progress.failure(&failed_source, cause));
            if let Some(details) = failed
                .error
                .as_mut()
                .and_then(|error| error.details.as_mut())
                .and_then(Value::as_object_mut)
            {
                details.insert("post_update_check".into(), check);
                details.insert("mutation_receipt".into(), envelope);
            }
            failed
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn adopt_preview_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.adopt.preview";
    let (_, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source = match required(input, "source", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    preview_adopt(&project_root, &source)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("plan serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.adopt";
    let (root, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let project_id = match required(input, "project_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source = match required(input, "source", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    adopt_in_place(&root, &project_root, &project_id, &source)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("mutation serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn migrate_preview_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.migrate.preview";
    let (_, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source = match required(input, "source", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    preview_migrate(&project_root, &source)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("plan serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn migrate_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.migrate";
    let (root, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let project_id = match required(input, "project_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source = match required(input, "source", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    migrate_selected(&root, &project_root, &project_id, &source)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("mutation serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_projectcentral_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor("projectcentral.inspect", "Inspect ProjectCentral", "Inspect a Work project for the ProjectCentral human-source/Agent-Wiki fractal, compatible OKF Wiki sources, and adoption ambiguity without mutation.", MutationClass::ReadOnly, "projectcentral-inspection", &["project"], false),
            inspect_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (descriptor("projectcentral.doctor", "Verify ProjectCentral", "Verify ProjectCentral identity, human source root, Agent governance/Wiki roots, Wiki source, adopted sources, and root federation.", MutationClass::ReadOnly, "projectcentral-doctor", &["project"], false), doctor_action),
        (descriptor("projectcentral.wiki.check", "Check project wiki", "Post-update check: the manifest is valid, the wiki exists at the manifest-declared source, parses, and carries the declared profile. Read-only.", MutationClass::ReadOnly, "projectcentral-wiki-check", &["project"], false), wiki_check_action),
        (descriptor("projectcentral.init", "Initialize ProjectCentral", "Create the recursive user/agents ProjectCentral relation around an existing native Work project without moving native files.", MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id"], true), init_action),
        (descriptor("projectcentral.adopt.preview", "Preview Wiki adoption", "Preview retaining one selected compatible Wiki in place as a participating source of the canonical Project Agent Wiki.", MutationClass::ReadOnly, "projectcentral-mutation-plan", &["project", "source"], false), adopt_preview_action),
        (descriptor("projectcentral.adopt", "Adopt Wiki in place", "Keep one selected compatible Wiki in place, record it as a participating source, create the canonical Project Agent Wiki, and federate the Project WikiSpace.", MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id", "source"], true), adopt_action),
        (descriptor("projectcentral.migrate.preview", "Preview Wiki migration", "Preview copying one selected compatible Wiki into ProjectCentral/agents/wiki while preserving its source.", MutationClass::ReadOnly, "projectcentral-mutation-plan", &["project", "source"], false), migrate_preview_action),
        (descriptor("projectcentral.migrate", "Migrate selected Wiki", "Copy one selected compatible Wiki into ProjectCentral/agents/wiki, preserve the original, record provenance, and federate it.", MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id", "source"], true), migrate_action),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("ProjectCentral Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral::{AGENT_GOVERNANCE_DIR, HUMAN_SOURCE_DIR};
    use tempfile::tempdir;

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

    fn native_action(root: &Path, action: &str, input: Value) -> ActionResult {
        let registry = crate::cli::create_runtime_action_registry();
        let connectors = crate::create_default_connector_registry();
        let connector_context = crate::ConnectorContext::current();
        let options = crate::root::RootOptions {
            explicit_root: Some(root.to_path_buf()),
            configured_root: None,
            home: None,
        };
        registry.execute(
            action,
            &input,
            &ActionExecutionContext {
                root_options: &options,
                connectors: &connectors,
                connector_context: &connector_context,
            },
        )
    }

    #[test]
    fn actual_publication_uncertainty_survives_each_native_result_formatter() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        for action in [
            "central.init",
            "projectcentral.init",
            "central.world.reproject.apply",
        ] {
            let fixture = NativeFixture::new_in(&scratch);
            let root = fixture.path().join("world");
            let project = root.join("Work/example");
            fs::create_dir_all(&project).unwrap();
            if action != "central.init" {
                crate::root::initialize_central(&root).unwrap();
            }
            let retained = fixture.path().join("retained-published-parent");
            let retained_for_observer = retained.clone();
            crate::wiki_publication::tests::after_publication(move |source| {
                fs::rename(source.parent().unwrap(), &retained_for_observer).unwrap();
                fs::create_dir(source.parent().unwrap()).unwrap();
            });
            let result = native_action(
                &root,
                action,
                json!({"project":"example","project_id":"example/project"}),
            );
            assert!(
                !result.ok,
                "{action} must not classify failed readback as success"
            );
            assert_eq!(result.status, ResultStatus::PartialCompletion);
            let value = serde_json::to_value(&result).unwrap();
            assert_eq!(value["action"], action);
            assert_eq!(value["error"]["code"], "central.publication_uncertain");
            assert_eq!(value["error"]["details"]["published"], true);
            assert_eq!(value["error"]["details"]["outcome"], "unknown");
            assert_eq!(value["error"]["details"]["automatic_retry"], false);
            assert!(value["error"]["details"].get("operation_ref").is_none());
            let source = PathBuf::from(value["error"]["details"]["source_path"].as_str().unwrap());
            assert!(
                retained.join(source.file_name().unwrap()).is_file(),
                "the actual native publication must survive refusal"
            );
            assert!(!source.exists());
        }
    }

    fn change_wiki_after_provenance(wiki: PathBuf, retained: PathBuf, remove: bool) {
        crate::wiki_publication::tests::after_publication(move |source| {
            if source.file_name().and_then(|name| name.to_str()) == Some("provenance.json") {
                if remove {
                    fs::rename(&wiki, &retained).unwrap();
                } else {
                    let original = fs::read(&wiki).unwrap();
                    fs::write(&retained, &original).unwrap();
                    let mut value: Value = serde_json::from_slice(&original).unwrap();
                    value["objects"][0]["profile"] = json!("foreign-profile/v1");
                    fs::write(&wiki, serde_json::to_vec(&value).unwrap()).unwrap();
                }
            } else {
                change_wiki_after_provenance(wiki, retained, remove);
            }
        });
    }

    #[test]
    fn actual_post_update_profile_and_missing_source_failures_retain_mutation_receipt() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        for remove in [false, true] {
            let fixture = NativeFixture::new_in(&scratch);
            let root = fixture.path().join("world");
            let project = root.join("Work/example");
            fs::create_dir_all(&project).unwrap();
            crate::root::initialize_central(&root).unwrap();
            let retained = fixture.path().join("retained-actual-published-wiki.json");
            change_wiki_after_provenance(project.join(WIKI_SOURCE), retained.clone(), remove);
            let result = native_action(
                &root,
                "projectcentral.init",
                json!({"project":"example","project_id":"example/project"}),
            );
            assert!(!result.ok);
            assert_eq!(result.status, ResultStatus::PartialCompletion);
            let value = serde_json::to_value(result).unwrap();
            let details = &value["error"]["details"];
            assert_eq!(value["error"]["code"], "central.mutation_incomplete");
            assert_eq!(details["published"], true);
            assert_eq!(details["outcome"], "partial");
            assert_eq!(details["post_update_check"]["ok"], false);
            assert_eq!(details["mutation_receipt"]["project_id"], "example/project");
            assert_eq!(details["failed_source"], json!(project.join(WIKI_SOURCE)));
            assert_eq!(details["completed_sources"].as_array().unwrap().len(), 4);
            assert!(details["completed_sources"]
                .as_array()
                .unwrap()
                .contains(&json!(project.join(PROJECT_PROVENANCE))));
            // Missing source is an observed failed presence check, not an
            // invented filesystem read error: wiki_check returns that receipt.
            assert_eq!(details["cause"]["kind"], "InvalidData");
            let presence = details["post_update_check"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|check| check["check"] == "wiki-present")
                .unwrap();
            assert_eq!(presence["ok"], !remove);
            let actual: Value = serde_json::from_slice(&fs::read(&retained).unwrap()).unwrap();
            assert_eq!(actual["objects"][0]["profile"], WIKI_PROFILE);
            assert!(project
                .join(PROJECTCENTRAL_DIR)
                .join(PROJECT_MANIFEST)
                .is_file());
        }
    }

    fn remove_manifest_after_provenance(manifest: PathBuf, retained: PathBuf) {
        crate::wiki_publication::tests::after_publication(move |source| {
            if source.file_name().and_then(|name| name.to_str()) == Some("provenance.json") {
                fs::rename(&manifest, &retained).unwrap();
            } else {
                remove_manifest_after_provenance(manifest, retained);
            }
        });
    }

    #[test]
    fn actual_post_update_manifest_read_failure_retains_exact_failed_source() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let root = fixture.path().join("world");
        let project = root.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        crate::root::initialize_central(&root).unwrap();
        let manifest = project.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
        let retained = fixture.path().join("retained-actual-manifest.json");
        remove_manifest_after_provenance(manifest.clone(), retained.clone());
        let result = native_action(
            &root,
            "projectcentral.init",
            json!({"project":"example","project_id":"example/project"}),
        );
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::PartialCompletion);
        let value = serde_json::to_value(result).unwrap();
        let details = &value["error"]["details"];
        assert_eq!(value["error"]["code"], "central.mutation_incomplete");
        assert_eq!(details["published"], true);
        assert_eq!(details["failed_source"], json!(manifest));
        assert_eq!(details["cause"]["kind"], "NotFound");
        assert_eq!(details["post_update_check"]["path"], json!(manifest));
        assert_eq!(details["completed_sources"].as_array().unwrap().len(), 4);
        assert_eq!(details["mutation_receipt"]["project_id"], "example/project");
        let actual: Value = serde_json::from_slice(&fs::read(retained).unwrap()).unwrap();
        assert_eq!(actual["project_id"], "example/project");
        assert!(project.join(WIKI_SOURCE).is_file());
    }

    fn write_existing_wiki(project: &Path, relative: &str, space_ref: &str) {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_json_replace(
            &path,
            &json!({"objects":[{
                "profile":WIKI_PROFILE,"object":"space","ref":space_ref,"revision":1,
                "provenance":[],"parent_space_refs":[],"child_space_refs":[],"node_refs":[]
            }]}),
        )
        .unwrap();
    }

    #[test]
    fn inspection_distinguishes_create_adopt_and_ambiguity_with_relative_sources() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        assert_eq!(
            inspect_projectcentral(&project).unwrap().outcome,
            ProjectCentralOutcome::CreateProjectCentral
        );

        write_existing_wiki(&project, "docs/wiki.json", "example:space:one");
        let one = inspect_projectcentral(&project).unwrap();
        assert_eq!(one.outcome, ProjectCentralOutcome::BindExistingWikiInPlace);
        assert_eq!(one.wiki_candidates[0].source, "docs/wiki.json");
        preview_adopt(&project, &one.wiki_candidates[0].source).unwrap();

        write_existing_wiki(&project, "Wiki/other.json", "example:space:two");
        assert_eq!(
            inspect_projectcentral(&project).unwrap().outcome,
            ProjectCentralOutcome::UnresolvedHumanDecisionRequired
        );
    }

    #[test]
    fn init_creates_the_fractal_without_imposing_a_human_document() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("README.md"), "native source").unwrap();
        let result = initialize_projectcentral(&central, &project, "example/project").unwrap();

        assert_eq!(
            fs::read_to_string(project.join("README.md")).unwrap(),
            "native source"
        );
        assert!(project.join(HUMAN_SOURCE_DIR).is_dir());
        assert!(project.join(AGENT_GOVERNANCE_DIR).is_dir());
        assert!(project.join(WIKI_SOURCE).is_file());
        assert!(!project.join("ProjectCentral/README.md").exists());
        assert!(
            root_contains_child(&central.join(ROOT_WIKI_SOURCE), &result.wiki_space_ref).unwrap()
        );
        assert!(doctor_projectcentral(&central, &project).unwrap().valid);
    }

    #[test]
    fn adoption_keeps_existing_wiki_in_place_but_still_creates_canonical_agent_wiki() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let adopted = central.join("Work/adopted");
        fs::create_dir_all(&adopted).unwrap();
        write_existing_wiki(&adopted, "docs/wiki.json", "example:space:adopted");

        let adoption =
            adopt_in_place(&central, &adopted, "example/adopted", "docs/wiki.json").unwrap();
        assert_eq!(adoption.wiki_source, WIKI_SOURCE);
        assert_eq!(adoption.adopted_sources, vec!["docs/wiki.json"]);
        assert!(adopted.join("docs/wiki.json").is_file());
        assert!(adopted.join(WIKI_SOURCE).is_file());
        assert!(doctor_projectcentral(&central, &adopted).unwrap().valid);
    }

    #[test]
    fn migration_moves_a_copy_into_agent_wiki_and_preserves_source() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let migrated = central.join("Work/migrated");
        fs::create_dir_all(&migrated).unwrap();
        write_existing_wiki(&migrated, "legacy/wiki.json", "example:space:migrated");

        migrate_selected(&central, &migrated, "example/migrated", "legacy/wiki.json").unwrap();
        assert!(migrated.join("legacy/wiki.json").is_file());
        assert!(migrated.join(WIKI_SOURCE).is_file());
        assert!(migrated.join(HUMAN_SOURCE_DIR).is_dir());
        assert!(doctor_projectcentral(&central, &migrated).unwrap().valid);
    }
}
