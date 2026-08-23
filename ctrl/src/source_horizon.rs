use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::projectcentral_flow::registered_flow_records;
use crate::projectcentral::{
    read_project_manifest, AGENT_GOVERNANCE_DIR, ROOT_AGENT_GOVERNANCE_DIR,
    ROOT_HUMAN_SOURCE_DIR, ROOT_WIKI_DIR, WIKI_DIR,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SOURCE_HORIZON_SCHEMA: &str = "central.source-change-horizon/v1";
pub const SOURCE_CHANGE_SCHEMA: &str = "central.source-change/v1";
pub const SOURCE_HORIZON_PROVIDER: &str = "central.filesystem-reconcile/v1";
pub const PROJECT_HORIZON_STATE: &str = ".central/source-change-horizon.json";
pub const CONTROL_HORIZON_STATE: &str = ".central/source-change-control.json";
pub const GROUND_RELATIONS_SOURCE: &str = "ProjectCentral/relations/source-relations.json";
pub const GROUND_RELATIONS_SCHEMA: &str = "central.project.ground-relations/v1";

const MAX_SCAN_DEPTH: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBinding {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub provenance: String,
    pub standing: String,
    pub treatment: String,
    pub agent_retrieval_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevision {
    pub revision: String,
    pub byte_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSource {
    pub binding: SourceBinding,
    pub revision: SourceRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceChange {
    pub schema: String,
    pub change_ref: String,
    pub cursor: u64,
    pub world_ref: String,
    pub source_ref: String,
    pub source_path: String,
    pub source_roles: Vec<String>,
    pub provenance: String,
    pub standing: String,
    pub treatment: String,
    pub agent_retrieval_allowed: bool,
    pub before_revision: Option<String>,
    pub after_revision: Option<String>,
    pub kind: SourceChangeKind,
    pub observed_at_unix_seconds: u64,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceHorizonState {
    schema: String,
    world_ref: String,
    cursor: u64,
    #[serde(default)]
    sources: BTreeMap<String, ObservedSource>,
    #[serde(default)]
    changes: Vec<SourceChange>,
    #[serde(default)]
    consumer_cursors: BTreeMap<String, u64>,
    provider: String,
    reconciled_at_unix_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceHorizon {
    pub schema: String,
    pub world_ref: String,
    pub cursor: u64,
    pub sources: Vec<ObservedSource>,
    pub changes: Vec<SourceChange>,
    pub consumer_cursors: BTreeMap<String, u64>,
    pub provider: String,
    pub reconciled_at_unix_seconds: u64,
    pub source_payloads_exposed: bool,
    pub automatic_agent_or_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    pub initialized: bool,
    pub new_changes: Vec<SourceChange>,
    pub horizon: SourceHorizon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompactionReport {
    pub before_changes: usize,
    pub after_changes: usize,
    pub minimum_active_cursor: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GroundRelationsFile {
    schema: String,
    project_id: String,
    #[serde(default)]
    relations: Vec<GroundRelation>,
}

#[derive(Debug, Clone, Deserialize)]
struct GroundRelation {
    #[serde(rename = "ref")]
    source_ref: String,
    path: String,
    provenance: String,
    standing: String,
    #[serde(default)]
    roles: Vec<String>,
    treatment: String,
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn normalize_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_project_member(raw: &str) -> io::Result<()> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source path must be non-empty and have no surrounding whitespace",
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
            format!("source path must remain inside its world: {raw}"),
        ));
    }
    Ok(())
}

fn source_ref(world_ref: &str, path: &str) -> String {
    let escaped = path.replace('%', "%25").replace(':', "%3A").replace(' ', "%20");
    format!("central:source:{world_ref}:{escaped}")
}

fn content_revision(path: &Path) -> io::Result<SourceRevision> {
    let bytes = fs::read(path)?;
    // Versioned FNV-1a is deliberately implemented in-tree: a change horizon needs a stable
    // content revision, not a new crypto/package dependency or a platform-specific metadata id.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in &bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(SourceRevision {
        revision: format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len()),
        byte_len: bytes.len() as u64,
    })
}

fn retrieval_allowed(world_root: &Path, source: &Path) -> bool {
    let mut cursor = source.parent();
    while let Some(dir) = cursor {
        if !dir.starts_with(world_root) {
            break;
        }
        if dir.join(AGENT_RETRIEVAL_DENY_MARKER).is_file() {
            return false;
        }
        if dir == world_root {
            break;
        }
        cursor = dir.parent();
    }
    true
}

fn safe_regular_file(world_root: &Path, path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(false);
    }
    let canonical_root = fs::canonicalize(world_root)?;
    let canonical_file = fs::canonicalize(path)?;
    Ok(canonical_file.starts_with(canonical_root))
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name, ".git" | ".central" | "target" | "node_modules" | ".next" | "dist" | "build")
}

