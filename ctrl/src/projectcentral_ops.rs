use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{
    projectcentral_paths, read_project_manifest, ProjectCentralManifest, WikiBinding,
    HUMAN_APERTURE, PROJECTCENTRAL_DIR, PROJECT_MANIFEST, WIKI_PROFILE, WIKI_SOURCE,
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
    let manifest_path = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
    let source_signals = discover_source_signals(project_root)?;

    if manifest_path.exists() {
        let manifest = match read_project_manifest(project_root) {
            Ok(manifest) => manifest,
            Err(error) => {
                return Ok(ProjectCentralInspection {
                    project_root: project_root.to_path_buf(),
                    outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                    manifest: None,
                    manifest_errors: vec![error.to_string()],
                    wiki_candidates: Vec::new(),
                    source_signals,
                    reason: "ProjectCentral manifest exists but cannot be read; mutation would require a human decision.".to_owned(),
                })
            }
        };
        let validation = manifest.validate();
        if !validation.valid {
            return Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                manifest: Some(manifest),
                manifest_errors: validation.errors,
                wiki_candidates: Vec::new(),
                source_signals,
                reason: "ProjectCentral manifest is present but invalid.".to_owned(),
            });
        }
        let paths = projectcentral_paths(project_root, &manifest);
        match compatible_wiki(&paths.wiki_source)? {
            Some(candidate) => Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::AlreadyConformant,
                manifest: Some(manifest),
                manifest_errors: Vec::new(),
                wiki_candidates: vec![candidate],
                source_signals,
                reason: "ProjectCentral manifest and bound OKF Wiki source are readable.".to_owned(),
            }),
            None => Ok(ProjectCentralInspection {
                project_root: project_root.to_path_buf(),
                outcome: ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                manifest: Some(manifest),
                manifest_errors: Vec::new(),
                wiki_candidates: Vec::new(),
                source_signals,
                reason: "ProjectCentral manifest is valid but its bound Wiki source is missing or incompatible.".to_owned(),
            }),
        }
    } else {
        let wiki_candidates = discover_wiki_candidates(project_root)?;
        let (outcome, reason) = match wiki_candidates.len() {
            0 => (
                ProjectCentralOutcome::CreateProjectCentral,
                "No compatible OKF Wiki source was found; a new ProjectCentral can be created around the native project.".to_owned(),
            ),
            1 => (
                ProjectCentralOutcome::BindExistingWikiInPlace,
                "One compatible OKF Wiki source was found and can be adopted in place without moving it.".to_owned(),
            ),
            _ => (
                ProjectCentralOutcome::UnresolvedHumanDecisionRequired,
                "Multiple compatible Wiki sources were found; Central will not guess which one is authoritative.".to_owned(),
            ),
        };
        Ok(ProjectCentralInspection {
            project_root: project_root.to_path_buf(),
            outcome,
            manifest: None,
            manifest_errors: Vec::new(),
            wiki_candidates,
            source_signals,
            reason,
        })
    }
}

pub fn doctor_projectcentral(central_root: &Path, project_root: &Path) -> io::Result<ProjectCentralDoctor> {
    let mut checks = Vec::new();
    let manifest = match read_project_manifest(project_root) {
        Ok(manifest) => {
            let validation = manifest.validate();
            checks.push(DoctorCheck {
                name: "manifest".to_owned(),
                valid: validation.valid,
                detail: if validation.valid { "central.project/v1 manifest is valid".to_owned() } else { validation.errors.join("; ") },
            });
            Some(manifest)
        }
        Err(error) => {
            checks.push(DoctorCheck { name: "manifest".to_owned(), valid: false, detail: error.to_string() });
            None
        }
    };

    if let Some(manifest) = manifest {
        let paths = projectcentral_paths(project_root, &manifest);
        checks.push(DoctorCheck {
            name: "human_aperture".to_owned(),
            valid: paths.human_aperture.is_file(),
            detail: paths.human_aperture.display().to_string(),
        });
        let candidate = compatible_wiki(&paths.wiki_source)?;
        checks.push(DoctorCheck {
            name: "wiki_source".to_owned(),
            valid: candidate.is_some(),
            detail: paths.wiki_source.display().to_string(),
        });
        if let Some(candidate) = candidate {
            let root_path = central_root.join("Wiki/wiki.json");
            let federated = root_contains_child(&root_path, &candidate.space_ref)?;
            checks.push(DoctorCheck {
                name: "root_federation".to_owned(),
                valid: federated,
                detail: format!("{} -> {}", root_path.display(), candidate.space_ref),
            });
        }
    }

    let valid = checks.iter().all(|check| check.valid);
    Ok(ProjectCentralDoctor { project_root: project_root.to_path_buf(), valid, checks })
}

