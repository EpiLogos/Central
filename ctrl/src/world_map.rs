//! The world map: one readable index of the user's full world, centred in the
//! Central install.
//!
//! `central.world` walks the ground and reports what is there as data — source
//! counts per Control area, the root and Project WikiSpaces with their child
//! refs, declared source-relations overrides, and per-Project ProjectCentral
//! state. It introduces no ontology of its own: it reuses the ProjectCentral
//! fractal paths, the `okf-wiki/v1` space shape, the ground-relations
//! schema/vocabulary and the mixed-root diagnostic verbatim.
//!
//! Every fault it reports is data about the world (a mixed root, a dangling
//! child ref, a partial ProjectCentral, unresolved provenance), never a
//! repair, never a prompt, never a side effect. The walk is read-only and
//! reads no source content beyond small Wiki and relations JSON objects.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionOutputDefinition,
    ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::projectcentral::{
    read_project_manifest, AGENT_GOVERNANCE_DIR, HUMAN_SOURCE_DIR, PROJECTCENTRAL_DIR,
    PROJECT_MANIFEST, ROOT_AGENT_GOVERNANCE_DIR, ROOT_HUMAN_SOURCE_DIR, ROOT_WIKI_DIR,
    ROOT_WIKI_SOURCE, WIKI_DIR, WIKI_PROFILE, WIKI_SOURCE,
};
use crate::projectcentral_ops::{
    doctor_projectcentral, inspect_projectcentral, DoctorCheck, ProjectCentralOutcome,
    WikiCandidate,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::{inspect_central, resolve_central_root, MixedRootDiagnostic};
use crate::source_horizon::{
    control_source_bindings, GROUND_RELATIONS_SOURCE, CONTROL_GROUND_RELATIONS_SCHEMA,
    CONTROL_GROUND_RELATIONS_SOURCE, CONTROL_WORLD_REF,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

pub const WORLD_MAP_SCHEMA: &str = "central.world-map/v1";
pub const ROOT_AGENT_EXPRESSIONS_DIR: &str = "Control/agents/expressions";
pub const ROOT_MACHINES_DIR: &str = "Control/machines";
/// Wiki and relations objects are small; anything larger is not read.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCAN_DEPTH: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundState {
    Healthy,
    MixedRoot,
    /// The root exists but is missing required Control directories.
    Invalid,
    Missing,
    NotDirectory,
}

impl GroundState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::MixedRoot => "mixed_root",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
            Self::NotDirectory => "not_directory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCentralState {
    /// No ProjectCentral directory: an ordinary native Project.
    Absent,
    /// ProjectCentral material is present but the canonical fractal is incomplete.
    Partial,
    /// The human-source / Agent-Wiki fractal is present and verified.
    Healthy,
    /// ProjectCentral metadata exists but Central will not guess the human decision.
    Unresolved,
}

