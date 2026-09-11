//! The world map: one readable index of the user's full world, centred in the
//! Central install.
//!
//! `central.world` walks the ground and reports what is there as data — source
//! counts per Control area, the root and Project WikiSpaces with their child
//! refs, declared source-relations overrides, and per-Project ProjectCentral
//! state including Flows and the NOW folder. It introduces no ontology of its
//! own: it reuses the ProjectCentral fractal paths, the `okf-wiki/v1` space
//! shape, the ground-relations schema and vocabulary, the Flow registry and
//! revision scheme, and the mixed-root diagnostic verbatim.
//!
//! `central.world.project` is the same world projected at Project scope: one
//! ProjectCentral centred, with its ground, sources and position under Work.
//!
//! `central.world.reproject.plan` and `central.world.reproject.apply` stamp the
//! canonical ProjectCentral scaffolding that is missing — and nothing else.
//! The plan and the receipt both state what they would never do: move, rename,
//! delete or relabel anything, write into an existing file, or author content.
//!
//! Every fault it reports is data about the world (a mixed root, a dangling
//! child ref, a partial ProjectCentral, unresolved provenance, uncommitted
//! Flow edits), never a repair, never a prompt, never a side effect. The
//! mapping actions are read-only and read no source content beyond small Wiki,
//! relations, Flow-registry and policy JSON objects.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::projectcentral::{
    read_project_manifest, ProjectCentralManifest, AGENT_GOVERNANCE_DIR, HUMAN_SOURCE_DIR,
    PROJECTCENTRAL_DIR, PROJECT_MANIFEST, ROOT_WIKI_SOURCE, WIKI_DIR, WIKI_PROFILE, WIKI_SOURCE,
};
use crate::projectcentral_flow::{
    content_revision_bytes, registered_flow_records, relative_member, FLOW_HISTORY_DIR,
    FLOW_REGISTRY,
};
use crate::projectcentral_now::{inspect_now, NOW_DIR};
use crate::projectcentral_ops::{
    doctor_projectcentral, inspect_projectcentral, project_space_ref, project_wiki_value,
    write_json_new, DoctorCheck, ProjectCentralDoctor, ProjectCentralOutcome, WikiCandidate,
    PROJECT_PROVENANCE,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::{inspect_central, resolve_central_root, MixedRootDiagnostic};
use crate::source_horizon::{
    control_source_bindings, project_source_bindings, CONTROL_GROUND_RELATIONS_SCHEMA,
    CONTROL_GROUND_RELATIONS_SOURCE, CONTROL_WORLD_REF, GROUND_RELATIONS_SCHEMA,
    GROUND_RELATIONS_SOURCE,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const WORLD_MAP_SCHEMA: &str = "central.world-map/v1";
/// Control areas that carry the user's world but are not required structure, so
/// `central.doctor` does not vouch for them; the map reports them as data.
pub const ROOT_AGENT_EXPRESSIONS_DIR: &str = "Control/agents/expressions";
pub const ROOT_MACHINES_DIR: &str = "Control/machines";
/// The canonical relations container of the ProjectCentral fractal. The
/// directory is scaffolding; the relations file inside it is authored material
/// and is never stamped.
pub const PROJECT_RELATIONS_DIR: &str = "ProjectCentral/relations";
pub const REPROJECT_PLAN_SCHEMA: &str = "central.world-reproject-plan/v1";
pub const REPROJECT_RECEIPT_SCHEMA: &str = "central.world-reproject-receipt/v1";
/// Wiki and relations objects are small; anything larger is not read.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCAN_DEPTH: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundState {
    Healthy,
    /// The personal root is also the Central product source checkout.
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
    /// Participating sources still carrying `unresolved` provenance.
    pub unresolved_provenance_sources: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings_error: Option<String>,
    /// The bounded-entity identity anchor of this Control (central.pasu/v1).
    pub identity: crate::pasu::PasuIdentityState,
}

/// One registered Flow, read from the Flow registry without reconciling it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowEntry {
    pub flow_ref: String,
    pub source_ref: String,
    pub path: String,
    pub lifecycle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The revision the Flow registry currently records.
    pub revision: String,
    pub revisions_recorded: usize,
    pub source_present: bool,
    /// True when the retained source no longer matches the registered revision:
    /// an external edit Central has not reconciled. None when the source is
    /// missing or unreadable. Computed with the Flow revision scheme itself, so
    /// reading it changes nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncommitted_edits: Option<bool>,
}

/// Flow state for one Project. A Project with no Flow registry reports
/// `present: false` and no Flows: absence is data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowState {
    pub registry: String,
    pub history: String,
    pub present: bool,
    pub flows: Vec<FlowEntry>,
    pub active: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// NOW state for one Project, as `inspect_now` sees it, counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NowState {
    pub path: String,
    pub present: bool,
    pub human_scratch: usize,
    pub active_items: usize,
    pub open_questions: usize,
    pub inactive_items: usize,
    pub invalid_items: usize,
    pub day_records: usize,
    pub promotions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    /// The human-source aperture of the ProjectCentral fractal.
    pub user: SourceArea,
    pub agent_governance: SourceArea,
    pub agent_wiki: WikiArea,
    pub flows: FlowState,
    pub now: NowState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_candidates: Option<Vec<WikiCandidate>>,
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
    pub root: PathBuf,
    pub root_state: String,
    pub ground_state: GroundState,
    pub valid: bool,
    pub mixed_root: MixedRootDiagnostic,
    pub control: ControlMap,
    pub work: WorkMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldProjection {
    /// The whole world, centred in the Central install.
    Root,
    /// The whole world, projected onto one ProjectCentral.
    Project,
}

/// Participating sources of one Project, counted by the provenance the Source
/// Change Horizon stamps them with. Only computable while the manifest is
/// readable; otherwise the reason is the data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectSourceSummary {
    pub bindings: usize,
    pub unresolved_provenance: usize,
    pub by_provenance: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// How one Project sits under Work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectPosition {
    pub work_root: String,
    pub exists: bool,
    pub project_count: usize,
    /// One-based position among the Work projects, ordered by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

/// The world resolved at Project scope: a holographic projection of Central
/// centred on one ProjectCentral.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectWorldMap {
    pub schema: String,
    pub projection: WorldProjection,
    pub root: PathBuf,
    /// The enclosing Central ground the Project hangs from.
    pub ground_state: GroundState,
    pub project: ProjectMap,
    pub sources: ProjectSourceSummary,
    pub position: ProjectPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaffoldKind {
    Directory,
    File,
}

