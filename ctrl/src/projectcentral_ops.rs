use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{
    projectcentral_paths, read_project_manifest, ProjectCentralManifest, WikiBinding,
    PROJECTCENTRAL_DIR, PROJECT_MANIFEST, WIKI_PROFILE, WIKI_SOURCE,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
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
    pub wiki_source: String,
    pub wiki_space_ref: String,
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
        return match compatible_wiki(&paths.wiki_source)? {
            Some(space_ref) => Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::AlreadyConformant,
                wiki_candidates: vec![WikiCandidate {
                    source: manifest.wiki.source.clone(),
                    space_ref,
                }],
                manifest: Some(manifest),
                manifest_errors: vec![],
                source_signals,
                reason: "ProjectCentral manifest and bound OKF Wiki source are readable.".into(),
            }),
            None => Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                manifest: Some(manifest),
                manifest_errors: vec![],
                wiki_candidates: vec![],
                source_signals,
                reason: "ProjectCentral manifest is valid but its bound Wiki source is missing or incompatible.".into(),
            }),
        };
    }

    let wiki_candidates = discover_wiki_candidates(project_root)?;
    let (outcome, reason) = match wiki_candidates.len() {
        0 => (
            ProjectCentralOutcome::CreateProjectCentral,
            "No compatible OKF Wiki source was found; a ProjectCentral can be created around the native project.",
        ),
        1 => (
            ProjectCentralOutcome::BindExistingWikiInPlace,
            "One compatible OKF Wiki source was found and can be adopted in place without moving it.",
        ),
        _ => (
            ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
            "Multiple compatible Wiki sources were found; Central will not guess which one is authoritative.",
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
            name: "human_aperture".into(),
            valid: paths.human_aperture.is_file(),
            detail: paths.human_aperture.display().to_string(),
        });
        let space_ref = compatible_wiki(&paths.wiki_source)?;
        checks.push(DoctorCheck {
            name: "wiki_source".into(),
            valid: space_ref.is_some(),
            detail: paths.wiki_source.display().to_string(),
        });
        if let Some(space_ref) = space_ref {
            let root_source = central_root.join("Wiki/wiki.json");
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
    ensure_project_directory(project_root)?;
    ensure_unbound(project_root)?;
    let manifest = ProjectCentralManifest::new(project_id);
    validate_manifest(&manifest)?;
    let paths = projectcentral_paths(project_root, &manifest);
    fs::create_dir_all(paths.wiki_source.parent().expect("Wiki source parent"))?;
    write_json_new(&paths.manifest, &serde_json::to_value(&manifest).expect("manifest serializes"))?;
    ensure_human_aperture(&paths.human_aperture, project_id)?;

    let space_ref = project_space_ref(project_id);
    write_json_new(&paths.wiki_source, &project_wiki_value(&space_ref, project_id))?;
    ensure_root_federation(central_root, Some(&space_ref))?;
    let provenance = append_provenance(project_root, "initialize", None, Some(&manifest.wiki.source))?;
    Ok(ProjectCentralMutation {
        outcome: ProjectCentralOutcome::CreateProjectCentral,
        project_root: project_root.to_path_buf(),
        project_id: project_id.into(),
        wiki_source: manifest.wiki.source,
        wiki_space_ref: space_ref,
        root_wiki: central_root.join("Wiki/wiki.json"),
        provenance,
    })
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
            "create ProjectCentral binding metadata if absent".into(),
            "create human aperture if absent".into(),
            format!("bind Wiki source in place: {source}"),
            "federate the bound WikiSpace from the Central root".into(),
        ],
        source: Some(source.into()),
        target: None,
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
    let space_ref = compatible_wiki(&project_root.join(source))?
        .expect("preview established Wiki compatibility");
    let mut manifest = ProjectCentralManifest::new(project_id);
    manifest.wiki = WikiBinding {
        profile: WIKI_PROFILE.into(),
        source: source.into(),
    };
    validate_manifest(&manifest)?;
    let paths = projectcentral_paths(project_root, &manifest);
    fs::create_dir_all(&paths.projectcentral_root)?;
    write_json_new(&paths.manifest, &serde_json::to_value(&manifest).expect("manifest serializes"))?;
    ensure_human_aperture(&paths.human_aperture, project_id)?;
    ensure_root_federation(central_root, Some(&space_ref))?;
    let provenance = append_provenance(project_root, "adopt_in_place", Some(source), Some(source))?;
    Ok(ProjectCentralMutation {
        outcome: ProjectCentralOutcome::BindExistingWikiInPlace,
        project_root: project_root.to_path_buf(),
        project_id: project_id.into(),
        wiki_source: source.into(),
        wiki_space_ref: space_ref,
        root_wiki: central_root.join("Wiki/wiki.json"),
        provenance,
    })
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
            format!("copy selected Wiki source: {source} -> {WIKI_SOURCE}"),
            "preserve the original source in place".into(),
            "write ProjectCentral binding metadata to the copied Wiki".into(),
            "create human aperture if absent".into(),
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
    let target_path = project_root.join(WIKI_SOURCE);
    fs::create_dir_all(target_path.parent().expect("Wiki target parent"))?;
    fs::copy(project_root.join(source), &target_path)?;
    let space_ref = compatible_wiki(&target_path)?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "copied Wiki failed compatibility verification")
    })?;
    let manifest = ProjectCentralManifest::new(project_id);
    let paths = projectcentral_paths(project_root, &manifest);
    write_json_new(&paths.manifest, &serde_json::to_value(&manifest).expect("manifest serializes"))?;
    ensure_human_aperture(&paths.human_aperture, project_id)?;
    ensure_root_federation(central_root, Some(&space_ref))?;
    let provenance = append_provenance(project_root, "migrate_copy", Some(source), Some(WIKI_SOURCE))?;
    Ok(ProjectCentralMutation {
        outcome: ProjectCentralOutcome::MigrateSelectedMaterial,
        project_root: project_root.to_path_buf(),
        project_id: project_id.into(),
        wiki_source: WIKI_SOURCE.into(),
        wiki_space_ref: space_ref,
        root_wiki: central_root.join("Wiki/wiki.json"),
        provenance,
    })
}