pub fn initialize_projectcentral(central_root: &Path, project_root: &Path, project_id: &str) -> io::Result<ProjectCentralMutation> {
    ensure_project_directory(project_root)?;
    let manifest_path = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
    if manifest_path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "ProjectCentral already has a manifest; inspect or doctor it instead of overwriting it."));
    }
    let manifest = ProjectCentralManifest::new(project_id);
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, validation.errors.join("; ")));
    }
    let space_ref = project_space_ref(project_id);
    let paths = projectcentral_paths(project_root, &manifest);
    fs::create_dir_all(paths.wiki_source.parent().expect("canonical Wiki source has parent"))?;
    write_json_new(&paths.manifest, &serde_json::to_value(&manifest).expect("manifest serializes"))?;
    ensure_human_aperture(&paths.human_aperture, project_id)?;
    write_json_new(&paths.wiki_source, &project_wiki_value(&space_ref, project_id))?;
    ensure_root_federation(central_root, Some(&space_ref))?;
    let provenance = append_provenance(project_root, "initialize", None, Some(&manifest.wiki.source))?;
    Ok(ProjectCentralMutation {
        outcome: ProjectCentralOutcome::CreateProjectCentral,
        project_root: project_root.to_path_buf(),
        project_id: project_id.to_owned(),
        wiki_source: manifest.wiki.source,
        wiki_space_ref: space_ref,
        root_wiki: central_root.join("Wiki/wiki.json"),
        provenance,
    })
}

pub fn preview_adopt(project_root: &Path, source: &str) -> io::Result<MutationPlan> {
    ensure_project_member(source)?;
    let source_path = project_root.join(source);
    if compatible_wiki(&source_path)?.is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "selected source is not a compatible okf-wiki/v1 object collection"));
    }
    Ok(MutationPlan {
        outcome: ProjectCentralOutcome::BindExistingWikiInPlace,
        project_root: project_root.to_path_buf(),
        operations: vec![
            "create ProjectCentral binding metadata if absent".to_owned(),
            "create human aperture if absent".to_owned(),
            format!("bind Wiki source in place: {source}"),
            "federate the bound WikiSpace from the Central root".to_owned(),
        ],
        source: Some(source.to_owned()),
        target: None,
        preserves_source: true,
    })
}

pub fn adopt_in_place(central_root: &Path, project_root: &Path, project_id: &str, source: &str) -> io::Result<ProjectCentralMutation> {
    preview_adopt(project_root, source)?;
    ensure_unbound(project_root)?;
    let source_path = project_root.join(source);
    let candidate = compatible_wiki(&source_path)?.expect("preview established compatibility");
    let mut manifest = ProjectCentralManifest::new(project_id);
    manifest.wiki = WikiBinding { profile: WIKI_PROFILE.to_owned(), source: source.to_owned() };
    let paths = projectcentral_paths(project_root, &manifest);
    fs::create_dir_all(&paths.projectcentral_root)?;
    write_json_new(&paths.manifest, &serde_json::to_value(&manifest).expect("manifest serializes"))?;
    ensure_human_aperture(&paths.human_aperture, project_id)?;
    ensure_root_federation(central_root, Some(&candidate.space_ref))?;
    let provenance = append_provenance(project_root, "adopt_in_place", Some(source), Some(source))?;
    Ok(ProjectCentralMutation {
        outcome: ProjectCentralOutcome::BindExistingWikiInPlace,
        project_root: project_root.to_path_buf(),
        project_id: project_id.to_owned(),
        wiki_source: source.to_owned(),
        wiki_space_ref: candidate.space_ref,
        root_wiki: central_root.join("Wiki/wiki.json"),
        provenance,
    })
}