impl ScaffoldKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }
}

/// A canonical piece of the ProjectCentral fractal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScaffoldStep {
    pub path: String,
    pub kind: ScaffoldKind,
}

/// Something the reprojection saw and deliberately did not act on, classified
/// with the same provenance vocabulary the Source Change Horizon stamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceClassified {
    pub path: String,
    pub provenance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treatment: Option<String>,
    pub note: String,
}

/// What reprojection would stamp for one ProjectCentral, and what it would
/// never do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReprojectPlan {
    pub schema: String,
    pub project: String,
    pub project_root: PathBuf,
    pub projectcentral: String,
    /// The manifest's project id when the manifest is readable, otherwise the
    /// name reprojection would derive from the Project directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id_source: Option<String>,
    pub would_stamp: Vec<ScaffoldStep>,
    pub already_present: Vec<ScaffoldStep>,
    pub left_alone: Vec<ProvenanceClassified>,
    pub declared_relations: usize,
    pub sources_by_provenance: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources_error: Option<String>,
    /// Why identity files would not be stamped even though they are missing,
    /// when the Project's identity cannot be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    pub would_not: Vec<String>,
    pub mutation: String,
    /// True when nothing is missing and apply would be a no-op.
    pub noop: bool,
}

/// What reprojection stamped, and what it deliberately left alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReprojectReceipt {
    pub schema: String,
    pub project: String,
    pub projectcentral: String,
    pub stamped: Vec<ScaffoldStep>,
    pub left_alone: Vec<ProvenanceClassified>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    pub would_not: Vec<String>,
    pub mutation: String,
    pub noop: bool,
}

fn skipped_directory(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist" | "build")
}

/// Counts ordinary files without reading any content. Symlinks never count, and
/// an unreadable subtree counts whatever is readable, because a map reports the
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
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
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
    SourceArea {
        path: relative.to_owned(),
        exists,
        sources,
    }
}

fn read_small_json(path: &Path) -> Result<Value, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) if bytes.len() as u64 <= MAX_JSON_BYTES => bytes,
        Ok(_) => return Err(format!("{} exceeds {MAX_JSON_BYTES} bytes", path.display())),
        Err(error) => return Err(error.to_string()),
    };
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))
}