impl ProjectCentralState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Partial => "partial",
            Self::Healthy => "healthy",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceArea {
    pub path: String,
    pub exists: bool,
    pub sources: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiSpaceState {
    pub path: String,
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    pub child_space_refs: Vec<String>,
    pub dangling_child_space_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiArea {
    pub path: String,
    pub exists: bool,
    pub sources: usize,
    pub wiki: WikiSpaceState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationsState {
    pub path: String,
    pub present: bool,
    pub declared_overrides: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ControlMap {
    pub path: String,
    pub user: SourceArea,
    pub agent_governance: SourceArea,
    pub agent_wiki: WikiArea,
    pub agent_expressions: SourceArea,
    pub machines: SourceArea,
    pub relations: RelationsState,
    /// Participating Control sources after declared relations override the tree fallback.
    pub source_bindings: usize,
    /// Participating sources still stamped `unresolved` provenance.
    pub unresolved_provenance_sources: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectCentralMap {
    pub state: ProjectCentralState,
    pub path: String,
    /// Canonical fractal pieces that are missing, or the Doctor checks that failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<ProjectCentralOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_errors: Option<Vec<String>>,
    pub wiki: Option<WikiSpaceState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_candidates: Option<Vec<WikiCandidate>>,
    pub human_source_files: usize,
    pub relations: RelationsState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_checks: Option<Vec<DoctorCheck>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectMap {
    pub name: String,
    pub path: String,
    /// Ordinary Project files: everything except ProjectCentral, dot directories
    /// and build/output directories.
    pub source_files: usize,
    pub projectcentral: ProjectCentralMap,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkMap {
    pub path: String,
    pub exists: bool,
    /// Non-directory entries sitting loose in Work, outside any Project.
    pub loose_files: usize,
    pub projects: Vec<ProjectMap>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorldMap {
    pub schema: String,
    pub root: std::path::PathBuf,
    pub root_state: String,
    pub ground_state: GroundState,
    pub valid: bool,
    pub mixed_root: MixedRootDiagnostic,
    pub control: ControlMap,
    pub work: WorkMap,
}

fn skipped_directory(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist" | "build")
}

/// Counts ordinary files without reading any content. Symlinks never count;
/// unreadable subtrees count whatever is readable, because a map reports the
/// world it can see rather than failing the whole tree.
fn count_files(directory: &Path, depth: usize, excluded: &[&str], count: &mut usize) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.collect::<Result<Vec<_>, _>>().unwrap_or_default();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_dir() {
            if !skipped_directory(&name) && !excluded.contains(&name.as_str()) {
                count_files(&entry.path(), depth + 1, excluded, count);
            }
        } else if file_type.is_file() && name != AGENT_RETRIEVAL_DENY_MARKER {
            *count += 1;
        }
    }
}

fn source_area(central_root: &Path, relative: &str) -> SourceArea {
    let path = central_root.join(relative);
    let exists = path.is_dir();
    let mut sources = 0;
    if exists {
        count_files(&path, 0, &[], &mut sources);
    }
    SourceArea { path: relative.to_owned(), exists, sources }
}

/// Reads one `okf-wiki/v1` space object. Presence, space ref, revision and
/// child refs are data; an unreadable or non-space file is an error field, not
/// a failure of the map.
fn wiki_space(path: &Path, relative: &str) -> WikiSpaceState {
    let mut state = WikiSpaceState {
        path: relative.to_owned(),
        present: path.is_file(),
        space_ref: None,
        revision: None,
        child_space_refs: Vec::new(),
        dangling_child_space_refs: Vec::new(),
        error: None,
    };
    if !state.present {
        return state;
    }
    let value = match fs::read(path) {
        Ok(bytes) if bytes.len() as u64 <= MAX_JSON_BYTES => {
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) => value,
                Err(error) => {
                    state.error = Some(format!("not valid Wiki JSON: {error}"));
                    return state;
                }
            }
        }
        Ok(_) => {
            state.error = Some(format!("Wiki source exceeds {MAX_JSON_BYTES} bytes"));
            return state;
        }
        Err(error) => {
            state.error = Some(error.to_string());
            return state;
        }
    };
    let space = value
        .get("objects")
        .and_then(Value::as_array)
        .and_then(|objects| {
            objects.iter().find(|object| {
                object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
                    && object.get("object").and_then(Value::as_str) == Some("space")
            })
        });
    let Some(space) = space else {
        state.error = Some(format!("no {WIKI_PROFILE} space object"));
        return state;
    };
    state.space_ref = space.get("ref").and_then(Value::as_str).map(str::to_owned);
    state.revision = space.get("revision").and_then(Value::as_u64);
    state.child_space_refs = space
        .get("child_space_refs")
        .and_then(Value::as_array)
        .map(|children| {
            children
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    state
}

/// A child ref dangles when no Wiki space on this ground declares it.
fn mark_dangling(state: &mut WikiSpaceState, known_space_refs: &BTreeSet<String>) {
    state.dangling_child_space_refs = state
        .child_space_refs
        .iter()
        .filter(|child| !known_space_refs.contains(*child))
        .cloned()
        .collect();
}

/// Reads one ground-relations file against the same schema/world contract the
/// source horizon enforces, so declared overrides are counted exactly where
/// they would be applied.
fn relations_state(
    path: &Path,
    relative: &str,
    schema: &str,
    world_id: Option<&str>,
) -> RelationsState {
    let mut state = RelationsState {
        path: relative.to_owned(),
        present: path.is_file(),
        declared_overrides: 0,
        schema: None,
        error: None,
    };
    if !state.present {
        return state;
    }
    let value = match fs::read(path) {
        Ok(bytes) if bytes.len() as u64 <= MAX_JSON_BYTES => {
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) => value,
                Err(error) => {
                    state.error = Some(format!("not valid relations JSON: {error}"));
                    return state;
                }
            }
        }
        Ok(_) => {
            state.error = Some(format!("relations source exceeds {MAX_JSON_BYTES} bytes"));
            return state;
        }
        Err(error) => {
            state.error = Some(error.to_string());
            return state;
        }
    };
    let declared_schema = value.get("schema").and_then(Value::as_str).map(str::to_owned);
    if declared_schema.as_deref() != Some(schema) {
        state.error = Some(format!(
            "relations schema must be {schema}, found {}",
            declared_schema.unwrap_or_else(|| "none".to_owned())
        ));
        return state;
    }
    if let Some(expected) = world_id {
        let declared_id = value.get("project_id").and_then(Value::as_str).map(str::to_owned);
        if declared_id.as_deref() != Some(expected) {
            state.error = Some(format!(
                "relations world id must be {expected}, found {}",
                declared_id.unwrap_or_else(|| "none".to_owned())
            ));
            return state;
        }
    }
    state.schema = declared_schema;
    state.declared_overrides = value
        .get("relations")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    state
}

fn relations_error(error: &io::Error) -> String {
    format!("source relations cannot be applied: {error}")
}

fn map_control(central_root: &Path) -> ControlMap {
    let user = source_area(central_root, ROOT_HUMAN_SOURCE_DIR);
    let agent_governance = source_area(central_root, ROOT_AGENT_GOVERNANCE_DIR);
    let agent_expressions = source_area(central_root, ROOT_AGENT_EXPRESSIONS_DIR);
    let machines = source_area(central_root, ROOT_MACHINES_DIR);
    let wiki = wiki_space(&central_root.join(ROOT_WIKI_SOURCE), ROOT_WIKI_SOURCE);
    let agent_wiki = WikiArea {
        path: ROOT_WIKI_DIR.to_owned(),
        exists: central_root.join(ROOT_WIKI_DIR).is_dir(),
        sources: source_area(central_root, ROOT_WIKI_DIR).sources,
        wiki,
    };
    let relations = relations_state(
        &central_root.join(CONTROL_GROUND_RELATIONS_SOURCE),
        CONTROL_GROUND_RELATIONS_SOURCE,
        CONTROL_GROUND_RELATIONS_SCHEMA,
        Some(CONTROL_WORLD_REF),
    );

    // The Control tree walk is the same one the Source Change Horizon uses: the
    // tree stamps every source `unresolved` and declared relations override it.
    let mut bindings_error = None;
    let (source_bindings, unresolved_provenance_sources) = match control_source_bindings(central_root)
    {
        Ok(bindings) => (
            bindings.len(),
            bindings
                .iter()
                .filter(|binding| binding.provenance == "unresolved")
                .count(),
        ),
        Err(error) => {
            bindings_error = Some(relations_error(&error));
            (0, 0)
        }
    };

    ControlMap {
        path: "Control".to_owned(),
        user,
        agent_governance,
        agent_wiki,
        agent_expressions,
        machines,
        relations,
        source_bindings,
        unresolved_provenance_sources,
        bindings_error,
    }
}

fn canonical_missing(project_root: &Path) -> Vec<String> {
    [
        (
            project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST),
            format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}"),
        ),
        (project_root.join(HUMAN_SOURCE_DIR), HUMAN_SOURCE_DIR.to_owned()),
        (
            project_root.join(AGENT_GOVERNANCE_DIR),
            AGENT_GOVERNANCE_DIR.to_owned(),
        ),
        (project_root.join(WIKI_DIR), WIKI_DIR.to_owned()),
        (project_root.join(WIKI_SOURCE), WIKI_SOURCE.to_owned()),
    ]
    .into_iter()
    .filter(|(path, _)| !path.exists())
    .map(|(_, relative)| relative)
    .collect()
}

fn map_projectcentral(central_root: &Path, project_root: &Path, relative: &str) -> ProjectCentralMap {
    let projectcentral_root = project_root.join(PROJECTCENTRAL_DIR);
    let projectcentral_path = format!("{relative}/{PROJECTCENTRAL_DIR}");
    let relations = relations_state(
        &project_root.join(GROUND_RELATIONS_SOURCE),
        &format!("{relative}/{GROUND_RELATIONS_SOURCE}"),
        crate::source_horizon::GROUND_RELATIONS_SCHEMA,
        None,
    );
    let absent = ProjectCentralMap {
        state: ProjectCentralState::Absent,
        path: projectcentral_path.clone(),
        missing: None,
        outcome: None,
        reason: None,
        manifest_errors: None,
        wiki: None,
        wiki_candidates: None,
        human_source_files: 0,
        relations: RelationsState {
            path: format!("{relative}/{GROUND_RELATIONS_SOURCE}"),
            ..relations.clone()
        },
        failed_checks: None,
        error: None,
    };
    if !projectcentral_root.is_dir() {
        return absent;
    }

    let manifest_path = projectcentral_root.join(PROJECT_MANIFEST);
    let manifest_present = manifest_path.is_file();
    let mut human_source_files = 0;
    count_files(&project_root.join(HUMAN_SOURCE_DIR), 0, &[], &mut human_source_files);

    let inspection = inspect_projectcentral(project_root).ok();
    let doctor = doctor_projectcentral(central_root, project_root).ok();
    let error = if inspection.is_none() && doctor.is_none() {
        Some("ProjectCentral inspect and doctor could not complete.".to_owned())
    } else {
        None
    };

    let mut wiki = wiki_space(
        &project_root.join(WIKI_SOURCE),
        &format!("{relative}/{WIKI_SOURCE}"),
    );
    // A Wiki retained in place still declares a space on this ground.
    let candidates = inspection.as_ref().map(|value| value.wiki_candidates.clone());

    let failed_checks = doctor.as_ref().map(|report| {
        report
            .checks
            .iter()
            .filter(|check| !check.valid)
            .cloned()
            .collect::<Vec<_>>()
    });
    let (state, missing) = if !manifest_present {
        (ProjectCentralState::Partial, canonical_missing(project_root))
    } else {
        let outcome = inspection.as_ref().map(|value| value.outcome);
        let doctor_valid = doctor.as_ref().map(|report| report.valid).unwrap_or(false);
        match outcome {
            Some(ProjectCentralOutcome::AlreadyConformant) if doctor_valid => {
                (ProjectCentralState::Healthy, Vec::new())
            }
            Some(ProjectCentralOutcome::UnresolvedHumanDecisionRequired) => (
                ProjectCentralState::Unresolved,
                failed_checks
                    .iter()
                    .flatten()
                    .map(|check| check.name.clone())
                    .collect(),
            ),
            _ => (
                ProjectCentralState::Partial,
                failed_checks
                    .iter()
                    .flatten()
                    .map(|check| check.name.clone())
                    .collect(),
            ),
        }
    };
    let _ = &mut wiki;

    ProjectCentralMap {
        state,
        path: projectcentral_path,
        missing: Some(missing).filter(|value| !value.is_empty()),
        outcome: inspection.as_ref().map(|value| value.outcome),
        reason: inspection.as_ref().map(|value| value.reason.clone()),
        manifest_errors: inspection.as_ref().map(|value| value.manifest_errors.clone()),
        wiki: Some(wiki),
        wiki_candidates: candidates,
        human_source_files,
        relations,
        failed_checks: failed_checks.filter(|checks| !checks.is_empty()),
        error,
    }
}

fn map_project(central_root: &Path, work_root: &Path, name: &str) -> io::Result<ProjectMap> {
    let project_root = work_root.join(name);
    let relative = format!("Work/{name}");
    let mut source_files = 0;
    count_files(&project_root, 0, &[PROJECTCENTRAL_DIR], &mut source_files);
    let projectcentral = map_projectcentral(central_root, &project_root, &relative);
    Ok(ProjectMap {
        name: name.to_owned(),
        path: relative,
        source_files,
        projectcentral,
    })
}

fn map_work(central_root: &Path) -> io::Result<WorkMap> {
    let work_root = central_root.join("Work");
    if !work_root.is_dir() {
        return Ok(WorkMap {
            path: "Work".to_owned(),
            exists: false,
            loose_files: 0,
            projects: Vec::new(),
        });
    }
    let mut entries = fs::read_dir(&work_root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut projects = Vec::new();
    let mut loose_files = 0;
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if entry.file_type()?.is_dir() {
            projects.push(map_project(central_root, &work_root, &name)?);
        } else {
            loose_files += 1;
        }
    }
    Ok(WorkMap {
        path: "Work".to_owned(),
        exists: true,
        loose_files,
        projects,
    })
}

pub fn map_world(central_root: &Path) -> io::Result<WorldMap> {
    let health = inspect_central(central_root)?;
    let work = map_work(central_root)?;
    let mut control = map_control(central_root);

    // Every Wiki space declared anywhere on this ground is the address set that
    // child refs resolve against; a ref outside it dangles.
    let mut known_space_refs = BTreeSet::new();
    if let Some(space_ref) = control.agent_wiki.wiki.space_ref.clone() {
        known_space_refs.insert(space_ref);
    }
    for project in &work.projects {
        if let Some(wiki) = project.projectcentral.wiki.as_ref() {
            if let Some(space_ref) = wiki.space_ref.clone() {
                known_space_refs.insert(space_ref);
            }
        }
        for candidate in project.projectcentral.wiki_candidates.iter().flatten() {
            known_space_refs.insert(candidate.space_ref.clone());
        }
    }
    mark_dangling(&mut control.agent_wiki.wiki, &known_space_refs);

    let ground_state = if health.mixed_root.detected {
        GroundState::MixedRoot
    } else if health.valid {
        GroundState::Healthy
    } else {
        match health.root_state.as_str() {
            "missing" => GroundState::Missing,
            "not_directory" => GroundState::NotDirectory,
            _ => GroundState::Invalid,
        }
    };

    Ok(WorldMap {
        schema: WORLD_MAP_SCHEMA.to_owned(),
        root: central_root.to_path_buf(),
        root_state: health.root_state.clone(),
        ground_state,
        valid: health.valid,
        mixed_root: health.mixed_root,
        control,
        work,
    })
}

fn sources_phrase(area: &Value) -> String {
    let path = area.get("path").and_then(Value::as_str).unwrap_or_default();
    let exists = area.get("exists").and_then(Value::as_bool).unwrap_or(false);
    let sources = area.get("sources").and_then(Value::as_u64).unwrap_or(0);
    if exists {
        format!("{path} — {sources} sources")
    } else {
        format!("{path} — missing")
    }
}

fn wiki_phrase(wiki: &Value) -> Option<String> {
    if !wiki.get("present").and_then(Value::as_bool).unwrap_or(false) {
        return Some("no wiki".to_owned());
    }
    if let Some(error) = wiki.get("error").and_then(Value::as_str) {
        return Some(format!("wiki unreadable ({error})"));
    }
    let space_ref = wiki.get("space_ref").and_then(Value::as_str).unwrap_or("?");
    let revision = wiki.get("revision").and_then(Value::as_u64);
    let children = wiki
        .get("child_space_refs")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let dangling = wiki
        .get("dangling_child_space_refs")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let mut phrase = match revision {
        Some(revision) => format!("wiki {space_ref} revision {revision}"),
        None => format!("wiki {space_ref}"),
    };
    if children > 0 {
        phrase.push_str(&format!(
            "; {children} child refs; {dangling} dangling"
        ));
    }
    Some(phrase)
}

/// The human projection of the world map: one readable tree, no jargon.
pub fn explain_world_map(data: &Value) -> String {
    let root = data.get("root").and_then(Value::as_str).unwrap_or_default();
    let ground_state = data
        .get("ground_state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut lines = vec![format!("{root} — Central ground {ground_state}")];

    let control = data.get("control").unwrap_or(&Value::Null);
    lines.push("  Control".to_owned());
    for area in ["user", "agent_governance", "agent_expressions", "machines"] {
        lines.push(format!("    {}", sources_phrase(&control[area])));
    }
    if let Some(wiki) = control.get("agent_wiki") {
        let area = sources_phrase(wiki);
        match wiki.get("wiki").and_then(wiki_phrase) {
            Some(phrase) => lines.push(format!("    {area}; {phrase}")),
            None => lines.push(format!("    {area}")),
        }
    }
    if let Some(relations) = control.get("relations") {
        let overrides = relations
            .get("declared_overrides")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let mut phrase = format!("    relations — {overrides} declared overrides");
        if let Some(error) = relations.get("error").and_then(Value::as_str) {
            phrase.push_str(&format!(" ({error})"));
        }
        lines.push(phrase);
    }
    if let Some(error) = control.get("bindings_error").and_then(Value::as_str) {
        lines.push(format!("    source bindings — {error}"));
    }
    let bindings = control
        .get("source_bindings")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unresolved = control
        .get("unresolved_provenance_sources")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    lines.push(format!(
        "    unresolved provenance — {unresolved} of {bindings} sources"
    ));

    let work = data.get("work").unwrap_or(&Value::Null);
    lines.push("  Work".to_owned());
    if !work.get("exists").and_then(Value::as_bool).unwrap_or(false) {
        lines.push("    Work is missing".to_owned());
        return lines.join("\n");
    }
    let loose = work
        .get("loose_files")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if loose > 0 {
        lines.push(format!("    loose files — {loose}"));
    }
    for project in work
        .get("projects")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let name = project.get("name").and_then(Value::as_str).unwrap_or_default();
        let source_files = project
            .get("source_files")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let mut parts = vec![name.to_owned()];
        let projectcentral = project.get("projectcentral").unwrap_or(&Value::Null);
        let state = projectcentral
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        match state {
            "absent" => parts.push("no ProjectCentral".to_owned()),
            "healthy" => parts.push("ProjectCentral healthy".to_owned()),
            "partial" => {
                let missing = projectcentral
                    .get("missing")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                parts.push(format!("ProjectCentral partial (missing {missing})"));
            }
            other => parts.push(format!("ProjectCentral {other}")),
        }
        if let Some(wiki) = projectcentral.get("wiki").and_then(wiki_phrase) {
            parts.push(wiki);
        }
        let human_sources = projectcentral
            .get("human_source_files")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if state != "absent" {
            parts.push(format!("{human_sources} ProjectCentral human sources"));
        }
        let overrides = projectcentral
            .get("relations")
            .and_then(|relations| relations.get("declared_overrides"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if overrides > 0 {
            parts.push(format!("{overrides} source-relations overrides"));
        }
        parts.push(format!("{source_files} project files"));
        lines.push(format!("    {}", parts.join(" — ")));
    }
    lines.join("\n")
}

fn descriptor() -> ActionDescriptor {
    ActionDescriptor {
        id: "central.world".to_owned(),
        title: "Show the world map".to_owned(),
        description: "Index the full world centred in the active Central root: Control source areas, root and Project WikiSpaces with child refs, declared source-relations overrides, and per-Project ProjectCentral state. Read-only; every fault is reported as data.".to_owned(),
        inputs: vec![],
        output: ActionOutputDefinition { output_type: "central-world-map".to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: true,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn world_action(_: &ActionRegistry, _: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "central.world";
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    match map_world(&root) {
        Ok(map) => ActionResult::success(action, serde_json::to_value(map).expect("world map serializes")),
        Err(error) => ActionResult::failure(Some(action), ResultStatus::InternalFailure, error.to_string(), None),
    }
}

pub fn register_world_map_actions(registry: &mut ActionRegistry) {
    registry.register(descriptor(), world_action).expect("world map Action id is valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral::{AGENT_GOVERNANCE_DIR, PROJECTCENTRAL_DIR};

    #[test]
    fn missing_canonical_pieces_are_named_by_their_ground_relative_path() {
        let temp = crate::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(PROJECTCENTRAL_DIR)).unwrap();
        fs::create_dir_all(project.join(HUMAN_SOURCE_DIR)).unwrap();

        let missing = canonical_missing(&project);

        assert_eq!(
            missing,
            vec![
                format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}"),
                AGENT_GOVERNANCE_DIR.to_owned(),
                WIKI_DIR.to_owned(),
                WIKI_SOURCE.to_owned(),
            ]
        );
    }

    #[test]
    fn skipped_directories_and_the_retrieval_marker_are_not_sources() {
        let temp = crate::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join(".git/config"), "").unwrap();
        fs::write(root.join("node_modules/index.js"), "").unwrap();
        fs::write(root.join("docs/note.md"), "note").unwrap();
        fs::write(root.join(AGENT_RETRIEVAL_DENY_MARKER), "").unwrap();

        let area = source_area(root, ".");

        assert_eq!(area.sources, 1);
    }
}