fn collect_files(root: &Path, world_root: &Path, depth: usize, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if depth > MAX_SCAN_DEPTH || !root.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !should_skip_dir(&name) {
                collect_files(&path, world_root, depth + 1, files)?;
            }
        } else if file_type.is_file()
            && entry.file_name() != AGENT_RETRIEVAL_DENY_MARKER
            && safe_regular_file(world_root, &path)?
        {
            files.push(path);
        }
    }
    Ok(())
}

fn insert_tree_bindings(
    world_root: &Path,
    scan_root: &Path,
    world_ref: &str,
    roles: &[&str],
    provenance: &str,
    standing: &str,
    treatment: &str,
    bindings: &mut BTreeMap<String, SourceBinding>,
) -> io::Result<()> {
    let mut files = Vec::new();
    collect_files(scan_root, world_root, 0, &mut files)?;
    for file in files {
        let relative = normalize_relative(file.strip_prefix(world_root).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "source escaped its world root")
        })?);
        let reference = source_ref(world_ref, &relative);
        bindings.entry(reference.clone()).or_insert(SourceBinding {
            source_ref: reference,
            path: relative,
            roles: roles.iter().map(|role| (*role).to_owned()).collect(),
            provenance: provenance.to_owned(),
            standing: standing.to_owned(),
            treatment: treatment.to_owned(),
            agent_retrieval_allowed: retrieval_allowed(world_root, &file),
        });
    }
    Ok(())
}

fn read_ground_relations(project_root: &Path, expected_project_id: &str) -> io::Result<Vec<GroundRelation>> {
    let path = project_root.join(GROUND_RELATIONS_SOURCE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let relations: GroundRelationsFile = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if relations.schema != GROUND_RELATIONS_SCHEMA || relations.project_id != expected_project_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ProjectCentral ground relation source has the wrong schema or Project identity",
        ));
    }
    for relation in &relations.relations {
        validate_project_member(&relation.path)?;
    }
    Ok(relations.relations)
}

