use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::read_project_manifest;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::reconcile_project_sources;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FLOW_REGISTRY: &str = ".central/flows.json";
pub const FLOW_HISTORY_DIR: &str = ".central/flow-revisions";
pub const DEFAULT_FLOW_DIR: &str = "ProjectCentral/now/flows";
pub const FLOW_REGISTRY_SCHEMA: &str = "central.project-flow-registry/v1";
pub const FLOW_DAY_SCHEMA: &str = "central.project-flow-day/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRevisionReceipt {
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_revision: Option<String>,
    pub actor: String,
    pub actor_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    pub recorded_at_unix_seconds: u64,
    pub source_path: String,
    pub history_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRecord {
    pub flow_ref: String,
    pub source_ref: String,
    pub path: String,
    pub created_at_unix_seconds: u64,
    pub current_revision: String,
    pub lifecycle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub scope_ref: String,
    pub privacy: String,
    #[serde(default)]
    pub revisions: Vec<FlowRevisionReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FlowRegistry {
    schema: String,
    project_id: String,
    #[serde(default)]
    flows: Vec<FlowRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowReading {
    pub schema: String,
    pub flow: FlowRecord,
    pub content: String,
    pub dirty_external_revision_reconciled: bool,
    pub automatic_agent_or_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowList {
    pub schema: String,
    pub project_id: String,
    pub flows: Vec<FlowRecord>,
    pub automatic_agent_or_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowDaySnapshot {
    pub schema: String,
    pub day: String,
    pub flow_ref: String,
    pub source_ref: String,
    pub source_path: String,
    pub revision: String,
    pub lifecycle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub snapshot_source: String,
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unique_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn relative_member(raw: &str) -> io::Result<PathBuf> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow path must be non-empty without surrounding whitespace",
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
            "Flow path must stay inside its owner world and contain no parent/root components",
        ));
    }
    Ok(path.to_path_buf())
}

fn reject_symlink_components(project_root: &Path, relative: &Path) -> io::Result<()> {
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "non-normal Flow path"));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("refusing symlink Flow path component: {}", current.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn safe_flow_path(project_root: &Path, raw: &str, must_exist: bool) -> io::Result<PathBuf> {
    let relative = relative_member(raw)?;
    reject_symlink_components(project_root, &relative)?;
    let path = project_root.join(&relative);
    if must_exist {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Flow source must be an ordinary file"));
        }
        let root = fs::canonicalize(project_root)?;
        let source = fs::canonicalize(&path)?;
        if !source.starts_with(root) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Flow source escaped its Project world"));
        }
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        reject_symlink_components(project_root, relative.parent().unwrap_or(Path::new("")))?;
    }
    Ok(path)
}

fn content_revision_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len())
}

fn content_revision(path: &Path) -> io::Result<String> {
    Ok(content_revision_bytes(&fs::read(path)?))
}

fn escaped_source_ref(project_id: &str, path: &str) -> String {
    let escaped = path.replace('%', "%25").replace(':', "%3A").replace(' ', "%20");
    format!("central:source:project:{project_id}:{escaped}")
}

fn flow_key(flow_ref: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in flow_ref.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn revision_key(revision: &str) -> String {
    revision.rsplit(':').next().unwrap_or("revision").to_owned()
}

fn registry_path(project_root: &Path) -> PathBuf {
    project_root.join(FLOW_REGISTRY)
}

fn load_registry(project_root: &Path) -> io::Result<FlowRegistry> {
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(io::ErrorKind::InvalidData, validation.errors.join("; ")));
    }
    let path = registry_path(project_root);
    if !path.is_file() {
        return Ok(FlowRegistry {
            schema: FLOW_REGISTRY_SCHEMA.into(),
            project_id: manifest.project_id,
            flows: vec![],
        });
    }
    let registry: FlowRegistry = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if registry.schema != FLOW_REGISTRY_SCHEMA || registry.project_id != manifest.project_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Flow registry schema or Project identity is invalid",
        ));
    }
    Ok(registry)
}