/// Reads one `okf-wiki/v1` space object. Presence, space ref, revision and
/// child refs are data; an unreadable or non-space file is an error field, not
/// a failure of the whole map.
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
    let value = match read_small_json(path) {
        Ok(value) => value,
        Err(error) => {
            state.error = Some(error);
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

/// A child ref dangles when no Wiki space anywhere on this ground declares it.
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
    let value = match read_small_json(path) {
        Ok(value) => value,
        Err(error) => {
            state.error = Some(error);
            return state;
        }
    };
    let declared_schema = value
        .get("schema")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if declared_schema.as_deref() != Some(schema) {
        state.error = Some(format!(
            "relations schema must be {schema}, found {}",
            declared_schema.unwrap_or_else(|| "none".to_owned())
        ));
        return state;
    }
    if let Some(expected) = world_id {
        let declared_id = value
            .get("project_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
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

fn plain_relations(relative: &str) -> RelationsState {
    RelationsState {
        path: relative.to_owned(),
        present: false,
        declared_overrides: 0,
        schema: None,
        error: None,
    }
}

fn absent_area(relative: &str) -> SourceArea {
    SourceArea {
        path: relative.to_owned(),
        exists: false,
        sources: 0,
    }
}

fn absent_wiki_area(relative: &str) -> WikiArea {
    WikiArea {
        path: relative.to_owned(),
        exists: false,
        sources: 0,
        wiki: WikiSpaceState {
            path: format!("{relative}/wiki.json"),
            present: false,
            space_ref: None,
            revision: None,
            child_space_refs: Vec::new(),
            dangling_child_space_refs: Vec::new(),
            error: None,
        },
    }
}

fn absent_flows() -> FlowState {
    FlowState {
        registry: FLOW_REGISTRY.to_owned(),
        history: FLOW_HISTORY_DIR.to_owned(),
        present: false,
        flows: Vec::new(),
        active: 0,
        error: None,
    }
}

fn absent_now(relative: &str) -> NowState {
    NowState {
        path: format!("{relative}/now"),
        present: false,
        human_scratch: 0,
        active_items: 0,
        open_questions: 0,
        inactive_items: 0,
        invalid_items: 0,
        day_records: 0,
        promotions: 0,
        error: None,
    }
}

/// Reads the Flow registry of one Project without reconciling anything: the
/// map discloses revisions and uncommitted external edits, it never absorbs
/// them. `list_flows` would reconcile (and write); the registry reader does not.
fn map_flows(project_root: &Path) -> FlowState {
    let registry = project_root.join(FLOW_REGISTRY);
    let present = registry.is_file();
    let mut state = FlowState {
        registry: FLOW_REGISTRY.to_owned(),
        history: FLOW_HISTORY_DIR.to_owned(),
        present,
        flows: Vec::new(),
        active: 0,
        error: None,
    };
    // A missing registry is the ordinary no-Flows case; only a registry that
    // exists but cannot be read is a fault, and then it is an error field.
    if !present {
        return state;
    }
    let records = match registered_flow_records(project_root) {
        Ok(records) => records,
        Err(error) => {
            state.error = Some(format!("flow registry cannot be read: {error}"));
            return state;
        }
    };
    for record in records {
        let source = project_root.join(&record.path);
        let mut source_present = source.is_file();
        let mut uncommitted_edits = None;
        if source_present {
            match fs::read(&source) {
                Ok(bytes) => {
                    uncommitted_edits =
                        Some(content_revision_bytes(&bytes) != record.current_revision)
                }
                Err(_) => source_present = false,
            }
        }
        if record.lifecycle == "active" {
            state.active += 1;
        }
        state.flows.push(FlowEntry {
            flow_ref: record.flow_ref,
            source_ref: record.source_ref,
            path: record.path,
            lifecycle: record.lifecycle,
            title: record.title,
            revision: record.current_revision,
            revisions_recorded: record.revisions.len(),
            source_present,
            uncommitted_edits,
        });
    }
    state
}

/// Reads the NOW folder of one Project through the NOW inspection, which is a
/// read: no promotion, no rollover, no write.
fn map_now(project_root: &Path, relative: &str) -> NowState {
    let present = project_root.join(NOW_DIR).exists();
    let mut state = NowState {
        path: format!("{relative}/now"),
        present,
        human_scratch: 0,
        active_items: 0,
        open_questions: 0,
        inactive_items: 0,
        invalid_items: 0,
        day_records: 0,
        promotions: 0,
        error: None,
    };
    if !present {
        return state;
    }
    match inspect_now(project_root) {
        Ok(inspection) => {
            state.human_scratch = inspection.human_scratch.len();
            state.active_items = inspection.active_items.len();
            state.open_questions = inspection.open_questions.len();
            state.inactive_items = inspection.inactive_items.len();
            state.invalid_items = inspection.invalid_items.len();
            state.day_records = inspection.day_records.len();
            state.promotions = inspection.promotions.len();
        }
        Err(error) => state.error = Some(format!("NOW folder cannot be inspected: {error}")),
    }
    state
}

/// Counts the sources of one ProjectCentral fractal area.
fn project_area(project_root: &Path, dir: &str, relative: &str) -> SourceArea {
    let mut area = source_area(project_root, dir);
    area.path = relative.to_owned();
    area
}

fn map_control(central_root: &Path) -> ControlMap {
    let user = source_area(central_root, crate::projectcentral::ROOT_HUMAN_SOURCE_DIR);
    let agent_governance = source_area(
        central_root,
        crate::projectcentral::ROOT_AGENT_GOVERNANCE_DIR,
    );
    let agent_expressions = source_area(central_root, ROOT_AGENT_EXPRESSIONS_DIR);
    let machines = source_area(central_root, ROOT_MACHINES_DIR);
    let wiki = wiki_space(&central_root.join(ROOT_WIKI_SOURCE), ROOT_WIKI_SOURCE);
    let wiki_area = source_area(central_root, crate::projectcentral::ROOT_WIKI_DIR);
    let agent_wiki = WikiArea {
        path: crate::projectcentral::ROOT_WIKI_DIR.to_owned(),
        exists: wiki_area.exists,
        sources: wiki_area.sources,
        wiki,
    };
    let relations = relations_state(
        &central_root.join(CONTROL_GROUND_RELATIONS_SOURCE),
        CONTROL_GROUND_RELATIONS_SOURCE,
        CONTROL_GROUND_RELATIONS_SCHEMA,
        Some(CONTROL_WORLD_REF),
    );
    // A malformed subject ref in ground relations already fails the bindings
    // read above; here absence or a read failure simply leaves the declared
    // side of the identity agreement unset.
    let ground_relations_subject_ref =
        crate::source_horizon::control_ground_relations_subject_ref(central_root)
            .ok()
            .flatten();
    let identity = crate::pasu::PasuIdentityState::read(central_root, ground_relations_subject_ref);

    // The Control tree walk is the same one the Source Change Horizon uses: the
    // tree stamps every source `unresolved`, and declared relations override it.
    let mut bindings_error = None;
    let (source_bindings, unresolved_provenance_sources) =
        match control_source_bindings(central_root) {
            Ok(bindings) => (
                bindings.len(),
                bindings
                    .iter()
                    .filter(|binding| binding.provenance == "unresolved")
                    .count(),
            ),
            Err(error) => {
                bindings_error = Some(format!("Control source bindings cannot be read: {error}"));
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
        identity,
    }
}

/// Names the canonical fractal pieces a ProjectCentral is missing, by their
/// ground-relative path.
fn canonical_missing(project_root: &Path) -> Vec<String> {
    [
        (
            project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST),
            format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}"),
        ),
        (
            project_root.join(HUMAN_SOURCE_DIR),
            HUMAN_SOURCE_DIR.to_owned(),
        ),
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

fn failed_check_names(doctor: &Option<ProjectCentralDoctor>) -> Vec<String> {
    doctor
        .as_ref()
        .map(|report| {
            report
                .checks
                .iter()
                .filter(|check| !check.valid)
                .map(|check| check.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn map_projectcentral(
    central_root: &Path,
    project_root: &Path,
    relative: &str,
) -> ProjectCentralMap {
    let projectcentral_path = format!("{relative}/{PROJECTCENTRAL_DIR}");
    if !project_root.join(PROJECTCENTRAL_DIR).is_dir() {
        return ProjectCentralMap {
            state: ProjectCentralState::Absent,
            path: projectcentral_path.clone(),
            missing: None,
            outcome: None,
            reason: None,
            manifest_errors: None,
            user: absent_area(&format!("{projectcentral_path}/user")),
            agent_governance: absent_area(&format!("{projectcentral_path}/agents/governance")),
            agent_wiki: absent_wiki_area(&format!("{projectcentral_path}/agents/wiki")),
            flows: absent_flows(),
            now: absent_now(&projectcentral_path),
            wiki_candidates: None,
            relations: plain_relations(&format!("{relative}/{GROUND_RELATIONS_SOURCE}")),
            failed_checks: None,
            error: None,
        };
    }

    // A Project with ProjectCentral material gets the real ProjectCentral facts:
    // the inspection outcome and the Doctor checks, verbatim.
    let inspection = inspect_projectcentral(project_root).ok();
    let doctor = doctor_projectcentral(central_root, project_root).ok();
    let error = if inspection.is_none() && doctor.is_none() {
        Some("ProjectCentral inspect and doctor could not complete.".to_owned())
    } else {
        None
    };
    let failed_checks = doctor.as_ref().map(|report| {
        report
            .checks
            .iter()
            .filter(|check| !check.valid)
            .cloned()
            .collect::<Vec<_>>()
    });

    let manifest_present = project_root
        .join(PROJECTCENTRAL_DIR)
        .join(PROJECT_MANIFEST)
        .is_file();
    let (state, missing) = if !manifest_present {
        // No manifest to interpret: name what the canonical fractal is missing.
        (
            ProjectCentralState::Partial,
            canonical_missing(project_root),
        )
    } else {
        let outcome = inspection.as_ref().map(|value| value.outcome);
        let doctor_valid = doctor.as_ref().is_some_and(|report| report.valid);
        match outcome {
            Some(ProjectCentralOutcome::AlreadyConformant) if doctor_valid => {
                (ProjectCentralState::Healthy, Vec::new())
            }
            Some(ProjectCentralOutcome::UnresolvedHumanDecisionRequired) => {
                (ProjectCentralState::Unresolved, failed_check_names(&doctor))
            }
            _ => (ProjectCentralState::Partial, failed_check_names(&doctor)),
        }
    };

    let user = project_area(
        project_root,
        HUMAN_SOURCE_DIR,
        &format!("{projectcentral_path}/user"),
    );
    let agent_governance = project_area(
        project_root,
        AGENT_GOVERNANCE_DIR,
        &format!("{projectcentral_path}/agents/governance"),
    );
    let wiki_area = project_area(
        project_root,
        WIKI_DIR,
        &format!("{projectcentral_path}/agents/wiki"),
    );

    ProjectCentralMap {
        state,
        path: projectcentral_path.clone(),
        missing: Some(missing).filter(|values| !values.is_empty()),
        outcome: inspection.as_ref().map(|value| value.outcome),
        reason: inspection.as_ref().map(|value| value.reason.clone()),
        manifest_errors: inspection
            .as_ref()
            .map(|value| value.manifest_errors.clone()),
        user,
        agent_governance,
        agent_wiki: WikiArea {
            path: format!("{projectcentral_path}/agents/wiki"),
            exists: wiki_area.exists,
            sources: wiki_area.sources,
            wiki: wiki_space(
                &project_root.join(WIKI_SOURCE),
                &format!("{relative}/{WIKI_SOURCE}"),
            ),
        },
        flows: map_flows(project_root),
        now: map_now(project_root, &projectcentral_path),
        wiki_candidates: inspection
            .as_ref()
            .map(|value| value.wiki_candidates.clone()),
        relations: relations_state(
            &project_root.join(GROUND_RELATIONS_SOURCE),
            &format!("{relative}/{GROUND_RELATIONS_SOURCE}"),
            GROUND_RELATIONS_SCHEMA,
            None,
        ),
        failed_checks: failed_checks.filter(|checks| !checks.is_empty()),
        error,
    }
}

fn map_project(central_root: &Path, work_root: &Path, name: &str) -> ProjectMap {
    let project_root = work_root.join(name);
    let relative = format!("Work/{name}");
    let mut source_files = 0;
    count_files(&project_root, 0, &[PROJECTCENTRAL_DIR], &mut source_files);
    let projectcentral = map_projectcentral(central_root, &project_root, &relative);
    ProjectMap {
        name: name.to_owned(),
        path: relative,
        source_files,
        projectcentral,
    }
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
            projects.push(map_project(central_root, &work_root, &name));
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
        if let Some(space_ref) = project.projectcentral.agent_wiki.wiki.space_ref.clone() {
            known_space_refs.insert(space_ref);
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

/// Participating sources of one Project, counted by the provenance the Source
/// Change Horizon stamps. A Project without a readable manifest has no Horizon
/// bindings to count, and the reason is reported instead of guessed around.
fn summarize_project_sources(project_root: &Path) -> ProjectSourceSummary {
    let mut summary = ProjectSourceSummary {
        bindings: 0,
        unresolved_provenance: 0,
        by_provenance: BTreeMap::new(),
        error: None,
    };
    match project_source_bindings(project_root) {
        Ok(bindings) => {
            summary.bindings = bindings.len();
            for binding in bindings {
                let provenance = binding.provenance.clone();
                *summary.by_provenance.entry(provenance).or_insert(0) += 1;
                if binding.provenance == "unresolved" {
                    summary.unresolved_provenance += 1;
                }
            }
        }
        Err(error) => {
            summary.error = Some(format!("project source bindings cannot be read: {error}"))
        }
    }
    summary
}

/// The world projected at Project scope, centred on one ProjectCentral.
pub fn map_project_world(central_root: &Path, name: &str) -> io::Result<ProjectWorldMap> {
    if !central_root.join("Work").is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Work does not exist in this Central root",
        ));
    }
    let name = validated_project_name(name)?;
    let (project_root, _) = project_root_of(central_root, &name)?;
    let health = inspect_central(central_root)?;
    let work_root = central_root.join("Work");

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

    let project = map_project(central_root, &work_root, &name);
    let sources = summarize_project_sources(&project_root);

    let mut siblings = Vec::new();
    for entry in fs::read_dir(&work_root)?.collect::<Result<Vec<_>, _>>()? {
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if !entry_name.starts_with('.') && entry.file_type()?.is_dir() {
            siblings.push(entry_name);
        }
    }
    siblings.sort();
    let project_count = siblings.len();
    let index = siblings
        .iter()
        .position(|sibling| sibling == &name)
        .map(|position| position + 1);

    Ok(ProjectWorldMap {
        schema: WORLD_MAP_SCHEMA.to_owned(),
        projection: WorldProjection::Project,
        root: central_root.to_path_buf(),
        ground_state,
        project,
        sources,
        position: ProjectPosition {
            work_root: "Work".to_owned(),
            exists: true,
            project_count,
            index,
        },
    })
}

/// The canonical ProjectCentral fractal, as ground-relative paths: the pieces
/// reprojection stamps when they are missing. The relations file and the
/// provenance receipt are deliberately absent from this list — both are
/// authored or earned, never stamped.
fn canonical_scaffold(project: &str) -> Vec<ScaffoldStep> {
    let directory = |path: String| ScaffoldStep {
        path,
        kind: ScaffoldKind::Directory,
    };
    let file = |path: String| ScaffoldStep {
        path,
        kind: ScaffoldKind::File,
    };
    vec![
        directory(format!("{project}/{PROJECTCENTRAL_DIR}")),
        file(format!("{project}/{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}")),
        directory(format!("{project}/{HUMAN_SOURCE_DIR}")),
        directory(format!("{project}/{AGENT_GOVERNANCE_DIR}")),
        directory(format!("{project}/{WIKI_DIR}")),
        file(format!("{project}/{WIKI_SOURCE}")),
        directory(format!("{project}/{PROJECT_RELATIONS_DIR}")),
    ]
}

/// The would-not contract of reprojection, stated the same way in every plan
/// and receipt.
fn reproject_would_not() -> Vec<String> {
    vec![
        "never move, rename, delete, or relabel anything".to_owned(),
        "never write into an existing file, including an existing manifest or wiki.json".to_owned(),
        "never stamp authored relations: ProjectCentral/relations/source-relations.json is human/agent authored".to_owned(),
        "never fabricate ProjectCentral/provenance.json: a mutation receipt must be earned by a real mutation".to_owned(),
        "never federate the Project WikiSpace into the root WikiSpace: wiki tooling's job".to_owned(),
        "never write Wiki content: reprojection stamps structure only".to_owned(),
    ]
}

/// The provenance the Source Change Horizon stamps a ProjectCentral area with,
/// reused verbatim so a plan classifies sources the way the Horizon does.
fn area_classification(relative: &str) -> (String, Option<String>, Option<String>) {
    if relative.starts_with(&format!("{PROJECTCENTRAL_DIR}/user")) {
        (
            "unresolved".to_owned(),
            Some("unspecified".to_owned()),
            Some("projectcentral-user".to_owned()),
        )
    } else if relative.starts_with(&format!("{PROJECTCENTRAL_DIR}/agents/governance")) {
        (
            "unresolved".to_owned(),
            Some("unspecified".to_owned()),
            Some("projectcentral-agent-governance".to_owned()),
        )
    } else if relative.starts_with(&format!("{PROJECTCENTRAL_DIR}/agents/wiki")) {
        (
            "agent-maintained".to_owned(),
            Some("unspecified".to_owned()),
            Some("projectcentral-agent-wiki".to_owned()),
        )
    } else {
        ("unresolved".to_owned(), None, None)
    }
}

/// Walks one ProjectCentral and classifies every file it actually contains that
/// reprojection will not stamp. Litter, out-of-place files and authored
/// material are named, classified, and left exactly where they are. `canonical`
/// holds ground-relative paths; everything this walk reports is
/// ProjectCentral-relative.
fn classify_left_alone(
    project_root: &Path,
    project: &str,
    canonical: &BTreeSet<String>,
) -> Vec<ProvenanceClassified> {
    let projectcentral = project_root.join(PROJECTCENTRAL_DIR);
    let mut classified = Vec::new();
    let mut pending = vec![(projectcentral, 0usize)];
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_SCAN_DEPTH {
            continue;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let mut entries = entries.collect::<Result<Vec<_>, _>>().unwrap_or_default();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let relative = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if file_type.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            if file_type.is_symlink() || canonical.contains(&format!("{project}/{relative}")) {
                continue;
            }
            let (provenance, standing, treatment) = area_classification(&relative);
            let note = if relative == GROUND_RELATIONS_SOURCE {
                "declared source-relations: authored material that overrides tree provenance; never stamped or rewritten".to_owned()
            } else if relative == PROJECT_PROVENANCE {
                "Central mutation receipt; earned by a real mutation, never fabricated".to_owned()
            } else if treatment.is_some() {
                "canonical-area material; left exactly where it is".to_owned()
            } else {
                "out-of-place inside ProjectCentral; left exactly where it is".to_owned()
            };
            classified.push(ProvenanceClassified {
                path: relative,
                provenance,
                standing,
                treatment,
                note,
            });
        }
    }
    classified
}

/// An existing manifest that cannot be read or does not validate is the one
/// canonical file whose state reprojection must name even though it never
/// touches it: identity is exactly what reprojection refuses to invent, so the
/// fault is classified as data alongside everything else left alone.
fn disclose_blocked_manifest(
    project_root: &Path,
    identity_error: &Option<String>,
    left_alone: &mut Vec<ProvenanceClassified>,
) {
    let Some(error) = identity_error else { return };
    let relative = format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}");
    if !project_root.join(&relative).is_file() {
        return;
    }
    left_alone.retain(|entry| entry.path != relative);
    let (provenance, standing, treatment) = area_classification(&relative);
    left_alone.push(ProvenanceClassified {
        path: relative,
        provenance,
        standing,
        treatment,
        note: format!("{error}; never rewritten"),
    });
}

fn validated_project_name(name: &str) -> io::Result<String> {
    let validated = relative_member(name)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    Ok(validated.to_string_lossy().replace('\\', "/"))
}

/// Resolves one Work Project a world action may address.
fn project_root_of(central_root: &Path, name: &str) -> io::Result<(PathBuf, String)> {
    let name = validated_project_name(name)?;
    let project_root = central_root.join("Work").join(&name);
    if !project_root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Project root does not exist: {}", project_root.display()),
        ));
    }
    Ok((project_root, format!("Work/{name}")))
}

/// The project id reprojection stamps with: the manifest's own id while the
/// manifest is readable, otherwise derived from the Project directory name.
/// A manifest that exists but cannot be read authorises nothing.
fn reprojection_identity(
    project_root: &Path,
    name: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    match read_project_manifest(project_root) {
        Ok(manifest) if manifest.validate().valid => {
            (Some(manifest.project_id), Some("manifest".to_owned()), None)
        }
        Ok(_) => (
            None,
            None,
            Some("ProjectCentral/project.json does not validate; reprojection stamps directories only".to_owned()),
        ),
        Err(error) if project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST).is_file() => (
            None,
            None,
            Some(format!("ProjectCentral/project.json cannot be read: {error}")),
        ),
        Err(_) => {
            let derived = name.trim();
            if derived.is_empty() {
                (None, None, Some("Project directory name is empty".to_owned()))
            } else {
                (
                    Some(derived.to_owned()),
                    Some("derived-from-directory-name".to_owned()),
                    None,
                )
            }
        }
    }
}

/// What canonical scaffolding a ProjectCentral already has, what is missing,
/// and which paths are occupied by something that is not scaffolding.
struct ScaffoldSurvey {
    missing: Vec<ScaffoldStep>,
    present: Vec<ScaffoldStep>,
    occupied: Vec<ProvenanceClassified>,
    canonical: BTreeSet<String>,
}

fn survey_scaffold(central_root: &Path, project: &str) -> ScaffoldSurvey {
    let mut survey = ScaffoldSurvey {
        missing: Vec::new(),
        present: Vec::new(),
        occupied: Vec::new(),
        canonical: BTreeSet::new(),
    };
    for step in canonical_scaffold(project) {
        let path = central_root.join(&step.path);
        let exists = path.exists();
        if exists {
            survey.canonical.insert(step.path.clone());
        }
        match (step.kind, exists) {
            (_, false) => survey.missing.push(step),
            (ScaffoldKind::Directory, true) if path.is_dir() => survey.present.push(step),
            (ScaffoldKind::File, true) if path.is_file() => survey.present.push(step),
            (kind, true) => survey.occupied.push(ProvenanceClassified {
                path: step.path,
                provenance: "unresolved".to_owned(),
                standing: None,
                treatment: None,
                note: format!(
                    "a {} occupies the canonical {} path; left exactly where it is",
                    if kind == ScaffoldKind::Directory {
                        "file"
                    } else {
                        "directory"
                    },
                    kind.as_str()
                ),
            }),
        }
    }
    survey
}

pub fn plan_reproject(central_root: &Path, name: &str) -> io::Result<ReprojectPlan> {
    let (project_root, project) = project_root_of(central_root, name)?;
    let (project_id, project_id_source, identity_error) =
        reprojection_identity(&project_root, name);
    let survey = survey_scaffold(central_root, &project);
    let noop = survey.missing.is_empty();
    let mut left_alone = classify_left_alone(&project_root, &project, &survey.canonical);
    disclose_blocked_manifest(&project_root, &identity_error, &mut left_alone);
    left_alone.extend(survey.occupied);
    // Without a readable identity nothing that carries one is stamped, so the
    // plan does not promise it either.
    let mut would_stamp = survey.missing;
    if identity_error.is_some() {
        would_stamp.retain(|step| step.kind == ScaffoldKind::Directory);
    }
    let sources = summarize_project_sources(&project_root);
    let relations = relations_state(
        &project_root.join(GROUND_RELATIONS_SOURCE),
        &format!("{project}/{GROUND_RELATIONS_SOURCE}"),
        GROUND_RELATIONS_SCHEMA,
        None,
    );

    Ok(ReprojectPlan {
        schema: REPROJECT_PLAN_SCHEMA.to_owned(),
        project: name.trim().to_owned(),
        project_root,
        projectcentral: format!("{project}/{PROJECTCENTRAL_DIR}"),
        project_id,
        project_id_source,
        would_stamp,
        already_present: survey.present,
        left_alone,
        declared_relations: relations.declared_overrides,
        sources_by_provenance: sources.by_provenance,
        sources_error: sources.error,
        blocked: identity_error,
        would_not: reproject_would_not(),
        mutation: "additive-only".to_owned(),
        noop,
    })
}

pub fn apply_reproject(central_root: &Path, name: &str) -> io::Result<ReprojectReceipt> {
    let (project_root, project) = project_root_of(central_root, name)?;
    let (project_id, _project_id_source, identity_error) =
        reprojection_identity(&project_root, name);
    let survey = survey_scaffold(central_root, &project);
    let mut left_alone = classify_left_alone(&project_root, &project, &survey.canonical);
    disclose_blocked_manifest(&project_root, &identity_error, &mut left_alone);
    left_alone.extend(survey.occupied);

    let mut stamped = Vec::new();
    // Stamp in canonical order: every parent directory precedes what it
    // contains, so one pass is enough. Nothing existing is ever written, and
    // `write_json_new` refuses to overwrite as a backstop.
    for step in survey.missing.iter() {
        match step.kind {
            ScaffoldKind::Directory => {
                fs::create_dir_all(central_root.join(&step.path))?;
                stamped.push(step.clone());
            }
            ScaffoldKind::File => {
                // Identity files are stamped only while the Project's identity
                // is known: the manifest from the manifest, the Wiki space from
                // the same id.
                if identity_error.is_some() {
                    continue;
                }
                let project_id = project_id.as_deref().unwrap_or_default();
                let value = if step
                    .path
                    .ends_with(&format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}"))
                {
                    serde_json::to_value(ProjectCentralManifest::new(project_id))
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                } else if step.path.ends_with(WIKI_SOURCE) {
                    project_wiki_value(&project_space_ref(project_id), project_id, &[])
                } else {
                    continue;
                };
                write_json_new(&central_root.join(&step.path), &value)?;
                stamped.push(step.clone());
            }
        }
    }

    let noop = stamped.is_empty();
    Ok(ReprojectReceipt {
        schema: REPROJECT_RECEIPT_SCHEMA.to_owned(),
        project: name.trim().to_owned(),
        projectcentral: format!("{project}/{PROJECTCENTRAL_DIR}"),
        stamped,
        left_alone,
        blocked: identity_error,
        would_not: reproject_would_not(),
        mutation: "additive-only".to_owned(),
        noop,
    })
}

fn count_phrase(area: &Value) -> String {
    let path = area.get("path").and_then(Value::as_str).unwrap_or_default();
    if !area.get("exists").and_then(Value::as_bool).unwrap_or(false) {
        return format!("{path} — missing");
    }
    let sources = area.get("sources").and_then(Value::as_u64).unwrap_or(0);
    let noun = if sources == 1 { "source" } else { "sources" };
    format!("{path} — {sources} {noun}")
}

fn wiki_phrase(wiki: &Value) -> Option<String> {
    if !wiki
        .get("present")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
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
        let refs = if children == 1 {
            "child ref"
        } else {
            "child refs"
        };
        phrase.push_str(&format!("; {children} {refs}; {dangling} dangling"));
    }
    Some(phrase)
}

fn flow_phrase(flows: &Value) -> Option<String> {
    let count = flows
        .get("flows")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let active = flows.get("active").and_then(Value::as_u64).unwrap_or(0);
    let dirty = flows
        .get("flows")
        .and_then(Value::as_array)
        .map(|flows| {
            flows
                .iter()
                .filter(|flow| flow.get("uncommitted_edits").and_then(Value::as_bool) == Some(true))
                .count()
        })
        .unwrap_or(0);
    let mut phrase = match count {
        0 => "no flows".to_owned(),
        1 => "flows 1".to_owned(),
        count => format!("flows {count}"),
    };
    if count == 0 {
        return Some(phrase);
    }
    phrase.push_str(&format!(" ({active} active"));
    if dirty > 0 {
        phrase.push_str(&format!("; {dirty} with uncommitted edits",));
    }
    phrase.push(')');
    if let Some(error) = flows.get("error").and_then(Value::as_str) {
        phrase.push_str(&format!("; {error}"));
    }
    Some(phrase)
}

fn now_phrase(now: &Value) -> String {
    if !now.get("present").and_then(Value::as_bool).unwrap_or(false) {
        return "now absent".to_owned();
    }
    let active = now.get("active_items").and_then(Value::as_u64).unwrap_or(0);
    let questions = now
        .get("open_questions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut phrase = format!("now {active} active");
    if questions > 0 {
        phrase.push_str(&format!(
            ", {questions} open question{}",
            if questions == 1 { "" } else { "s" }
        ));
    }
    if let Some(error) = now.get("error").and_then(Value::as_str) {
        phrase.push_str(&format!(" ({error})"));
    }
    phrase
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
        lines.push(format!("    {}", count_phrase(&control[area])));
    }
    if let Some(wiki) = control.get("agent_wiki") {
        match wiki.get("wiki").and_then(wiki_phrase) {
            Some(phrase) => lines.push(format!("    {}; {}", count_phrase(wiki), phrase)),
            None => lines.push(format!("    {}", count_phrase(wiki))),
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
    lines.push(format!(
        "    unresolved provenance — {} of {} sources",
        control
            .get("unresolved_provenance_sources")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        control
            .get("source_bindings")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    ));

    let work = data.get("work").unwrap_or(&Value::Null);
    lines.push("  Work".to_owned());
    if !work.get("exists").and_then(Value::as_bool).unwrap_or(false) {
        lines.push("    Work is missing".to_owned());
        return lines.join("\n");
    }
    let loose = work.get("loose_files").and_then(Value::as_u64).unwrap_or(0);
    if loose > 0 {
        lines.push(format!("    loose files — {loose}"));
    }
    for project in work
        .get("projects")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
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
        // A Project with no ProjectCentral has no Wiki relation to report; the
        // absence is already the first clause of the line.
        if state != "absent" {
            if let Some(wiki) = projectcentral
                .get("agent_wiki")
                .and_then(|area| area.get("wiki"))
                .and_then(wiki_phrase)
            {
                parts.push(wiki);
            }
        }
        if state != "absent" {
            let user = projectcentral.get("user").unwrap_or(&Value::Null);
            parts.push(format!(
                "{} ProjectCentral human sources",
                user.get("sources").and_then(Value::as_u64).unwrap_or(0)
            ));
        }
        if let Some(phrase) = flow_phrase(projectcentral.get("flows").unwrap_or(&Value::Null)) {
            parts.push(phrase);
        }
        parts.push(now_phrase(
            projectcentral.get("now").unwrap_or(&Value::Null),
        ));
        let overrides = projectcentral
            .get("relations")
            .and_then(|relations| relations.get("declared_overrides"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if overrides > 0 {
            parts.push(format!("{overrides} source-relations overrides"));
        }
        parts.push(format!(
            "{} project files",
            project
                .get("source_files")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ));
        lines.push(format!("    {}", parts.join(" — ")));
    }
    lines.join("\n")
}

/// The human projection of the world at Project scope.
pub fn explain_project_world_map(data: &Value) -> String {
    let root = data.get("root").and_then(Value::as_str).unwrap_or_default();
    let ground_state = data
        .get("ground_state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let project = data.get("project").unwrap_or(&Value::Null);
    let name = project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let path = project
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut lines = vec![format!(
        "{root} — Central ground {ground_state} — project {name} ({path})"
    )];

    let projectcentral = project.get("projectcentral").unwrap_or(&Value::Null);
    let state = projectcentral
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut head = format!("  ProjectCentral ({state})");
    if let Some(missing) = projectcentral.get("missing").and_then(Value::as_array) {
        let missing = missing
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        head.push_str(&format!(" — missing {missing}"));
    }
    lines.push(head);
    for area in ["user", "agent_governance"] {
        lines.push(format!("    {}", count_phrase(&projectcentral[area])));
    }
    if let Some(area) = projectcentral.get("agent_wiki") {
        match area.get("wiki").and_then(wiki_phrase) {
            Some(phrase) => lines.push(format!("    {}; {}", count_phrase(area), phrase)),
            None => lines.push(format!("    {}", count_phrase(area))),
        }
    }
    if let Some(relations) = projectcentral.get("relations") {
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
    if let Some(phrase) = flow_phrase(projectcentral.get("flows").unwrap_or(&Value::Null)) {
        lines.push(format!("    {phrase}"));
    }
    lines.push(format!(
        "    {}",
        now_phrase(projectcentral.get("now").unwrap_or(&Value::Null))
    ));

    let sources = data.get("sources").unwrap_or(&Value::Null);
    let mut sources_line = format!(
        "  sources — {} participating",
        sources.get("bindings").and_then(Value::as_u64).unwrap_or(0)
    );
    if let Some(error) = sources.get("error").and_then(Value::as_str) {
        sources_line.push_str(&format!(" ({error})"));
    } else {
        sources_line.push_str(&format!(
            ", {} unresolved provenance",
            sources
                .get("unresolved_provenance")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ));
    }
    lines.push(sources_line);

    let position = data.get("position").unwrap_or(&Value::Null);
    let index = position
        .get("index")
        .and_then(Value::as_u64)
        .map(|index| index.to_string())
        .unwrap_or_else(|| "?".to_owned());
    lines.push(format!(
        "  under {} — project {index} of {}",
        position
            .get("work_root")
            .and_then(Value::as_str)
            .unwrap_or("Work"),
        position
            .get("project_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    ));
    lines.push(format!(
        "  {} project files",
        project
            .get("source_files")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    lines.join("\n")
}

fn stamp_phrase(step: &Value) -> String {
    let path = step.get("path").and_then(Value::as_str).unwrap_or_default();
    let kind = step.get("kind").and_then(Value::as_str).unwrap_or("?");
    format!("{path} ({kind})")
}

fn reproject_lines(data: &Value, stamped_key: &str) -> Vec<String> {
    let project = data
        .get("project")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let projectcentral = data
        .get("projectcentral")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let applying = stamped_key == "stamped";
    let steps = data.get(stamped_key).and_then(Value::as_array);
    let mut lines = vec![format!(
        "Work/{project} — reprojection {}",
        if applying { "applied" } else { "plan" }
    )];
    if applying {
        lines.push(format!("  {projectcentral}"));
    }
    let step_label = if applying {
        "  stamped:"
    } else {
        "  would stamp:"
    };
    let empty_label = if applying {
        "  stamped nothing — nothing was missing"
    } else {
        "  nothing to stamp — ProjectCentral already canonical"
    };
    match steps {
        Some(steps) if !steps.is_empty() => {
            lines.push(step_label.to_owned());
            for step in steps {
                lines.push(format!("    - {}", stamp_phrase(step)));
            }
        }
        Some(_) => lines.push(empty_label.to_owned()),
        None => {}
    }
    if let Some(alone) = data.get("left_alone").and_then(Value::as_array) {
        lines.push(format!("  left alone — {} entries", alone.len()));
        for entry in alone {
            let path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let provenance = entry
                .get("provenance")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let note = entry
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or_default();
            lines.push(format!("    - {path} [{provenance}] {note}"));
        }
    }
    if let Some(blocked) = data.get("blocked").and_then(Value::as_str) {
        lines.push(format!("  blocked — {blocked}"));
    }
    lines.push("  would never:".to_owned());
    let rules = data
        .get("would_not")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for rule in rules {
        if let Some(rule) = rule.as_str() {
            lines.push(format!("    - {rule}"));
        }
    }
    lines
}

/// The human projection of a reprojection plan.
pub fn explain_reproject_plan(data: &Value) -> String {
    reproject_lines(data, "would_stamp").join("\n")
}

/// The human projection of a reprojection receipt.
pub fn explain_reproject_receipt(data: &Value) -> String {
    reproject_lines(data, "stamped").join("\n")
}

fn world_descriptor() -> ActionDescriptor {
    ActionDescriptor {
        id: "central.world".to_owned(),
        title: "Show the world map".to_owned(),
        description: "Index the full world centred in the active Central root: Control source areas, the root and Project WikiSpaces with their child refs, declared source-relations overrides, and per-Project ProjectCentral state with Flows and NOW. Read-only; every fault is reported as data.".to_owned(),
        inputs: vec![],
        output: ActionOutputDefinition { output_type: "central-world-map".to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: true,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn project_input(required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: "project".to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn project_world_descriptor() -> ActionDescriptor {
    ActionDescriptor {
        id: "central.world.project".to_owned(),
        title: "Show the world of one Project".to_owned(),
        description: "Project the world onto one ProjectCentral: its fractal ground, Wiki, Flows, NOW folder, participating sources with their provenance, and its position under Work. Read-only; a missing or partial ProjectCentral is reported as data.".to_owned(),
        inputs: vec![project_input(true)],
        output: ActionOutputDefinition { output_type: "central-world-map".to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: true,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn reproject_plan_descriptor() -> ActionDescriptor {
    ActionDescriptor {
        id: "central.world.reproject.plan".to_owned(),
        title: "Plan ProjectCentral reprojection".to_owned(),
        description: "List the canonical ProjectCentral scaffolding that is missing and would be stamped, classify everything else by source provenance, and state what reprojection would never do. Read-only.".to_owned(),
        inputs: vec![project_input(true)],
        output: ActionOutputDefinition { output_type: "central-world-reproject-plan".to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: true,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn reproject_apply_descriptor() -> ActionDescriptor {
    ActionDescriptor {
        id: "central.world.reproject.apply".to_owned(),
        title: "Reproject ProjectCentral".to_owned(),
        description: "Stamp only the canonical ProjectCentral scaffolding that is missing: project.json, user, agents/governance, agents/wiki with its wiki.json, and relations. Never moves, renames, deletes, relabels, or writes into anything that already exists.".to_owned(),
        inputs: vec![project_input(true)],
        output: ActionOutputDefinition { output_type: "central-world-reproject-receipt".to_owned() },
        mutation_class: MutationClass::LocallyMutating,
        preview_supported: false,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn central_root(context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|root| root.path)
        .map_err(|message| ActionResult::failure(None, ResultStatus::InvalidInput, message, None))
}

fn serialized<T: Serialize>(action: &str, value: T) -> ActionResult {
    ActionResult::success(
        action,
        serde_json::to_value(value).expect("world map serializes"),
    )
}

fn world_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.world";
    let root = match central_root(context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match map_world(&root) {
        Ok(map) => serialized(action, map),
        Err(error) => ActionResult::failure(
            Some(action),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

fn project_world_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.world.project";
    let root = match central_root(context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let Some(name) = input.get("project").and_then(Value::as_str) else {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "central.world.project requires project.".to_owned(),
            None,
        );
    };
    match map_project_world(&root, name) {
        Ok(map) => serialized(action, map),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::InvalidInput =>
        {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                error.to_string(),
                None,
            )
        }
        Err(error) => ActionResult::failure(
            Some(action),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

fn reproject_plan_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.world.reproject.plan";
    let root = match central_root(context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let Some(name) = input.get("project").and_then(Value::as_str) else {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "central.world.reproject.plan requires project.".to_owned(),
            None,
        );
    };
    match plan_reproject(&root, name) {
        Ok(plan) => serialized(action, plan),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::InvalidInput =>
        {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                error.to_string(),
                None,
            )
        }
        Err(error) => ActionResult::failure(
            Some(action),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

fn reproject_apply_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.world.reproject.apply";
    let root = match central_root(context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let Some(name) = input.get("project").and_then(Value::as_str) else {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "central.world.reproject.apply requires project.".to_owned(),
            None,
        );
    };
    match apply_reproject(&root, name) {
        Ok(receipt) => serialized(action, receipt),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::InvalidInput =>
        {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                error.to_string(),
                None,
            )
        }
        Err(error) => ActionResult::failure(
            Some(action),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

pub fn register_world_map_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            world_descriptor(),
            world_action
                as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (project_world_descriptor(), project_world_action),
        (reproject_plan_descriptor(), reproject_plan_action),
        (reproject_apply_descriptor(), reproject_apply_action),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("world map Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempdir;

    #[test]
    fn missing_canonical_pieces_are_named_by_their_ground_relative_path() {
        let temp = tempdir().unwrap();
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
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join(".git/config"), "").unwrap();
        fs::write(root.join("node_modules/index.js"), "").unwrap();
        fs::write(root.join("docs/note.md"), "note").unwrap();
        fs::write(root.join(AGENT_RETRIEVAL_DENY_MARKER), "").unwrap();

        let area = source_area(root, "docs");

        assert_eq!(area.sources, 1);
        assert_eq!(area.path, "docs");
    }

    #[test]
    fn dangling_refs_are_the_children_no_declared_space_answers() {
        let mut wiki = wiki_space(Path::new("/nonexistent/wiki.json"), "x/wiki.json");
        assert!(!wiki.present);

        wiki.child_space_refs = vec!["central:wiki:project:a".to_owned()];
        mark_dangling(
            &mut wiki,
            &BTreeSet::from(["central:wiki:project:a".to_owned()]),
        );
        assert!(wiki.dangling_child_space_refs.is_empty());

        mark_dangling(&mut wiki, &BTreeSet::new());
        assert_eq!(
            wiki.dangling_child_space_refs,
            vec!["central:wiki:project:a"]
        );
    }
}