pub fn project_source_bindings(project_root: &Path) -> io::Result<Vec<SourceBinding>> {
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(io::ErrorKind::InvalidData, validation.errors.join("; ")));
    }
    let world_ref = format!("project:{}", manifest.project_id);
    let mut bindings = BTreeMap::<String, SourceBinding>::new();

    insert_tree_bindings(
        project_root,
        &project_root.join(&manifest.human_source),
        &world_ref,
        &["project-human-source-aperture"],
        "unresolved",
        "unspecified",
        "projectcentral-user",
        &mut bindings,
    )?;
    insert_tree_bindings(
        project_root,
        &project_root.join(AGENT_GOVERNANCE_DIR),
        &world_ref,
        &["agent-governance-source"],
        "unresolved",
        "unspecified",
        "projectcentral-agent-governance",
        &mut bindings,
    )?;
    insert_tree_bindings(
        project_root,
        &project_root.join(WIKI_DIR),
        &world_ref,
        &["agent-wiki-source"],
        "agent-maintained",
        "unspecified",
        "projectcentral-agent-wiki",
        &mut bindings,
    )?;

    for relation in read_ground_relations(project_root, &manifest.project_id)? {
        let relative = relation.path.clone();
        let path = project_root.join(&relative);
        if !safe_regular_file(project_root, &path)? {
            continue;
        }
        // An explicit recognised relation is the identity/standing authority for its path.
        // Remove the aperture fallback first so one physical source produces one logical change.
        bindings.retain(|_, binding| binding.path != relative);
        bindings.insert(
            relation.source_ref.clone(),
            SourceBinding {
                source_ref: relation.source_ref,
                path: relative,
                roles: relation.roles,
                provenance: relation.provenance,
                standing: relation.standing,
                treatment: relation.treatment,
                agent_retrieval_allowed: retrieval_allowed(project_root, &path),
            },
        );
    }

    // Current accepted ProjectCentral already supports Wiki sources retained in place. They are
    // participants, not generic Project truth, and therefore retain an explicit Wiki role.
    for adopted in &manifest.wiki.adopted_sources {
        validate_project_member(adopted)?;
        let path = project_root.join(adopted);
        if safe_regular_file(project_root, &path)? {
            let reference = source_ref(&world_ref, adopted);
            bindings.entry(reference.clone()).or_insert(SourceBinding {
                source_ref: reference,
                path: adopted.clone(),
                roles: vec!["adopted-agent-wiki-source".to_owned()],
                provenance: "unresolved".to_owned(),
                standing: "unspecified".to_owned(),
                treatment: "retain-native-in-place".to_owned(),
                agent_retrieval_allowed: retrieval_allowed(project_root, &path),
            });
        }
    }

    for flow in registered_flow_records(project_root)? {
        validate_project_member(&flow.path)?;
        let path = project_root.join(&flow.path);
        if !safe_regular_file(project_root, &path)? {
            continue;
        }
        // FlowRef is the continuity identity. SourceRef remains the current ordinary-file
        // relation and may therefore change when the retained source is renamed.
        bindings.retain(|_, binding| binding.path != flow.path);
        bindings.insert(
            flow.source_ref.clone(),
            SourceBinding {
                source_ref: flow.source_ref,
                path: flow.path,
                roles: vec!["flow-source".to_owned()],
                provenance: "collaborative-revision-provenance".to_owned(),
                standing: "working-source".to_owned(),
                treatment: "projectcentral-flow-retained-in-place".to_owned(),
                agent_retrieval_allowed: retrieval_allowed(project_root, &path),
            },
        );
    }

    Ok(bindings.into_values().collect())
}

pub fn control_source_bindings(central_root: &Path) -> io::Result<Vec<SourceBinding>> {
    let world_ref = "control:root";
    let mut bindings = BTreeMap::<String, SourceBinding>::new();
    insert_tree_bindings(
        central_root,
        &central_root.join(ROOT_HUMAN_SOURCE_DIR),
        world_ref,
        &["personal-human-source-aperture"],
        "unresolved",
        "unspecified",
        "control-user",
        &mut bindings,
    )?;
    insert_tree_bindings(
        central_root,
        &central_root.join(ROOT_AGENT_GOVERNANCE_DIR),
        world_ref,
        &["agent-governance-source"],
        "unresolved",
        "unspecified",
        "control-agent-governance",
        &mut bindings,
    )?;
    insert_tree_bindings(
        central_root,
        &central_root.join(ROOT_WIKI_DIR),
        world_ref,
        &["agent-wiki-source"],
        "agent-maintained",
        "unspecified",
        "control-agent-wiki",
        &mut bindings,
    )?;
    Ok(bindings.into_values().collect())
}

fn observe_bindings(world_root: &Path, bindings: Vec<SourceBinding>) -> io::Result<BTreeMap<String, ObservedSource>> {
    let mut observed = BTreeMap::new();
    for binding in bindings {
        validate_project_member(&binding.path)?;
        let path = world_root.join(&binding.path);
        if !safe_regular_file(world_root, &path)? {
            continue;
        }
        let revision = content_revision(&path)?;
        observed.insert(binding.source_ref.clone(), ObservedSource { binding, revision });
    }
    Ok(observed)
}

fn load_state(path: &Path) -> io::Result<Option<SourceHorizonState>> {
    if !path.is_file() {
        return Ok(None);
    }
    let state: SourceHorizonState = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if state.schema != SOURCE_HORIZON_SCHEMA || state.provider != SOURCE_HORIZON_PROVIDER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source horizon state has an unsupported schema/provider",
        ));
    }
    Ok(Some(state))
}