fn write_registry(project_root: &Path, registry: &FlowRegistry) -> io::Result<()> {
    let path = registry_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(registry)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

fn history_path(project_root: &Path, flow_ref: &str, revision: &str) -> PathBuf {
    project_root
        .join(FLOW_HISTORY_DIR)
        .join(flow_key(flow_ref))
        .join(format!("{}.md", revision_key(revision)))
}

fn relative(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn store_revision(
    project_root: &Path,
    record: &mut FlowRecord,
    bytes: &[u8],
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<bool> {
    let revision = content_revision_bytes(bytes);
    if revision == record.current_revision {
        return Ok(false);
    }
    let parent_revision = if record.current_revision.is_empty() {
        None
    } else {
        Some(record.current_revision.clone())
    };
    let history = history_path(project_root, &record.flow_ref, &revision);
    if let Some(parent) = history.parent() {
        fs::create_dir_all(parent)?;
    }
    if !history.exists() {
        fs::write(&history, bytes)?;
    }
    record.current_revision = revision.clone();
    record.revisions.push(FlowRevisionReceipt {
        revision,
        parent_revision,
        actor: actor.to_owned(),
        actor_kind: actor_kind.to_owned(),
        agent_session_ref,
        recorded_at_unix_seconds: unix_seconds(),
        source_path: record.path.clone(),
        history_source: relative(project_root, &history),
    });
    Ok(true)
}

fn seed_revision(
    project_root: &Path,
    record: &mut FlowRecord,
    bytes: &[u8],
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<()> {
    let revision = content_revision_bytes(bytes);
    let history = history_path(project_root, &record.flow_ref, &revision);
    if let Some(parent) = history.parent() {
        fs::create_dir_all(parent)?;
    }
    if !history.exists() {
        fs::write(&history, bytes)?;
    }
    record.current_revision = revision.clone();
    record.revisions.push(FlowRevisionReceipt {
        revision,
        parent_revision: None,
        actor: actor.to_owned(),
        actor_kind: actor_kind.to_owned(),
        agent_session_ref,
        recorded_at_unix_seconds: unix_seconds(),
        source_path: record.path.clone(),
        history_source: relative(project_root, &history),
    });
    Ok(())
}

fn reconcile_record(project_root: &Path, record: &mut FlowRecord) -> io::Result<bool> {
    let path = safe_flow_path(project_root, &record.path, true)?;
    let bytes = fs::read(path)?;
    store_revision(project_root, record, &bytes, "unknown", "unknown-external", None)
}

fn validate_actor_kind(kind: &str) -> io::Result<()> {
    if matches!(kind, "human" | "agent" | "system") {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "actor_kind must be human, agent, or system",
        ))
    }
}

fn validate_lifecycle(value: &str) -> io::Result<()> {
    if matches!(value, "active" | "dormant" | "closed") {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow lifecycle must be active, dormant, or closed",
        ))
    }
}

fn validate_flow_placement(project_root: &Path, path: &str) -> io::Result<()> {
    let manifest = read_project_manifest(project_root)?;
    let candidate = Path::new(path);
    let reserved = [
        manifest.human_source.as_str(),
        "ProjectCentral/agents/governance",
        "ProjectCentral/agents/wiki",
        "ProjectCentral/now/user",
        "ProjectCentral/now/agents",
    ];
    if reserved.iter().any(|root| candidate.starts_with(Path::new(root))) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow source placement would overlap an existing Central authority container; retain Flow in its own or a neutral provider/domain-local source container",
        ));
    }
    Ok(())
}

fn default_path(local_stamp: Option<&str>) -> io::Result<String> {
    let filename = match local_stamp {
        Some(stamp) => {
            let valid = stamp.len() == 15
                && stamp.as_bytes()[4] == b'-'
                && stamp.as_bytes()[7] == b'-'
                && stamp.as_bytes()[10] == b'-'
                && stamp
                    .bytes()
                    .enumerate()
                    .all(|(index, byte)| matches!(index, 4 | 7 | 10) || byte.is_ascii_digit());
            if !valid {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "local_stamp must use YYYY-MM-DD-HHMM",
                ));
            }
            format!("{stamp}.md")
        }
        None => format!("flow-{}.md", unique_nanos()),
    };
    Ok(format!("{DEFAULT_FLOW_DIR}/{filename}"))
}

fn ensure_unique_path(registry: &FlowRegistry, path: &str, except_flow: Option<&str>) -> io::Result<()> {
    if registry
        .flows
        .iter()
        .any(|flow| flow.path == path && Some(flow.flow_ref.as_str()) != except_flow)
    {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "another Flow already owns that path"));
    }
    Ok(())
}