pub fn ensure_root_federation(central_root: &Path, child_ref: Option<&str>) -> io::Result<PathBuf> {
    let path = central_root.join("Wiki/wiki.json");
    fs::create_dir_all(path.parent().expect("root Wiki parent"))?;
    let mut value = if path.exists() {
        serde_json::from_slice::<Value>(&fs::read(&path)?).map_err(|error| {
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
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "root Wiki requires an objects list"))?;
    let root_index = objects.iter().position(|object| {
        object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
            && object.get("object").and_then(Value::as_str) == Some("space")
            && object.get("ref").and_then(Value::as_str) == Some(ROOT_WIKI_REF)
    });
    let root_index = match root_index {
        Some(index) => index,
        None if objects.is_empty() => {
            objects.push(root_space_value());
            0
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} does not contain the canonical Central root WikiSpace", path.display()),
            ));
        }
    };

    if let Some(child_ref) = child_ref {
        let object = objects[root_index]
            .as_object_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "root WikiSpace must be an object"))?;
        let children = object
            .entry("child_space_refs")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "root child_space_refs must be an array"))?;
        if !children.iter().any(|entry| entry.as_str() == Some(child_ref)) {
            children.push(Value::String(child_ref.into()));
            children.sort_by(|a, b| a.as_str().unwrap_or_default().cmp(b.as_str().unwrap_or_default()));
            let revision = object.get("revision").and_then(Value::as_u64).unwrap_or(1) + 1;
            object.insert("revision".into(), Value::from(revision));
        }
    }
    write_json_replace(&path, &value)?;
    Ok(path)
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
                    .is_some_and(|children| children.iter().any(|entry| entry.as_str() == Some(child_ref)))
        }))
}