fn write_state(path: &Path, state: &SourceHorizonState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

fn public_horizon(state: &SourceHorizonState, since: Option<u64>) -> SourceHorizon {
    let cutoff = since.unwrap_or(0);
    SourceHorizon {
        schema: state.schema.clone(),
        world_ref: state.world_ref.clone(),
        cursor: state.cursor,
        sources: state.sources.values().cloned().collect(),
        changes: state
            .changes
            .iter()
            .filter(|change| change.cursor > cutoff)
            .cloned()
            .collect(),
        consumer_cursors: state.consumer_cursors.clone(),
        provider: state.provider.clone(),
        reconciled_at_unix_seconds: state.reconciled_at_unix_seconds,
        source_payloads_exposed: false,
        automatic_agent_or_model_invocation: false,
    }
}

fn reconcile(
    world_root: &Path,
    state_path: &Path,
    world_ref: &str,
    bindings: Vec<SourceBinding>,
) -> io::Result<ReconcileReport> {
    let current = observe_bindings(world_root, bindings)?;
    let now = unix_seconds();
    let existing = load_state(state_path)?;
    let initialized = existing.is_none();
    let mut state = existing.unwrap_or(SourceHorizonState {
        schema: SOURCE_HORIZON_SCHEMA.to_owned(),
        world_ref: world_ref.to_owned(),
        cursor: 0,
        sources: BTreeMap::new(),
        changes: Vec::new(),
        consumer_cursors: BTreeMap::new(),
        provider: SOURCE_HORIZON_PROVIDER.to_owned(),
        reconciled_at_unix_seconds: now,
    });
    if state.world_ref != world_ref {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source horizon world identity changed",
        ));
    }

    let mut new_changes = Vec::new();
    if !initialized {
        let previous_refs = state.sources.keys().cloned().collect::<BTreeSet<_>>();
        let current_refs = current.keys().cloned().collect::<BTreeSet<_>>();
        let all_refs = previous_refs.union(&current_refs).cloned().collect::<Vec<_>>();
        for reference in all_refs {
            let before = state.sources.get(&reference);
            let after = current.get(&reference);
            let kind = match (before, after) {
                (None, Some(_)) => Some(SourceChangeKind::Added),
                (Some(_), None) => Some(SourceChangeKind::Removed),
                (Some(left), Some(right)) if left.revision.revision != right.revision.revision => {
                    Some(SourceChangeKind::Modified)
                }
                _ => None,
            };
            let Some(kind) = kind else { continue };
            state.cursor = state.cursor.saturating_add(1);
            let basis = after.or(before).expect("change has a before or after source");
            let change = SourceChange {
                schema: SOURCE_CHANGE_SCHEMA.to_owned(),
                change_ref: format!("central:change:{world_ref}:{}", state.cursor),
                cursor: state.cursor,
                world_ref: world_ref.to_owned(),
                source_ref: reference,
                source_path: basis.binding.path.clone(),
                source_roles: basis.binding.roles.clone(),
                provenance: basis.binding.provenance.clone(),
                standing: basis.binding.standing.clone(),
                treatment: basis.binding.treatment.clone(),
                agent_retrieval_allowed: basis.binding.agent_retrieval_allowed,
                before_revision: before.map(|value| value.revision.revision.clone()),
                after_revision: after.map(|value| value.revision.revision.clone()),
                kind,
                observed_at_unix_seconds: now,
                provider: SOURCE_HORIZON_PROVIDER.to_owned(),
                actor: None,
            };
            state.changes.push(change.clone());
            new_changes.push(change);
        }
    }
    state.sources = current;
    state.reconciled_at_unix_seconds = now;
    write_state(state_path, &state)?;
    let horizon = public_horizon(&state, None);
    Ok(ReconcileReport { initialized, new_changes, horizon })
}

pub fn reconcile_project_sources(project_root: &Path) -> io::Result<ReconcileReport> {
    let manifest = read_project_manifest(project_root)?;
    let world_ref = format!("project:{}", manifest.project_id);
    reconcile(
        project_root,
        &project_root.join(PROJECT_HORIZON_STATE),
        &world_ref,
        project_source_bindings(project_root)?,
    )
}

pub fn reconcile_control_sources(central_root: &Path) -> io::Result<ReconcileReport> {
    reconcile(
        central_root,
        &central_root.join(CONTROL_HORIZON_STATE),
        "control:root",
        control_source_bindings(central_root)?,
    )
}