pub fn registered_flow_records(project_root: &Path) -> io::Result<Vec<FlowRecord>> {
    Ok(load_registry(project_root)?.flows)
}

pub fn list_flows(project_root: &Path) -> io::Result<FlowList> {
    let mut registry = load_registry(project_root)?;
    let mut changed = false;
    for flow in &mut registry.flows {
        changed |= reconcile_record(project_root, flow)?;
    }
    if changed {
        write_registry(project_root, &registry)?;
    }
    let _ = reconcile_project_sources(project_root)?;
    Ok(FlowList {
        schema: "central.project-flow-list/v1".into(),
        project_id: registry.project_id,
        flows: registry.flows,
        automatic_agent_or_model_invocation: false,
    })
}

pub fn create_flow(
    project_root: &Path,
    local_stamp: Option<&str>,
    explicit_path: Option<&str>,
    title: Option<String>,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<FlowRecord> {
    validate_actor_kind(actor_kind)?;
    let mut registry = load_registry(project_root)?;
    let path = match explicit_path {
        Some(path) => relative_member(path)?.to_string_lossy().replace('\\', "/"),
        None => default_path(local_stamp)?,
    };
    ensure_unique_path(&registry, &path, None)?;
    validate_flow_placement(project_root, &path)?;
    let source = safe_flow_path(project_root, &path, false)?;
    if source.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Flow source already exists; use adopt for retained source"));
    }
    fs::write(&source, b"")?;
    let flow_ref = format!("central:flow:project:{}:{}", registry.project_id, unique_nanos());
    let mut record = FlowRecord {
        flow_ref: flow_ref.clone(),
        source_ref: escaped_source_ref(&registry.project_id, &path),
        path,
        created_at_unix_seconds: unix_seconds(),
        current_revision: String::new(),
        lifecycle: "active".into(),
        title,
        scope_ref: format!("project:{}", registry.project_id),
        privacy: "inherits-source-authority".into(),
        revisions: vec![],
    };
    seed_revision(project_root, &mut record, b"", actor, actor_kind, agent_session_ref)?;
    registry.flows.push(record.clone());
    registry.flows.sort_by(|left, right| left.flow_ref.cmp(&right.flow_ref));
    write_registry(project_root, &registry)?;
    let _ = reconcile_project_sources(project_root)?;
    Ok(record)
}

pub fn adopt_flow(
    project_root: &Path,
    raw_path: &str,
    title: Option<String>,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<FlowRecord> {
    validate_actor_kind(actor_kind)?;
    let mut registry = load_registry(project_root)?;
    let path = relative_member(raw_path)?.to_string_lossy().replace('\\', "/");
    ensure_unique_path(&registry, &path, None)?;
    validate_flow_placement(project_root, &path)?;
    let source = safe_flow_path(project_root, &path, true)?;
    let bytes = fs::read(source)?;
    let flow_ref = format!("central:flow:project:{}:{}", registry.project_id, unique_nanos());
    let mut record = FlowRecord {
        flow_ref,
        source_ref: escaped_source_ref(&registry.project_id, &path),
        path,
        created_at_unix_seconds: unix_seconds(),
        current_revision: String::new(),
        lifecycle: "active".into(),
        title,
        scope_ref: format!("project:{}", registry.project_id),
        privacy: "inherits-source-authority".into(),
        revisions: vec![],
    };
    seed_revision(project_root, &mut record, &bytes, actor, actor_kind, agent_session_ref)?;
    registry.flows.push(record.clone());
    registry.flows.sort_by(|left, right| left.flow_ref.cmp(&right.flow_ref));
    write_registry(project_root, &registry)?;
    let _ = reconcile_project_sources(project_root)?;
    Ok(record)
}

pub fn read_flow(project_root: &Path, flow_ref: &str) -> io::Result<FlowReading> {
    let mut registry = load_registry(project_root)?;
    let flow = registry
        .flows
        .iter_mut()
        .find(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "FlowRef is not registered in this Project"))?;
    let reconciled = reconcile_record(project_root, flow)?;
    let source = safe_flow_path(project_root, &flow.path, true)?;
    let content = String::from_utf8(fs::read(source)?)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Flow source is not UTF-8 text"))?;
    let record = flow.clone();
    if reconciled {
        write_registry(project_root, &registry)?;
    }
    let _ = reconcile_project_sources(project_root)?;
    Ok(FlowReading {
        schema: "central.project-flow-reading/v1".into(),
        flow: record,
        content,
        dirty_external_revision_reconciled: reconciled,
        automatic_agent_or_model_invocation: false,
    })
}