pub fn preview_migrate(project_root: &Path, source: &str) -> io::Result<MutationPlan> {
    ensure_project_member(source)?;
    let source_path = project_root.join(source);
    if compatible_wiki(&source_path)?.is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "selected source is not a compatible okf-wiki/v1 object collection"));
    }
    let target = project_root.join(WIKI_SOURCE);
    if target.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("migration target already exists: {}", target.display())));
    }
    Ok(MutationPlan {
        outcome: ProjectCentralOutcome::MigrateSelectedMaterial,
        project_root: project_root.to_path_buf(),
        operations: vec![
            format!("copy selected Wiki source: {source} -> {WIKI_SOURCE}"),
            "preserve the original source in place".to_owned(),
            "write ProjectCentral binding metadata to the copied Wiki".to_owned(),
            "create human aperture if absent".to_owned(),
            "record migration provenance".to_owned(),
            "federate the migrated WikiSpace from the Central root".to_owned(),
        ],
        source: Some(source.to_owned()),
        target: Some(WIKI_SOURCE.to_owned()),
        preserves_source: true,
    })
}

pub fn migrate_selected(central_root: &Path, project_root: &Path, project_id: &str, source: &str) -> io::Result<ProjectCentralMutation> {
    preview_migrate(project_root, source)?;
    ensure_unbound(project_root)?;
    let source_path = project_root.join(source);
    let target_path = project_root.join(WIKI_SOURCE);
    fs::create_dir_all(target_path.parent().expect("canonical Wiki target has parent"))?;
    fs::copy(&source_path, &target_path)?;
    let candidate = compatible_wiki(&target_path)?.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "copied Wiki failed compatibility verification"))?;
    let manifest = ProjectCentralManifest::new(project_id);
    let paths = projectcentral_paths(project_root, &manifest);
    write_json_new(&paths.manifest, &serde_json::to_value(&manifest).expect("manifest serializes"))?;
    ensure_human_aperture(&paths.human_aperture, project_id)?;
    ensure_root_federation(central_root, Some(&candidate.space_ref))?;
    let provenance = append_provenance(project_root, "migrate_copy", Some(source), Some(WIKI_SOURCE))?;
    Ok(ProjectCentralMutation {
        outcome: ProjectCentralOutcome::MigrateSelectedMaterial,
        project_root: project_root.to_path_buf(),
        project_id: project_id.to_owned(),
        wiki_source: WIKI_SOURCE.to_owned(),
        wiki_space_ref: candidate.space_ref,
        root_wiki: central_root.join("Wiki/wiki.json"),
        provenance,
    })
}