pub fn read_project_change_horizon(project_root: &Path, since: Option<u64>) -> io::Result<SourceHorizon> {
    // Reading the current horizon is also the correctness reconciliation path. This makes direct
    // external edits available without a manual sync command while keeping all mutation under
    // derived .central state and never invoking an Agent/model.
    let report = reconcile_project_sources(project_root)?;
    let state = load_state(&project_root.join(PROJECT_HORIZON_STATE))?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source horizon state was not created"))?;
    let _ = report;
    Ok(public_horizon(&state, since))
}

pub fn acknowledge_project_cursor(project_root: &Path, consumer: &str, cursor: u64) -> io::Result<SourceHorizon> {
    if consumer.trim().is_empty() || consumer != consumer.trim() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "consumer must be non-empty"));
    }
    let path = project_root.join(PROJECT_HORIZON_STATE);
    let mut state = load_state(&path)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source horizon must be reconciled before acknowledgement"))?;
    if cursor > state.cursor {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cannot acknowledge cursor {cursor} beyond current cursor {}", state.cursor),
        ));
    }
    let entry = state.consumer_cursors.entry(consumer.to_owned()).or_insert(0);
    *entry = (*entry).max(cursor);
    write_state(&path, &state)?;
    Ok(public_horizon(&state, None))
}

pub fn compact_project_changes(project_root: &Path) -> io::Result<CompactionReport> {
    let path = project_root.join(PROJECT_HORIZON_STATE);
    let mut state = load_state(&path)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source horizon must be reconciled before compaction"))?;
    let before_changes = state.changes.len();
    let minimum_active_cursor = state.consumer_cursors.values().copied().min();
    if let Some(cursor) = minimum_active_cursor {
        state.changes.retain(|change| change.cursor > cursor);
        write_state(&path, &state)?;
    }
    Ok(CompactionReport {
        before_changes,
        after_changes: state.changes.len(),
        minimum_active_cursor,
    })
}

fn action_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    inputs: Vec<ActionInputDefinition>,
    output_type: &str,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition { output_type: output_type.to_owned() },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
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
        .ok_or_else(|| ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} requires {field}."),
            None,
        ))
}

fn project_root(action: &str, input: &Value, context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    validate_project_member(&project).map_err(|error| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
    })?;
    let central = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?.path;
    let root = central.join("Work").join(project);
    if !root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", root.display()),
            None,
        ));
    }
    Ok(root)
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn horizon_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.change.horizon";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let since = input.get("cursor").and_then(Value::as_u64);
    read_project_change_horizon(&root, since)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("horizon serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn reconcile_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.change.reconcile";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    reconcile_project_sources(&root)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("reconcile serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn acknowledge_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.change.ack";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let consumer = match required(input, "consumer", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let cursor = match input.get("cursor").and_then(Value::as_u64) {
        Some(value) => value,
        None => {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires numeric cursor."),
                None,
            )
        }
    };
    acknowledge_project_cursor(&root, &consumer, cursor)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("horizon serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_source_horizon_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor(
                "projectcentral.change.horizon",
                "Read current Source Change Horizon",
                "Reconcile participating Project sources into deterministic revisions and return the current change horizon. This updates derived .central state only and never invokes an Agent/model.",
                MutationClass::LocallyMutating,
                vec![action_input("project", true), action_input("cursor", false)],
                "central-source-change-horizon",
            ),
            horizon_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.change.reconcile",
                "Reconcile Project source revisions",
                "Correct watcher/startup hints by rescanning the authoritative participating source set and emitting logical changes only for revision differences.",
                MutationClass::LocallyMutating,
                vec![action_input("project", true)],
                "central-source-change-reconcile",
            ),
            reconcile_action,
        ),
        (
            descriptor(
                "projectcentral.change.ack",
                "Acknowledge Source Change cursor",
                "Advance one named consumer cursor without changing any source. Compaction may only remove changes older than every active cursor.",
                MutationClass::LocallyMutating,
                vec![
                    action_input("project", true),
                    action_input("consumer", true),
                    action_input("cursor", true),
                ],
                "central-source-change-horizon",
            ),
            acknowledge_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("Source Change Horizon Action ids are valid");
    }
}