pub fn write_flow(
    project_root: &Path,
    flow_ref: &str,
    expected_revision: &str,
    content: &str,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<FlowRecord> {
    validate_actor_kind(actor_kind)?;
    let mut registry = load_registry(project_root)?;
    let index = registry
        .flows
        .iter()
        .position(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "FlowRef is not registered in this Project"))?;
    let reconciled = reconcile_record(project_root, &mut registry.flows[index])?;
    if reconciled {
        write_registry(project_root, &registry)?;
    }
    if registry.flows[index].current_revision != expected_revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "Flow revision conflict: expected {expected_revision}, current {}",
                registry.flows[index].current_revision
            ),
        ));
    }
    let source = safe_flow_path(project_root, &registry.flows[index].path, true)?;
    fs::write(&source, content.as_bytes())?;
    let changed = store_revision(
        project_root,
        &mut registry.flows[index],
        content.as_bytes(),
        actor,
        actor_kind,
        agent_session_ref,
    )?;
    if changed {
        write_registry(project_root, &registry)?;
    }
    let record = registry.flows[index].clone();
    let _ = reconcile_project_sources(project_root)?;
    Ok(record)
}

pub fn rename_flow(
    project_root: &Path,
    flow_ref: &str,
    expected_revision: &str,
    new_path: &str,
) -> io::Result<FlowRecord> {
    let mut registry = load_registry(project_root)?;
    let index = registry
        .flows
        .iter()
        .position(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "FlowRef is not registered in this Project"))?;
    if reconcile_record(project_root, &mut registry.flows[index])? {
        write_registry(project_root, &registry)?;
    }
    if registry.flows[index].current_revision != expected_revision {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Flow revision conflict before rename"));
    }
    let normalized = relative_member(new_path)?.to_string_lossy().replace('\\', "/");
    ensure_unique_path(&registry, &normalized, Some(flow_ref))?;
    validate_flow_placement(project_root, &normalized)?;
    let destination = safe_flow_path(project_root, &normalized, false)?;
    if destination.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "rename destination already exists"));
    }
    let old = safe_flow_path(project_root, &registry.flows[index].path, true)?;
    fs::rename(old, &destination)?;
    registry.flows[index].path = normalized.clone();
    registry.flows[index].source_ref = escaped_source_ref(&registry.project_id, &normalized);
    write_registry(project_root, &registry)?;
    let record = registry.flows[index].clone();
    let _ = reconcile_project_sources(project_root)?;
    Ok(record)
}

pub fn set_flow_lifecycle(
    project_root: &Path,
    flow_ref: &str,
    expected_revision: &str,
    lifecycle: &str,
) -> io::Result<FlowRecord> {
    validate_lifecycle(lifecycle)?;
    let mut registry = load_registry(project_root)?;
    let index = registry
        .flows
        .iter()
        .position(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "FlowRef is not registered in this Project"))?;
    if reconcile_record(project_root, &mut registry.flows[index])? {
        write_registry(project_root, &registry)?;
    }
    if registry.flows[index].current_revision != expected_revision {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Flow revision conflict before lifecycle change"));
    }
    registry.flows[index].lifecycle = lifecycle.to_owned();
    write_registry(project_root, &registry)?;
    Ok(registry.flows[index].clone())
}