pub fn ensure_root_federation(central_root: &Path, child_ref: Option<&str>) -> io::Result<PathBuf> {
    let path = central_root.join("Wiki/wiki.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut value = if path.exists() {
        serde_json::from_slice::<Value>(&fs::read(&path)?).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{} is not valid Wiki JSON: {error}", path.display())))?
    } else {
        json!({"objects": [root_space_value()]})
    };
    let objects = value.get_mut("objects").and_then(Value::as_array_mut).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "root Wiki requires an objects list"))?;
    let root = objects.iter_mut().find(|object| {
        object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
            && object.get("object").and_then(Value::as_str) == Some("space")
            && object.get("ref").and_then(Value::as_str) == Some(ROOT_WIKI_REF)
    });
    let root = match root {
        Some(root) => root,
        None if objects.is_empty() => {
            objects.push(root_space_value());
            objects.last_mut().expect("just pushed root WikiSpace")
        }
        None => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} does not contain the canonical Central root WikiSpace", path.display()))),
    };
    if let Some(child_ref) = child_ref {
        let object = root.as_object_mut().expect("root WikiSpace is object");
        let children = object.entry("child_space_refs").or_insert_with(|| json!([])).as_array_mut().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "root child_space_refs must be an array"))?;
        if !children.iter().any(|value| value.as_str() == Some(child_ref)) {
            children.push(Value::String(child_ref.to_owned()));
            children.sort_by(|a, b| a.as_str().unwrap_or_default().cmp(b.as_str().unwrap_or_default()));
            let revision = object.get("revision").and_then(Value::as_u64).unwrap_or(1) + 1;
            object.insert("revision".to_owned(), Value::from(revision));
        }
    }
    write_json_replace(&path, &value)?;
    Ok(path)
}

fn root_contains_child(root_source: &Path, child_ref: &str) -> io::Result<bool> {
    if !root_source.is_file() {
        return Ok(false);
    }
    let value: Value = serde_json::from_slice(&fs::read(root_source)?).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(value.get("objects").and_then(Value::as_array).into_iter().flatten().any(|object| {
        object.get("ref").and_then(Value::as_str) == Some(ROOT_WIKI_REF)
            && object.get("child_space_refs").and_then(Value::as_array).is_some_and(|children| children.iter().any(|entry| entry.as_str() == Some(child_ref)))
    }))
}

fn discover_wiki_candidates(project_root: &Path) -> io::Result<Vec<WikiCandidate>> {
    let mut paths = Vec::new();
    collect_json(project_root, project_root, 0, &mut paths)?;
    let mut candidates = Vec::new();
    for path in paths {
        if let Some(candidate) = compatible_wiki(&path)? {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|a, b| a.source.cmp(&b.source));
    candidates.dedup_by(|a, b| a.source == b.source);
    Ok(candidates)
}

fn collect_json(project_root: &Path, current: &Path, depth: usize, output: &mut Vec<PathBuf>) -> io::Result<()> {
    if depth > MAX_WIKI_SCAN_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if matches!(name.as_ref(), ".git" | ".central" | "node_modules" | "target" | "dist" | "build" | PROJECTCENTRAL_DIR) {
                continue;
            }
            collect_json(project_root, &path, depth + 1, output)?;
        } else if path.extension().and_then(|value| value.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("json")) {
            if fs::metadata(&path)?.len() <= MAX_WIKI_BYTES && path.starts_with(project_root) {
                output.push(path);
            }
        }
    }
    Ok(())
}