fn discover_wiki_candidates(project_root: &Path) -> io::Result<Vec<WikiCandidate>> {
    let mut paths = vec![];
    collect_json(project_root, 0, &mut paths)?;
    let mut candidates = vec![];
    for path in paths {
        if let Some(space_ref) = compatible_wiki(&path)? {
            let relative = path.strip_prefix(project_root).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "discovered Wiki escaped Project root")
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
                ".git" | ".central" | "node_modules" | "target" | "dist" | "build" | PROJECTCENTRAL_DIR
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

fn compatible_wiki(path: &Path) -> io::Result<Option<String>> {
    if !path.is_file() || fs::metadata(path)?.len() > MAX_WIKI_BYTES {
        return Ok(None);
    }
    let value: Value = match serde_json::from_slice(&fs::read(path)?) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(objects) = value.get("objects").and_then(Value::as_array) else {
        return Ok(None);
    };
    Ok(objects.iter().find_map(|object| {
        if object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
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
    }))
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
        Err(io::Error::new(io::ErrorKind::InvalidInput, validation.errors.join("; ")))
    }
}

fn project_space_ref(project_id: &str) -> String {
    format!("central:wiki:project:{project_id}")
}

fn project_wiki_value(space_ref: &str, title: &str) -> Value {
    json!({"objects":[{
        "profile":WIKI_PROFILE,
        "object":"space",
        "ref":space_ref,
        "revision":1,
        "provenance":[],
        "title":title,
        "parent_space_refs":[ROOT_WIKI_REF],
        "child_space_refs":[],
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

fn ensure_human_aperture(path: &Path, project_id: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().expect("human aperture parent"))?;
    fs::write(
        path,
        format!(
            "# {project_id}\n\nThis is the human-authored Project aperture. Keep the Project's purpose, intended experience, important judgements, and links to canonical native source here. Agent-maintained knowledge belongs in the bound Project Wiki, not in this file unless a human accepts a proposed source revision.\n"
        ),
    )
}

fn ensure_project_directory(project_root: &Path) -> io::Result<()> {
    if project_root.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Project root does not exist as a directory: {}", project_root.display()),
        ))
    }
}

fn ensure_unbound(project_root: &Path) -> io::Result<()> {
    ensure_project_directory(project_root)?;
    if project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST).exists() {
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

fn write_json_new(path: &Path, value: &Value) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("refusing to overwrite existing file: {}", path.display()),
        ));
    }
    write_json_replace(path, value)
}