pub fn snapshot_flows_for_day(
    project_root: &Path,
    snapshot_root: &Path,
    day: &str,
) -> io::Result<Vec<FlowDaySnapshot>> {
    let mut registry = load_registry(project_root)?;
    let mut changed = false;
    for flow in &mut registry.flows {
        changed |= reconcile_record(project_root, flow)?;
    }
    if changed {
        write_registry(project_root, &registry)?;
    }
    if registry.flows.is_empty() {
        return Ok(vec![]);
    }
    let root = snapshot_root.join("flows");
    fs::create_dir_all(&root)?;
    let mut snapshots = Vec::new();
    for flow in &registry.flows {
        let source = safe_flow_path(project_root, &flow.path, true)?;
        let target = root.join(format!("{}.md", flow_key(&flow.flow_ref)));
        fs::copy(source, &target)?;
        snapshots.push(FlowDaySnapshot {
            schema: FLOW_DAY_SCHEMA.into(),
            day: day.to_owned(),
            flow_ref: flow.flow_ref.clone(),
            source_ref: flow.source_ref.clone(),
            source_path: flow.path.clone(),
            revision: flow.current_revision.clone(),
            lifecycle: flow.lifecycle.clone(),
            title: flow.title.clone(),
            snapshot_source: relative(project_root, &target),
        });
    }
    let mut bytes = serde_json::to_vec_pretty(&snapshots)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(snapshot_root.join("flows.json"), bytes)?;
    Ok(snapshots)
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

fn optional(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn project_context(action: &str, input: &Value, context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    let project = relative_member(&project).map_err(|error| ActionResult::failure(
        Some(action), ResultStatus::InvalidInput, error.to_string(), None,
    ))?;
    let root = resolve_central_root(context.root_options).map_err(|message| ActionResult::failure(
        Some(action), ResultStatus::InvalidInput, message, None,
    ))?.path;
    let project_root = root.join("Work").join(project);
    if !project_root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", project_root.display()),
            None,
        ));
    }
    read_project_manifest(&project_root).map_err(|error| ActionResult::failure(
        Some(action), ResultStatus::InvalidCentralStructure, error.to_string(), None,
    ))?;
    Ok(project_root)
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::AlreadyExists => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.into(),
        input_type: "string".into(),
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
    output_type: &str,
    inputs: &[(&str, bool)],
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        inputs: inputs.iter().map(|(name, required)| input(name, *required)).collect(),
        output: ActionOutputDefinition { output_type: output_type.into() },
        mutation_class,
        preview_supported: false,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn list_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.list";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    list_flows(&root)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("Flow list serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn read_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.read";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let flow_ref = match required(input, "flow_ref", action) { Ok(value) => value, Err(result) => return result };
    read_flow(&root, &flow_ref)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("Flow reading serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn create_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.create";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let actor = match required(input, "actor", action) { Ok(value) => value, Err(result) => return result };
    let actor_kind = match required(input, "actor_kind", action) { Ok(value) => value, Err(result) => return result };
    create_flow(
        &root,
        optional(input, "local_stamp").as_deref(),
        optional(input, "path").as_deref(),
        optional(input, "title"),
        &actor,
        &actor_kind,
        optional(input, "agent_session_ref"),
    )
    .map(|flow| ActionResult::success(action, json!({"flow": flow, "automatic_agent_or_model_invocation": false})))
    .unwrap_or_else(|error| io_failure(action, error))
}

fn adopt_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.adopt";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let path = match required(input, "path", action) { Ok(value) => value, Err(result) => return result };
    let actor = match required(input, "actor", action) { Ok(value) => value, Err(result) => return result };
    let actor_kind = match required(input, "actor_kind", action) { Ok(value) => value, Err(result) => return result };
    adopt_flow(&root, &path, optional(input, "title"), &actor, &actor_kind, optional(input, "agent_session_ref"))
        .map(|flow| ActionResult::success(action, json!({"flow": flow, "automatic_agent_or_model_invocation": false})))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn write_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.write";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let flow_ref = match required(input, "flow_ref", action) { Ok(value) => value, Err(result) => return result };
    let expected_revision = match required(input, "expected_revision", action) { Ok(value) => value, Err(result) => return result };
    let content = input.get("content").and_then(Value::as_str).unwrap_or("").to_owned();
    let actor = match required(input, "actor", action) { Ok(value) => value, Err(result) => return result };
    let actor_kind = match required(input, "actor_kind", action) { Ok(value) => value, Err(result) => return result };
    write_flow(&root, &flow_ref, &expected_revision, &content, &actor, &actor_kind, optional(input, "agent_session_ref"))
        .map(|flow| ActionResult::success(action, json!({"flow": flow, "automatic_agent_or_model_invocation": false})))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn rename_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.rename";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let flow_ref = match required(input, "flow_ref", action) { Ok(value) => value, Err(result) => return result };
    let expected_revision = match required(input, "expected_revision", action) { Ok(value) => value, Err(result) => return result };
    let new_path = match required(input, "new_path", action) { Ok(value) => value, Err(result) => return result };
    rename_flow(&root, &flow_ref, &expected_revision, &new_path)
        .map(|flow| ActionResult::success(action, json!({"flow": flow, "automatic_agent_or_model_invocation": false})))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn lifecycle_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.lifecycle";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let flow_ref = match required(input, "flow_ref", action) { Ok(value) => value, Err(result) => return result };
    let expected_revision = match required(input, "expected_revision", action) { Ok(value) => value, Err(result) => return result };
    let lifecycle = match required(input, "lifecycle", action) { Ok(value) => value, Err(result) => return result };
    set_flow_lifecycle(&root, &flow_ref, &expected_revision, &lifecycle)
        .map(|flow| ActionResult::success(action, json!({"flow": flow, "automatic_agent_or_model_invocation": false})))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn history_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.flow.history";
    let root = match project_context(action, input, context) { Ok(root) => root, Err(result) => return result };
    let flow_ref = match required(input, "flow_ref", action) { Ok(value) => value, Err(result) => return result };
    match read_flow(&root, &flow_ref) {
        Ok(reading) => ActionResult::success(action, json!({
            "flow_ref": reading.flow.flow_ref,
            "current_revision": reading.flow.current_revision,
            "revisions": reading.flow.revisions,
            "automatic_agent_or_model_invocation": false,
        })),
        Err(error) => io_failure(action, error),
    }
}