fn compatible_wiki(path: &Path) -> io::Result<Option<WikiCandidate>> {
    if !path.is_file() {
        return Ok(None);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_WIKI_BYTES {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(objects) = value.get("objects").and_then(Value::as_array) else { return Ok(None) };
    let Some(space) = objects.iter().find(|object| {
        object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
            && object.get("object").and_then(Value::as_str) == Some("space")
            && object.get("ref").and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty())
    }) else { return Ok(None) };
    let space_ref = space.get("ref").and_then(Value::as_str).expect("predicate checked ref").to_owned();
    Ok(Some(WikiCandidate { source: path.display().to_string(), space_ref }))
}

fn discover_source_signals(project_root: &Path) -> io::Result<Vec<SourceSignal>> {
    let mut signals = Vec::new();
    for (kind, relative) in [
        ("readme", "README.md"),
        ("docs", "docs"),
        ("design", "design"),
        ("obsidian", ".obsidian"),
        ("wiki", "Wiki"),
        ("wiki", "wiki"),
    ] {
        if project_root.join(relative).exists() {
            signals.push(SourceSignal { kind: kind.to_owned(), path: relative.to_owned() });
        }
    }
    Ok(signals)
}

fn project_space_ref(project_id: &str) -> String {
    format!("central:wiki:project:{project_id}")
}

fn project_wiki_value(space_ref: &str, title: &str) -> Value {
    json!({
        "objects": [{
            "profile": WIKI_PROFILE,
            "object": "space",
            "ref": space_ref,
            "revision": 1,
            "provenance": [],
            "title": title,
            "parent_space_refs": [ROOT_WIKI_REF],
            "child_space_refs": [],
            "node_refs": []
        }]
    })
}

fn root_space_value() -> Value {
    json!({
        "profile": WIKI_PROFILE,
        "object": "space",
        "ref": ROOT_WIKI_REF,
        "revision": 1,
        "provenance": [],
        "title": "Central",
        "parent_space_refs": [],
        "child_space_refs": [],
        "node_refs": []
    })
}

fn ensure_human_aperture(path: &Path, project_id: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("# {project_id}\n\nThis is the human-authored Project aperture. Keep the Project's purpose, intended experience, important judgements, and links to canonical native source here. Agent-maintained knowledge belongs in the bound Project Wiki, not in this file unless a human accepts a proposed source revision.\n"))
}

fn ensure_project_directory(project_root: &Path) -> io::Result<()> {
    if project_root.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Project root does not exist as a directory: {}", project_root.display())))
    }
}

fn ensure_unbound(project_root: &Path) -> io::Result<()> {
    ensure_project_directory(project_root)?;
    let manifest = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
    if manifest.exists() {
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "ProjectCentral is already bound; inspect/doctor it rather than replacing its source implicitly."))
    } else {
        Ok(())
    }
}

fn ensure_project_member(raw: &str) -> io::Result<()> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "source must be a non-empty project-root-relative path"));
    }
    let path = Path::new(raw);
    if path.is_absolute() || !path.components().all(|component| matches!(component, Component::Normal(_))) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "source must remain inside the Project and may not contain parent/root components"));
    }
    Ok(())
}

fn write_json_new(path: &Path, value: &Value) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("refusing to overwrite existing file: {}", path.display())));
    }
    write_json_replace(path, value)
}

fn write_json_replace(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn append_provenance(project_root: &Path, action: &str, source: Option<&str>, target: Option<&str>) -> io::Result<PathBuf> {
    let path = project_root.join(PROJECT_PROVENANCE);
    let mut value = if path.exists() {
        serde_json::from_slice::<Value>(&fs::read(&path)?).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    } else {
        json!({"schema":"central.project.provenance/v1","entries":[]})
    };
    let entries = value.get_mut("entries").and_then(Value::as_array_mut).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ProjectCentral provenance entries must be an array"))?;
    let unix_seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    entries.push(json!({
        "action": action,
        "source": source,
        "target": target,
        "recorded_at_unix_seconds": unix_seconds,
        "source_preserved": source.is_some()
    }));
    write_json_replace(&path, &value)?;
    Ok(path)
}

fn action_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required: true,
        choices: None,
        selection: None,
    }
}

fn action_descriptor(id: &str, title: &str, description: &str, mutation: MutationClass, output: &str, inputs: &[&str], preview_supported: bool) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs: inputs.iter().map(|name| action_input(name)).collect(),
        output: ActionOutputDefinition { output_type: output.to_owned() },
        mutation_class: mutation,
        preview_supported,
        required_ports: Vec::new(),
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn required(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input.get(field).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned).ok_or_else(|| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, format!("{action} requires {field}."), None)
    })
}