fn write_json_replace(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn append_provenance(
    project_root: &Path,
    action: &str,
    source: Option<&str>,
    target: Option<&str>,
) -> io::Result<PathBuf> {
    let path = project_root.join(PROJECT_PROVENANCE);
    let mut value = if path.exists() {
        serde_json::from_slice::<Value>(&fs::read(&path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    } else {
        json!({"schema":"central.project.provenance/v1","entries":[]})
    };
    let entries = value
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ProjectCentral provenance entries must be an array"))?;
    entries.push(json!({
        "action":action,
        "source":source,
        "target":target,
        "recorded_at_unix_seconds":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "source_preserved":source.is_some()
    }));
    write_json_replace(&path, &value)?;
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
        output: ActionOutputDefinition { output_type: output_type.into() },
        mutation_class,
        preview_supported,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
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
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
    })?;
    let root = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?.path;
    let project_root = root.join("Work").join(project);
    ensure_project_directory(&project_root).map_err(|error| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
    })?;
    Ok((root, project_root))
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists => {
            ResultStatus::InvalidInput
        }
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn inspect_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.inspect";
    let (_, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    inspect_projectcentral(&project_root)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("inspection serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn doctor_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.doctor";
    let (root, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match doctor_projectcentral(&root, &project_root) {
        Ok(report) if report.valid => {
            ActionResult::success(action, serde_json::to_value(report).expect("doctor serializes"))
        }
        Ok(report) => ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            "ProjectCentral verification failed.",
            Some(serde_json::to_value(report).expect("doctor serializes")),
        ),
        Err(error) => io_failure(action, error),
    }
}

fn init_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.init";
    let (root, project_root) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let project_id = match required(input, "project_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    initialize_projectcentral(&root, &project_root, &project_id)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("mutation serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn adopt_preview_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
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
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("plan serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn adopt_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
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
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("mutation serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn migrate_preview_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
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
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("plan serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn migrate_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
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
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("mutation serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_projectcentral_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor("projectcentral.inspect", "Inspect ProjectCentral", "Inspect a Work project for ProjectCentral, compatible OKF Wiki sources, and adoption ambiguity without mutation.", MutationClass::ReadOnly, "projectcentral-inspection", &["project"], false),
            inspect_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (descriptor("projectcentral.doctor", "Verify ProjectCentral", "Verify ProjectCentral binding, aperture, Wiki source, and root federation.", MutationClass::ReadOnly, "projectcentral-doctor", &["project"], false), doctor_action),
        (descriptor("projectcentral.init", "Initialize ProjectCentral", "Create ProjectCentral around an existing native Work project without moving native files.", MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id"], true), init_action),
        (descriptor("projectcentral.adopt.preview", "Preview Wiki adoption", "Preview binding one selected compatible Wiki in place.", MutationClass::ReadOnly, "projectcentral-mutation-plan", &["project", "source"], false), adopt_preview_action),
        (descriptor("projectcentral.adopt", "Adopt Wiki in place", "Bind one selected compatible Wiki in place, preserve provenance, and federate its WikiSpace.", MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id", "source"], true), adopt_action),
        (descriptor("projectcentral.migrate.preview", "Preview Wiki migration", "Preview copying one selected compatible Wiki into ProjectCentral while preserving its source.", MutationClass::ReadOnly, "projectcentral-mutation-plan", &["project", "source"], false), migrate_preview_action),
        (descriptor("projectcentral.migrate", "Migrate selected Wiki", "Copy one selected compatible Wiki into ProjectCentral, preserve the original, record provenance, and federate it.", MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id", "source"], true), migrate_action),
    ];
    for (descriptor, handler) in actions {
        registry.register(descriptor, handler).expect("ProjectCentral Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
    fn init_creates_project_and_root_wikispaces_without_copying_project_sources() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("README.md"), "native source").unwrap();
        let result = initialize_projectcentral(&central, &project, "example/project").unwrap();
        assert_eq!(fs::read_to_string(project.join("README.md")).unwrap(), "native source");
        assert!(project.join(WIKI_SOURCE).is_file());
        assert!(root_contains_child(&central.join("Wiki/wiki.json"), &result.wiki_space_ref).unwrap());
        assert!(doctor_projectcentral(&central, &project).unwrap().valid);
    }

    #[test]
    fn adoption_binds_in_place_and_migration_preserves_source() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let adopted = central.join("Work/adopted");
        fs::create_dir_all(&adopted).unwrap();
        write_existing_wiki(&adopted, "docs/wiki.json", "example:space:adopted");
        let adoption = adopt_in_place(&central, &adopted, "example/adopted", "docs/wiki.json").unwrap();
        assert_eq!(adoption.wiki_source, "docs/wiki.json");
        assert!(adopted.join("docs/wiki.json").is_file());
        assert!(!adopted.join(WIKI_SOURCE).exists());

        let migrated = central.join("Work/migrated");
        fs::create_dir_all(&migrated).unwrap();
        write_existing_wiki(&migrated, "legacy/wiki.json", "example:space:migrated");
        migrate_selected(&central, &migrated, "example/migrated", "legacy/wiki.json").unwrap();
        assert!(migrated.join("legacy/wiki.json").is_file());
        assert!(migrated.join(WIKI_SOURCE).is_file());
    }
}