pub fn register_projectcentral_flow_actions(registry: &mut ActionRegistry) {
    let actions = [
        (descriptor("projectcentral.flow.list", "List Project Flows", "List stable Flow identities and current source/revision/lifecycle state. Reconciles external file edits into revision provenance and Source Change Horizon without invoking an Agent/model.", MutationClass::LocallyMutating, "projectcentral-flow-list", &[("project", true)]), list_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult),
        (descriptor("projectcentral.flow.read", "Read Project Flow", "Read the current ordinary Flow source by stable FlowRef and reconcile any external editor revision with actor unknown.", MutationClass::LocallyMutating, "projectcentral-flow-reading", &[("project", true), ("flow_ref", true)]), read_action),
        (descriptor("projectcentral.flow.create", "Create Project Flow", "Create a blank ordinary-file Flow with stable FlowRef. ProjectCentral/now/flows/YYYY-MM-DD-HHMM.md is the default convention when local_stamp is supplied; path is not identity.", MutationClass::LocallyMutating, "projectcentral-flow", &[("project", true), ("actor", true), ("actor_kind", true), ("local_stamp", false), ("path", false), ("title", false), ("agent_session_ref", false)]), create_action),
        (descriptor("projectcentral.flow.adopt", "Adopt retained source as Flow", "Give an existing ordinary Project file a stable FlowRef without moving it, preserving provider/domain-local placement.", MutationClass::LocallyMutating, "projectcentral-flow", &[("project", true), ("path", true), ("actor", true), ("actor_kind", true), ("title", false), ("agent_session_ref", false)]), adopt_action),
        (descriptor("projectcentral.flow.write", "Write Project Flow revision", "Revision-safe canonical whole-file write shared by human and Agent callers. A stale expected_revision returns an explicit conflict.", MutationClass::LocallyMutating, "projectcentral-flow", &[("project", true), ("flow_ref", true), ("expected_revision", true), ("content", false), ("actor", true), ("actor_kind", true), ("agent_session_ref", false)]), write_action),
        (descriptor("projectcentral.flow.rename", "Rename Project Flow source", "Move the retained ordinary file within the Project while preserving FlowRef continuity and changing only the current SourceRef/path relation.", MutationClass::LocallyMutating, "projectcentral-flow", &[("project", true), ("flow_ref", true), ("expected_revision", true), ("new_path", true)]), rename_action),
        (descriptor("projectcentral.flow.lifecycle", "Set Project Flow lifecycle", "Set active, dormant, or closed lifecycle on the stable Flow identity without changing its source revision.", MutationClass::LocallyMutating, "projectcentral-flow", &[("project", true), ("flow_ref", true), ("expected_revision", true), ("lifecycle", true)]), lifecycle_action),
        (descriptor("projectcentral.flow.history", "Read Project Flow history", "Read exact stored revision receipts for one FlowRef. Current Flow remains refinable while prior bytes remain under derived owner history.", MutationClass::LocallyMutating, "projectcentral-flow-history", &[("project", true), ("flow_ref", true)]), history_action),
    ];
    for (descriptor, handler) in actions {
        registry.register(descriptor, handler).expect("Project Flow Action ids are valid");
    }
}