fn action_project(action: &str, input: &Value, context: &ActionExecutionContext<'_>) -> Result<(PathBuf, PathBuf), ActionResult> {
    let project = required(input, "project", action)?;
    ensure_project_member(&project).map_err(|error| ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None))?;
    let root = resolve_central_root(context.root_options).map_err(|message| ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None))?.path;
    let project_root = root.join("Work").join(project);
    ensure_project_directory(&project_root).map_err(|error| ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None))?;
    Ok((root, project_root))
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn inspect_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.inspect";
    let (_, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    match inspect_projectcentral(&project_root) {
        Ok(report) => ActionResult::success(action, serde_json::to_value(report).expect("inspection serializes")),
        Err(error) => io_failure(action, error),
    }
}

fn doctor_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.doctor";
    let (root, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    match doctor_projectcentral(&root, &project_root) {
        Ok(report) if report.valid => ActionResult::success(action, serde_json::to_value(report).expect("doctor serializes")),
        Ok(report) => ActionResult::failure(Some(action), ResultStatus::VerificationFailure, "ProjectCentral verification failed.", Some(serde_json::to_value(report).expect("doctor serializes"))),
        Err(error) => io_failure(action, error),
    }
}

fn init_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.init";
    let (root, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    let project_id = match required(input, "project_id", action) { Ok(value) => value, Err(result) => return result };
    match initialize_projectcentral(&root, &project_root, &project_id) {
        Ok(result) => ActionResult::success(action, serde_json::to_value(result).expect("mutation serializes")),
        Err(error) => io_failure(action, error),
    }
}

fn adopt_preview_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.adopt.preview";
    let (_, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    let source = match required(input, "source", action) { Ok(value) => value, Err(result) => return result };
    match preview_adopt(&project_root, &source) {
        Ok(plan) => ActionResult::success(action, serde_json::to_value(plan).expect("plan serializes")),
        Err(error) => io_failure(action, error),
    }
}

fn adopt_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.adopt";
    let (root, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    let project_id = match required(input, "project_id", action) { Ok(value) => value, Err(result) => return result };
    let source = match required(input, "source", action) { Ok(value) => value, Err(result) => return result };
    match adopt_in_place(&root, &project_root, &project_id, &source) {
        Ok(result) => ActionResult::success(action, serde_json::to_value(result).expect("mutation serializes")),
        Err(error) => io_failure(action, error),
    }
}

fn migrate_preview_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.migrate.preview";
    let (_, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    let source = match required(input, "source", action) { Ok(value) => value, Err(result) => return result };
    match preview_migrate(&project_root, &source) {
        Ok(plan) => ActionResult::success(action, serde_json::to_value(plan).expect("plan serializes")),
        Err(error) => io_failure(action, error),
    }
}

fn migrate_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.migrate";
    let (root, project_root) = match action_project(action, input, context) { Ok(value) => value, Err(result) => return result };
    let project_id = match required(input, "project_id", action) { Ok(value) => value, Err(result) => return result };
    let source = match required(input, "source", action) { Ok(value) => value, Err(result) => return result };
    match migrate_selected(&root, &project_root, &project_id, &source) {
        Ok(result) => ActionResult::success(action, serde_json::to_value(result).expect("mutation serializes")),
        Err(error) => io_failure(action, error),
    }
}

pub fn register_projectcentral_actions(registry: &mut ActionRegistry) {
    registry.register(action_descriptor(
        "projectcentral.inspect", "Inspect ProjectCentral", "Inspect one existing Work project for ProjectCentral, compatible OKF Wiki sources, and migration/adoption ambiguity without mutation.",
        MutationClass::ReadOnly, "projectcentral-inspection", &["project"], false,
    ), inspect_action).expect("ProjectCentral Action ids are valid");
    registry.register(action_descriptor(
        "projectcentral.doctor", "Verify ProjectCentral", "Verify the manifest, human aperture, bound Wiki source, and root federation for one ProjectCentral.",
        MutationClass::ReadOnly, "projectcentral-doctor", &["project"], false,
    ), doctor_action).expect("ProjectCentral Action ids are valid");
    registry.register(action_descriptor(
        "projectcentral.init", "Initialize ProjectCentral", "Create ProjectCentral around an existing native Work project without moving its ordinary files, then federate its WikiSpace from the Central root.",
        MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id"], true,
    ), init_action).expect("ProjectCentral Action ids are valid");
    registry.register(action_descriptor(
        "projectcentral.adopt.preview", "Preview Wiki adoption", "Preview binding one explicitly selected compatible Wiki source in place. No source is moved or rewritten.",
        MutationClass::ReadOnly, "projectcentral-mutation-plan", &["project", "source"], false,
    ), adopt_preview_action).expect("ProjectCentral Action ids are valid");
    registry.register(action_descriptor(
        "projectcentral.adopt", "Adopt Wiki in place", "Bind one explicitly selected compatible Wiki source to ProjectCentral without moving it, preserving provenance and federating its WikiSpace.",
        MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id", "source"], true,
    ), adopt_action).expect("ProjectCentral Action ids are valid");
    registry.register(action_descriptor(
        "projectcentral.migrate.preview", "Preview Wiki migration", "Preview copying one explicitly selected compatible Wiki source into the canonical ProjectCentral Wiki location while preserving the original.",
        MutationClass::ReadOnly, "projectcentral-mutation-plan", &["project", "source"], false,
    ), migrate_preview_action).expect("ProjectCentral Action ids are valid");
    registry.register(action_descriptor(
        "projectcentral.migrate", "Migrate selected Wiki", "Copy one explicitly selected compatible Wiki source into ProjectCentral, preserve the original source, record provenance, and federate the resulting WikiSpace.",
        MutationClass::LocallyMutating, "projectcentral-mutation", &["project", "project_id", "source"], true,
    ), migrate_action).expect("ProjectCentral Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_existing_wiki(project: &Path, relative: &str, space_ref: &str) {
        let path = project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_json_replace(&path, &json!({"objects":[{
            "profile":WIKI_PROFILE,"object":"space","ref":space_ref,"revision":1,
            "provenance":[],"parent_space_refs":[],"child_space_refs":[],"node_refs":[]
        }]})).unwrap();
    }

    #[test]
    fn inspection_distinguishes_create_adopt_and_ambiguity() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let empty = inspect_projectcentral(&project).unwrap();
        assert_eq!(empty.outcome, ProjectCentralOutcome::CreateProjectCentral);

        write_existing_wiki(&project, "docs/wiki.json", "example:space:one");
        let one = inspect_projectcentral(&project).unwrap();
        assert_eq!(one.outcome, ProjectCentralOutcome::BindExistingWikiInPlace);

        write_existing_wiki(&project, "Wiki/other.json", "example:space:two");
        let many = inspect_projectcentral(&project).unwrap();
        assert_eq!(many.outcome, ProjectCentralOutcome::UnresolvedHumanDecisionRequired);
    }

    #[test]
    fn init_creates_project_and_root_wikispaces_without_copying_project_sources() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("README.md"), "native source").unwrap();
        let result = initialize_projectcentral(&central, &project, "example/project").unwrap();
        assert_eq!(result.outcome, ProjectCentralOutcome::CreateProjectCentral);
        assert_eq!(fs::read_to_string(project.join("README.md")).unwrap(), "native source");
        assert!(project.join(WIKI_SOURCE).is_file());
        assert!(central.join("Wiki/wiki.json").is_file());
        assert!(root_contains_child(&central.join("Wiki/wiki.json"), &result.wiki_space_ref).unwrap());
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
        let migration = migrate_selected(&central, &migrated, "example/migrated", "legacy/wiki.json").unwrap();
        assert_eq!(migration.outcome, ProjectCentralOutcome::MigrateSelectedMaterial);
        assert!(migrated.join("legacy/wiki.json").is_file());
        assert!(migrated.join(WIKI_SOURCE).is_file());
    }
}
