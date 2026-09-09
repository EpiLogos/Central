use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::read_project_manifest;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::reconcile_project_sources;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FLOW_REGISTRY: &str = ".central/flows.json";
pub const FLOW_HISTORY_DIR: &str = ".central/flow-revisions";
pub const DEFAULT_FLOW_DIR: &str = "ProjectCentral/now/flows";
pub const ROOT_FLOW_DIR: &str = "Control/agents/now/flows";
/// The root register's identity in the canonical world-ref grammar.
pub const ROOT_WORLD_REF: &str = "control:root";

/// The register a Flow belongs to. Central root is the meta-project: it keeps a
/// NOW field of its own (`Control/agents/now`), and a Flow that is not about any
/// one project belongs there. A ProjectCentral is a specification over that same
/// shape, so both registers hold Flows through one implementation and differ
/// only in where the NOW field sits and how the register names itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowRegister {
    /// `control:root`, or the ProjectCentral manifest's project id.
    pub id: String,
    pub root_register: bool,
}

impl FlowRegister {
    /// How the register names itself in the world-ref grammar, and the scope
    /// every Flow and source ref inside it is stamped with.
    pub fn scope_ref(&self) -> String {
        if self.root_register {
            ROOT_WORLD_REF.to_owned()
        } else {
            format!("project:{}", self.id)
        }
    }
    fn flow_dir(&self) -> &'static str {
        if self.root_register { ROOT_FLOW_DIR } else { DEFAULT_FLOW_DIR }
    }
}

/// Read the register a directory is. A ProjectCentral manifest names a project
/// register; a directory holding `Control/` and `Work/` is the Central root.
pub fn register_of(register_root: &Path) -> io::Result<FlowRegister> {
    match read_project_manifest(register_root) {
        Ok(manifest) => Ok(FlowRegister { id: manifest.project_id, root_register: false }),
        Err(error) => {
            if register_root.join("Control").is_dir() && register_root.join("Work").is_dir() {
                Ok(FlowRegister { id: ROOT_WORLD_REF.to_owned(), root_register: true })
            } else {
                Err(error)
            }
        }
    }
}

/// Reconcile the Source Change Horizon of whichever register holds the Flow:
/// Central keeps a root horizon of its own beside the per-project ones.
fn reconcile_register_sources(register_root: &Path) -> io::Result<()> {
    if register_of(register_root)?.root_register {
        crate::source_horizon::reconcile_control_sources(register_root)?;
    } else {
        reconcile_project_sources(register_root)?;
    }
    Ok(())
}

/// The source bindings of whichever register holds the Flow.
fn register_source_bindings(
    register_root: &Path,
) -> io::Result<Vec<crate::source_horizon::SourceBinding>> {
    if register_of(register_root)?.root_register {
        crate::source_horizon::control_source_bindings(register_root)
    } else {
        crate::source_horizon::project_source_bindings(register_root)
    }
}
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

/// W1.3 owner read model — one Flow as the NOW field presents it.
///
/// The only civil-date evidence Central owns is the local stamp embedded in
/// the source filename (`YYYY-MM-DD-HHMM`, supplied by the creating caller).
/// Unix timestamps are never converted to civil dates here: that would be a
/// timezone guess, which the DAY law forbids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowNowEntry {
    pub flow: FlowRecord,
    /// Local civil date embedded in the filename stamp, when present.
    /// `None` is an explicit unavailable state (adopted or unconventionally
    /// named sources), never a guess.
    pub local_stamp_date: Option<String>,
    /// `live` (active), `held` (dormant) or `closed` — derived from the
    /// lifecycle the owner already records.
    pub currentness: String,
}

/// W1.3 owner read model — Flows grouped by embedded local civil date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowDayGroup {
    pub day: String,
    pub flows: Vec<FlowNowEntry>,
}

/// W1.3 owner read model — deterministic DAY facts (#138 §10).
///
/// Only facts the owner can establish without a timezone guess are reported.
/// Closure dates are not derivable (lifecycle changes carry unix provenance
/// only), so closed Flows are reported without a day claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowDayFacts {
    pub current_day: String,
    pub begun_today: Vec<FlowNowEntry>,
    pub continuing: Vec<FlowNowEntry>,
    pub closed: Vec<FlowNowEntry>,
    pub undated_live: Vec<FlowNowEntry>,
}

/// W1.3 owner read model — what a Flow page can disclose at rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowAtRestDisclosure {
    pub disclosed_by: String,
    pub available: bool,
    pub shows: Vec<String>,
}

/// W1.3 owner read model — the "while thinking" state. Central does not own
/// AgentSession binding (AIKit #122 does), so this state is an explicit
/// unavailable disclosure rather than a fabricated signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowThinkingDisclosure {
    pub available: bool,
    pub owner: String,
    pub reason: String,
}

/// W1.3 owner read model — the NOW presentation of a Project's Flows.
///
/// Several Flows may be live in one NOW; a date boundary does not close a
/// Flow; grouping by day is presentation, never semantic identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowNowView {
    pub schema: String,
    pub project_id: String,
    /// `caller-supplied` when `current_day` was given, `unavailable`
    /// otherwise. Central never derives the current civil date itself.
    pub current_day_source: String,
    pub live_flows: Vec<FlowNowEntry>,
    pub held_flows: Vec<FlowNowEntry>,
    pub closed_flows: Vec<FlowNowEntry>,
    pub day_groups: Vec<FlowDayGroup>,
    /// Flows with no embedded local civil date: an explicit ungrouped state.
    pub undated_flows: Vec<FlowNowEntry>,
    pub day_facts: Option<FlowDayFacts>,
    pub date_boundary_law: String,
    pub engagement: FlowEngagement,
    pub automatic_agent_or_model_invocation: bool,
}

/// W1.3 owner read model — rest-vs-thinking disclosure for the whole NOW
/// field. Rest state is disclosed from owner data; thinking state belongs to
/// the Agency owner and is disclosed as unavailable here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowEngagement {
    pub disclosure_ladder: Vec<String>,
    pub at_rest: FlowAtRestDisclosure,
    pub while_thinking: FlowThinkingDisclosure,
}

pub const FLOW_DATE_BOUNDARY_LAW: &str = "A local civil date boundary does not close a Flow and does not create a new identity; day grouping is presentation only. The current civil date is supplied by the caller or read from the field — never guessed from a timezone.";

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

pub(crate) fn relative_member(raw: &str) -> io::Result<PathBuf> {
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

pub(crate) fn reject_symlink_components(project_root: &Path, relative: &Path) -> io::Result<()> {
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "non-normal Flow path",
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "refusing symlink Flow path component: {}",
                        current.display()
                    ),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(crate) fn safe_source_member_path(
    project_root: &Path,
    raw: &str,
    must_exist: bool,
) -> io::Result<PathBuf> {
    let relative = relative_member(raw)?;
    reject_symlink_components(project_root, &relative)?;
    let path = project_root.join(&relative);
    if must_exist {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Flow source must be an ordinary file",
            ));
        }
        let root = fs::canonicalize(project_root)?;
        let source = fs::canonicalize(&path)?;
        if !source.starts_with(root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Flow source escaped its Project world",
            ));
        }
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        reject_symlink_components(project_root, relative.parent().unwrap_or(Path::new("")))?;
    }
    Ok(path)
}

/// The Flow revision of a byte span: the same scheme `store_revision` records,
/// exposed so read-only callers can compare a retained source against its
/// registered revision without reconciling anything.
pub(crate) fn content_revision_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len())
}

fn escaped_source_ref(register: &FlowRegister, path: &str) -> String {
    let escaped = path
        .replace('%', "%25")
        .replace(':', "%3A")
        .replace(' ', "%20");
    format!("central:source:{}:{escaped}", register.scope_ref())
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
    let register = register_of(project_root)?;
    if !register.root_register {
        let validation = read_project_manifest(project_root)?.validate();
        if !validation.valid {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                validation.errors.join("; "),
            ));
        }
    }
    let path = registry_path(project_root);
    if !path.is_file() {
        return Ok(FlowRegistry {
            schema: FLOW_REGISTRY_SCHEMA.into(),
            project_id: register.id,
            flows: vec![],
        });
    }
    let registry: FlowRegistry = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if registry.schema != FLOW_REGISTRY_SCHEMA || registry.project_id != register.id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Flow registry schema or register identity is invalid",
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

pub const MAX_FLOW_TEXT_BYTES: usize = 4 * 1024 * 1024;
fn require_flow_retrieval(_project_root: &Path, path: &Path) -> io::Result<()> {
    if !crate::source_horizon::retrieval_allowed(Path::new("/"), path) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Flow source is excluded by .no-agent-retrieval",
        ));
    }
    Ok(())
}
fn flow_bytes(project_root: &Path, relative: &str) -> io::Result<Vec<u8>> {
    let path = safe_source_member_path(project_root, relative, true)?;
    require_flow_retrieval(project_root, &path)?;
    let file = crate::file_mutation::open_native_file(project_root, relative)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow requires a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take((MAX_FLOW_TEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FLOW_TEXT_BYTES
        || bytes.contains(&0)
        || std::str::from_utf8(&bytes).is_err()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Flow requires bounded UTF-8 text without NUL (maximum 4 MiB)",
        ));
    }
    Ok(bytes)
}
pub fn inspect_flow(project_root: &Path, flow_ref: &str) -> io::Result<Value> {
    let record = load_registry(project_root)?
        .flows
        .into_iter()
        .find(|f| f.flow_ref == flow_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "FlowRef is not registered in this Project",
            )
        })?;
    let check = safe_source_member_path(project_root, &record.path, true).and_then(|p| {
        require_flow_retrieval(project_root, &p)?;
        let m = crate::file_mutation::open_native_file(project_root, &record.path)?.metadata()?;
        if !m.is_file() || m.len() > MAX_FLOW_TEXT_BYTES as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Flow source is not bounded regular text",
            ));
        }
        Ok(())
    });
    let reason = check.err().map(|e| e.to_string());
    let capability = json!({"available":reason.is_none(),"reason":reason});
    let write_reason = reason.or_else(|| {
        register_source_bindings(project_root)
            .and_then(|bindings| {
                let binding = bindings
                    .into_iter()
                    .find(|b| b.source_ref == record.source_ref)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            "Flow source is not in its native horizon",
                        )
                    })?;
                crate::world_source::enforce_write_authority(&binding, "agent", None)
            })
            .err()
            .map(|e| e.to_string())
    });
    let write = json!({"available":write_reason.is_none(),"reason":write_reason});
    Ok(
        json!({"schema":"central.project-flow-inspection/v1","flow":record,"revision_observation":"last-observed","capabilities":{"read":capability,"write":write,"history":capability},"automatic_agent_or_model_invocation":false}),
    )
}
fn reconcile_record(project_root: &Path, record: &mut FlowRecord) -> io::Result<bool> {
    let bytes = flow_bytes(project_root, &record.path)?;
    store_revision(
        project_root,
        record,
        &bytes,
        "unknown",
        "unknown-external",
        None,
    )
}

pub(crate) fn validate_actor_kind(kind: &str) -> io::Result<()> {
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
    let register = register_of(project_root)?;
    let candidate = Path::new(path);
    let human = if register.root_register {
        String::new()
    } else {
        read_project_manifest(project_root)?.human_source
    };
    let reserved: Vec<&str> = if register.root_register {
        vec![
            "Control/user",
            "Control/agents/governance",
            "Control/agents/wiki",
            "Control/agents/now/user",
            "Control/agents/now/agents",
            "Control/machines",
        ]
    } else {
        vec![
            human.as_str(),
            "ProjectCentral/agents/governance",
            "ProjectCentral/agents/wiki",
            "ProjectCentral/now/user",
            "ProjectCentral/now/agents",
        ]
    };
    if reserved
        .iter()
        .any(|root| candidate.starts_with(Path::new(root)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow source placement would overlap an existing Central authority container; retain Flow in its own or a neutral provider/domain-local source container",
        ));
    }
    Ok(())
}

fn default_path(register: &FlowRegister, local_stamp: Option<&str>, title: Option<&str>) -> io::Result<String> {
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
            let slug = crate::names::slugify(title.unwrap_or(""), 6);
            if slug.is_empty() {
                format!("{stamp}.md")
            } else {
                format!("{slug}-{stamp}.md")
            }
        }
        None => {
            // Naming law: a flow created without an explicit stamp still gets a
            // readable local civil name; nanos are never the human-facing path.
            let (date, hhmm) = crate::names::local_civil_stamp();
            let stamp = format!("{date}-{hhmm}");
            let slug = crate::names::slugify(title.unwrap_or(""), 6);
            if slug.is_empty() {
                format!("{stamp}.md")
            } else {
                format!("{slug}-{stamp}.md")
            }
        }
    };
    Ok(format!("{}/{filename}", register.flow_dir()))
}

fn ensure_unique_path(
    registry: &FlowRegistry,
    path: &str,
    except_flow: Option<&str>,
) -> io::Result<()> {
    if registry
        .flows
        .iter()
        .any(|flow| flow.path == path && Some(flow.flow_ref.as_str()) != except_flow)
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "another Flow already owns that path",
        ));
    }
    Ok(())
}

pub fn registered_flow_records(project_root: &Path) -> io::Result<Vec<FlowRecord>> {
    Ok(load_registry(project_root)?.flows)
}

/// Extract the local civil date (`YYYY-MM-DD`) from a Flow source filename
/// stamp. Returns `None` when the filename carries no stamp — an explicit
/// unavailable state, never a guess.
/// The local civil day a Flow's retained filename carries.
///
/// The naming law puts the readable slug first — `<slug>-YYYY-MM-DD-HHMM.md` —
/// so the stamp is at the END of the stem, not the start. A stem that is only
/// the stamp still reads, and a filename that carries no stamp at all is
/// honestly undated rather than mis-parsed.
pub(crate) fn embedded_local_stamp_date(path: &str) -> Option<String> {
    let stem = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())?;
    let stem = stem.strip_suffix(".md").unwrap_or(&stem);
    let date_at = |offset: usize| -> Option<String> {
        let bytes = stem.as_bytes();
        if bytes.len() < offset + 10 {
            return None;
        }
        let window = &bytes[offset..offset + 10];
        let shaped = window[4] == b'-'
            && window[7] == b'-'
            && window
                .iter()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
        if !shaped {
            return None;
        }
        let day = &stem[offset..offset + 10];
        let year: u32 = day[..4].parse().ok()?;
        if !(1..=9999).contains(&year) {
            return None;
        }
        Some(day.to_owned())
    };
    // `…-YYYY-MM-DD-HHMM` — the stamp the naming law appends.
    if stem.len() >= 15 {
        if let Some(day) = date_at(stem.len() - 15) {
            let time = &stem.as_bytes()[stem.len() - 4..];
            if stem.as_bytes()[stem.len() - 5] == b'-' && time.iter().all(u8::is_ascii_digit) {
                return Some(day);
            }
        }
    }
    // A stem that is the stamp alone.
    date_at(0)
}

fn validate_civil_day(day: &str) -> io::Result<()> {
    let bytes = day.as_bytes();
    let valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "current_day must use YYYY-MM-DD; Central never derives the civil date from a timezone",
        ));
    }
    Ok(())
}

fn currentness(lifecycle: &str) -> &'static str {
    match lifecycle {
        "active" => "live",
        "dormant" => "held",
        _ => "closed",
    }
}

fn now_entry(flow: &FlowRecord) -> FlowNowEntry {
    FlowNowEntry {
        local_stamp_date: embedded_local_stamp_date(&flow.path),
        currentness: currentness(&flow.lifecycle).to_owned(),
        flow: flow.clone(),
    }
}

/// W1.3 owner read model — the NOW presentation of a Project's Flows.
///
/// `current_day` is the local civil date the caller supplies (or `None`, in
/// which case day facts are an explicit unavailable state). Several Flows may
/// be live in one NOW; crossing a date boundary does not close a Flow.
pub fn flow_now_view(project_root: &Path, current_day: Option<&str>) -> io::Result<FlowNowView> {
    if let Some(day) = current_day {
        validate_civil_day(day)?;
    }
    let list = list_flows(project_root)?;
    let mut live_flows = Vec::new();
    let mut held_flows = Vec::new();
    let mut closed_flows = Vec::new();
    let mut undated_flows = Vec::new();
    let mut groups: std::collections::BTreeMap<String, Vec<FlowNowEntry>> =
        std::collections::BTreeMap::new();
    let mut begun_today = Vec::new();
    let mut continuing = Vec::new();
    let mut undated_live = Vec::new();
    for flow in &list.flows {
        let entry = now_entry(flow);
        match entry.currentness.as_str() {
            "live" => live_flows.push(entry.clone()),
            "held" => held_flows.push(entry.clone()),
            _ => closed_flows.push(entry.clone()),
        }
        match entry.local_stamp_date.clone() {
            Some(day) => {
                groups.entry(day.clone()).or_default().push(entry.clone());
                if current_day == Some(day.as_str()) {
                    begun_today.push(entry.clone());
                } else if entry.currentness == "live" && current_day.is_some() {
                    continuing.push(entry.clone());
                }
            }
            None => {
                undated_flows.push(entry.clone());
                if entry.currentness == "live" && current_day.is_some() {
                    undated_live.push(entry.clone());
                }
            }
        }
    }
    live_flows.sort_by(|left, right| left.flow.flow_ref.cmp(&right.flow.flow_ref));
    let day_groups: Vec<FlowDayGroup> = groups
        .into_iter()
        .rev()
        .map(|(day, mut flows)| {
            flows.sort_by(|left, right| left.flow.flow_ref.cmp(&right.flow.flow_ref));
            FlowDayGroup { day, flows }
        })
        .collect();
    let day_facts = current_day.map(|day| FlowDayFacts {
        current_day: day.to_owned(),
        begun_today,
        continuing,
        closed: closed_flows.clone(),
        undated_live,
    });
    Ok(FlowNowView {
        schema: "central.project-flow-now/v1".into(),
        project_id: list.project_id,
        current_day_source: if current_day.is_some() {
            "caller-supplied".into()
        } else {
            "unavailable".into()
        },
        live_flows,
        held_flows,
        closed_flows,
        day_groups,
        undated_flows,
        day_facts,
        date_boundary_law: FLOW_DATE_BOUNDARY_LAW.into(),
        engagement: FlowEngagement {
            disclosure_ladder: vec![
                "available".into(),
                "retrieved".into(),
                "loaded".into(),
                "disclosed".into(),
            ],
            at_rest: FlowAtRestDisclosure {
                disclosed_by: "central:projectcentral.flow".into(),
                available: true,
                shows: vec![
                    "lifecycle".into(),
                    "current_revision".into(),
                    "retrieval_availability".into(),
                    "revision_history".into(),
                ],
            },
            while_thinking: FlowThinkingDisclosure {
                available: false,
                owner: "aikit:agent-session".into(),
                reason: "Central does not own AgentSession binding; the thinking state of a Flow is disclosed by AIKit #122, never inferred by Central".into(),
            },
        },
        automatic_agent_or_model_invocation: false,
    })
}

pub fn list_flows(project_root: &Path) -> io::Result<FlowList> {
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    let mut registry = load_registry(project_root)?;
    let mut changed = false;
    for flow in &mut registry.flows {
        let path = safe_source_member_path(project_root, &flow.path, true)?;
        if require_flow_retrieval(project_root, &path).is_ok() {
            changed |= reconcile_record(project_root, flow)?;
        }
    }
    if changed {
        write_registry(project_root, &registry)?;
    }
    reconcile_register_sources(project_root)?;
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
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    validate_actor_kind(actor_kind)?;
    let mut registry = load_registry(project_root)?;
    let register = register_of(project_root)?;
    let path = match explicit_path {
        Some(path) => relative_member(path)?.to_string_lossy().replace('\\', "/"),
        None => default_path(&register, local_stamp, title.as_deref())?,
    };
    ensure_unique_path(&registry, &path, None)?;
    validate_flow_placement(project_root, &path)?;
    let source = safe_source_member_path(project_root, &path, false)?;
    if source.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Flow source already exists; use adopt for retained source",
        ));
    }
    fs::write(&source, b"")?;
    let flow_ref = format!(
        "central:flow:{}:{}",
        register.scope_ref(),
        unique_nanos()
    );
    let mut record = FlowRecord {
        flow_ref: flow_ref.clone(),
        source_ref: escaped_source_ref(&register, &path),
        path,
        created_at_unix_seconds: unix_seconds(),
        current_revision: String::new(),
        lifecycle: "active".into(),
        title,
        scope_ref: register.scope_ref(),
        privacy: "inherits-source-authority".into(),
        revisions: vec![],
    };
    seed_revision(
        project_root,
        &mut record,
        b"",
        actor,
        actor_kind,
        agent_session_ref,
    )?;
    registry.flows.push(record.clone());
    registry
        .flows
        .sort_by(|left, right| left.flow_ref.cmp(&right.flow_ref));
    write_registry(project_root, &registry)?;
    reconcile_register_sources(project_root)?;
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
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    validate_actor_kind(actor_kind)?;
    let mut registry = load_registry(project_root)?;
    let register = register_of(project_root)?;
    let path = relative_member(raw_path)?
        .to_string_lossy()
        .replace('\\', "/");
    ensure_unique_path(&registry, &path, None)?;
    validate_flow_placement(project_root, &path)?;
    let bytes = flow_bytes(project_root, &path)?;
    let flow_ref = format!(
        "central:flow:{}:{}",
        register.scope_ref(),
        unique_nanos()
    );
    let mut record = FlowRecord {
        flow_ref,
        source_ref: escaped_source_ref(&register, &path),
        path,
        created_at_unix_seconds: unix_seconds(),
        current_revision: String::new(),
        lifecycle: "active".into(),
        title,
        scope_ref: register.scope_ref(),
        privacy: "inherits-source-authority".into(),
        revisions: vec![],
    };
    seed_revision(
        project_root,
        &mut record,
        &bytes,
        actor,
        actor_kind,
        agent_session_ref,
    )?;
    registry.flows.push(record.clone());
    registry
        .flows
        .sort_by(|left, right| left.flow_ref.cmp(&right.flow_ref));
    write_registry(project_root, &registry)?;
    reconcile_register_sources(project_root)?;
    Ok(record)
}

pub fn read_flow(project_root: &Path, flow_ref: &str) -> io::Result<FlowReading> {
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    let mut registry = load_registry(project_root)?;
    let flow = registry
        .flows
        .iter_mut()
        .find(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "FlowRef is not registered in this Project",
            )
        })?;
    let reconciled = reconcile_record(project_root, flow)?;
    let content = String::from_utf8(flow_bytes(project_root, &flow.path)?)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Flow source is not UTF-8 text"))?;
    if content_revision_bytes(content.as_bytes()) != flow.current_revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Flow changed while reading its revision",
        ));
    }
    let record = flow.clone();
    if reconciled {
        write_registry(project_root, &registry)?;
    }
    reconcile_register_sources(project_root)?;
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
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    validate_actor_kind(actor_kind)?;
    if content.len() > MAX_FLOW_TEXT_BYTES || content.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow content must be bounded text without NUL",
        ));
    }
    let mut registry = load_registry(project_root)?;
    let index = registry
        .flows
        .iter()
        .position(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "FlowRef is not registered in this Project",
            )
        })?;
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
    crate::world_source::validate_attribution(actor_kind, agent_session_ref.as_deref())?;
    let binding = register_source_bindings(project_root)?
        .into_iter()
        .find(|b| b.source_ref == registry.flows[index].source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Flow is not in its native source horizon",
            )
        })?;
    crate::world_source::enforce_write_authority(
        &binding,
        actor_kind,
        agent_session_ref.as_deref(),
    )?;
    let _source = safe_source_member_path(project_root, &registry.flows[index].path, true)?;
    crate::source_safety::replace(
        project_root,
        &registry.flows[index].path,
        expected_revision,
        content,
    )?;
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
    reconcile_register_sources(project_root)?;
    Ok(record)
}

pub fn rename_flow(
    project_root: &Path,
    flow_ref: &str,
    expected_revision: &str,
    new_path: &str,
) -> io::Result<FlowRecord> {
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    let mut registry = load_registry(project_root)?;
    let index = registry
        .flows
        .iter()
        .position(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "FlowRef is not registered in this Project",
            )
        })?;
    if reconcile_record(project_root, &mut registry.flows[index])? {
        write_registry(project_root, &registry)?;
    }
    if registry.flows[index].current_revision != expected_revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Flow revision conflict before rename",
        ));
    }
    let normalized = relative_member(new_path)?
        .to_string_lossy()
        .replace('\\', "/");
    ensure_unique_path(&registry, &normalized, Some(flow_ref))?;
    validate_flow_placement(project_root, &normalized)?;
    let destination = safe_source_member_path(project_root, &normalized, false)?;
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "rename destination already exists",
        ));
    }
    let old = safe_source_member_path(project_root, &registry.flows[index].path, true)?;
    fs::rename(old, &destination)?;
    registry.flows[index].path = normalized.clone();
    registry.flows[index].source_ref = escaped_source_ref(&register_of(project_root)?, &normalized);
    write_registry(project_root, &registry)?;
    let record = registry.flows[index].clone();
    reconcile_register_sources(project_root)?;
    Ok(record)
}

pub fn set_flow_lifecycle(
    project_root: &Path,
    flow_ref: &str,
    expected_revision: &str,
    lifecycle: &str,
) -> io::Result<FlowRecord> {
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    validate_lifecycle(lifecycle)?;
    let mut registry = load_registry(project_root)?;
    let index = registry
        .flows
        .iter()
        .position(|flow| flow.flow_ref == flow_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "FlowRef is not registered in this Project",
            )
        })?;
    if reconcile_record(project_root, &mut registry.flows[index])? {
        write_registry(project_root, &registry)?;
    }
    if registry.flows[index].current_revision != expected_revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Flow revision conflict before lifecycle change",
        ));
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
    let _source_lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    let mut registry = load_registry(project_root)?;
    let mut changed = false;
    for flow in &mut registry.flows {
        let path = safe_source_member_path(project_root, &flow.path, true)?;
        if require_flow_retrieval(project_root, &path).is_ok() {
            changed |= reconcile_record(project_root, flow)?;
        }
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
        let source = safe_source_member_path(project_root, &flow.path, true)?;
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
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires {field}."),
                None,
            )
        })
}

fn optional(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The register an action addresses. `project` names a Work project; its
/// absence names the Central root, the meta-project that keeps its own NOW
/// field and that every ProjectCentral specifies over.
fn project_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let Some(project) = optional(input, "project") else {
        let root = resolve_central_root(context.root_options)
            .map_err(|message| {
                ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
            })?
            .path;
        register_of(&root).map_err(|error| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidCentralStructure,
                format!("Central root is not a usable Flow register: {error}"),
                None,
            )
        })?;
        return Ok(root);
    };
    let project = relative_member(&project).map_err(|error| {
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
    crate::projectcentral_flow::reject_symlink_components(&root, &Path::new("Work").join(&project))
        .map_err(|e| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                e.to_string(),
                None,
            )
        })?;
    let project_root = root.join("Work").join(project);
    if !project_root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", project_root.display()),
            None,
        ));
    }
    read_project_manifest(&project_root).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidCentralStructure,
            error.to_string(),
            None,
        )
    })?;
    Ok(project_root)
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::AlreadyExists => ResultStatus::VerificationFailure,
        io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
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
        inputs: inputs
            .iter()
            .map(|(name, required)| input(name, *required))
            .collect(),
        output: ActionOutputDefinition {
            output_type: output_type.into(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn inspect_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.inspect";
    let root = match project_context(action, input, context) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let flow = match required(input, "flow_ref", action) {
        Ok(r) => r,
        Err(e) => return e,
    };
    inspect_flow(&root, &flow)
        .map(|v| ActionResult::success(action, v))
        .unwrap_or_else(|e| io_failure(action, e))
}
fn list_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.list";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    list_flows(&root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("Flow list serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.read";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let flow_ref = match required(input, "flow_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    read_flow(&root, &flow_ref)
        .and_then(|reading| {
            if input
                .get("expected_revision")
                .is_some_and(|v| v.as_str() != Some(reading.flow.current_revision.as_str()))
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Flow read revision conflict",
                ));
            }
            Ok(reading)
        })
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("Flow reading serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn create_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.create";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let actor = match required(input, "actor", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let actor_kind = match required(input, "actor_kind", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    create_flow(
        &root,
        optional(input, "local_stamp").as_deref(),
        optional(input, "path").as_deref(),
        optional(input, "title"),
        &actor,
        &actor_kind,
        optional(input, "agent_session_ref"),
    )
    .map(|flow| {
        ActionResult::success(
            action,
            json!({"flow": flow, "automatic_agent_or_model_invocation": false}),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

fn adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.adopt";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let path = match required(input, "path", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let actor = match required(input, "actor", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let actor_kind = match required(input, "actor_kind", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    adopt_flow(
        &root,
        &path,
        optional(input, "title"),
        &actor,
        &actor_kind,
        optional(input, "agent_session_ref"),
    )
    .map(|flow| {
        ActionResult::success(
            action,
            json!({"flow": flow, "automatic_agent_or_model_invocation": false}),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

fn write_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.write";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let flow_ref = match required(input, "flow_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let expected_revision = match required(input, "expected_revision", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let content = input
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let actor = match required(input, "actor", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let actor_kind = match required(input, "actor_kind", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    write_flow(
        &root,
        &flow_ref,
        &expected_revision,
        &content,
        &actor,
        &actor_kind,
        optional(input, "agent_session_ref"),
    )
    .map(|flow| {
        ActionResult::success(
            action,
            json!({"flow": flow, "automatic_agent_or_model_invocation": false}),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

fn rename_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.rename";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let flow_ref = match required(input, "flow_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let expected_revision = match required(input, "expected_revision", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let new_path = match required(input, "new_path", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    rename_flow(&root, &flow_ref, &expected_revision, &new_path)
        .map(|flow| {
            ActionResult::success(
                action,
                json!({"flow": flow, "automatic_agent_or_model_invocation": false}),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn lifecycle_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.lifecycle";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let flow_ref = match required(input, "flow_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let expected_revision = match required(input, "expected_revision", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let lifecycle = match required(input, "lifecycle", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    set_flow_lifecycle(&root, &flow_ref, &expected_revision, &lifecycle)
        .map(|flow| {
            ActionResult::success(
                action,
                json!({"flow": flow, "automatic_agent_or_model_invocation": false}),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn now_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.now";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let current_day = optional(input, "current_day");
    if let Some(day) = current_day.as_deref() {
        if let Err(error) = validate_civil_day(day) {
            return io_failure(action, error);
        }
    }
    flow_now_view(&root, current_day.as_deref())
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("Flow NOW view serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn history_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.flow.history";
    let root = match project_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let flow_ref = match required(input, "flow_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match read_flow(&root, &flow_ref) {
        Ok(reading) => ActionResult::success(
            action,
            json!({
                "flow_ref": reading.flow.flow_ref,
                "current_revision": reading.flow.current_revision,
                "revisions": reading.flow.revisions,
                "automatic_agent_or_model_invocation": false,
            }),
        ),
        Err(error) => io_failure(action, error),
    }
}

pub fn register_projectcentral_flow_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor(
                "projectcentral.flow.inspect",
                "Inspect Project Flow",
                "Read native Flow descriptor and last-observed revision with retrieval availability; no source body is returned.",
                MutationClass::ReadOnly,
                "projectcentral-flow-inspection",
                &[("project", false), ("flow_ref", true)],
            ),
            inspect_action
                as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.flow.list",
                "List Project Flows",
                "List stable Flow identities and current source/revision/lifecycle state. Reconciles external file edits into revision provenance and Source Change Horizon without invoking an Agent/model.",
                MutationClass::LocallyMutating,
                "projectcentral-flow-list",
                &[("project", false)],
            ),
            list_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.flow.read",
                "Read Project Flow",
                "Read the current ordinary Flow source by stable FlowRef and reconcile any external editor revision with actor unknown.",
                MutationClass::LocallyMutating,
                "projectcentral-flow-reading",
                &[
                    ("project", false),
                    ("flow_ref", true),
                    ("expected_revision", false),
                ],
            ),
            read_action,
        ),
        (
            descriptor(
                "projectcentral.flow.create",
                "Create Project Flow",
                "Create a blank ordinary-file Flow with stable FlowRef. ProjectCentral/now/flows/YYYY-MM-DD-HHMM.md is the default convention when local_stamp is supplied; path is not identity.",
                MutationClass::LocallyMutating,
                "projectcentral-flow",
                &[
                    ("project", false),
                    ("actor", true),
                    ("actor_kind", true),
                    ("local_stamp", false),
                    ("path", false),
                    ("title", false),
                    ("agent_session_ref", false),
                ],
            ),
            create_action,
        ),
        (
            descriptor(
                "projectcentral.flow.adopt",
                "Adopt retained source as Flow",
                "Give an existing ordinary Project file a stable FlowRef without moving it, preserving provider/domain-local placement.",
                MutationClass::LocallyMutating,
                "projectcentral-flow",
                &[
                    ("project", false),
                    ("path", true),
                    ("actor", true),
                    ("actor_kind", true),
                    ("title", false),
                    ("agent_session_ref", false),
                ],
            ),
            adopt_action,
        ),
        (
            descriptor(
                "projectcentral.flow.write",
                "Write Project Flow revision",
                "Revision-safe canonical whole-file write shared by human and Agent callers. A stale expected_revision returns an explicit conflict.",
                MutationClass::LocallyMutating,
                "projectcentral-flow",
                &[
                    ("project", false),
                    ("flow_ref", true),
                    ("expected_revision", true),
                    ("content", false),
                    ("actor", true),
                    ("actor_kind", true),
                    ("agent_session_ref", false),
                ],
            ),
            write_action,
        ),
        (
            descriptor(
                "projectcentral.flow.rename",
                "Rename Project Flow source",
                "Move the retained ordinary file within the Project while preserving FlowRef continuity and changing only the current SourceRef/path relation.",
                MutationClass::LocallyMutating,
                "projectcentral-flow",
                &[
                    ("project", false),
                    ("flow_ref", true),
                    ("expected_revision", true),
                    ("new_path", true),
                ],
            ),
            rename_action,
        ),
        (
            descriptor(
                "projectcentral.flow.lifecycle",
                "Set Project Flow lifecycle",
                "Set active, dormant, or closed lifecycle on the stable Flow identity without changing its source revision.",
                MutationClass::LocallyMutating,
                "projectcentral-flow",
                &[
                    ("project", false),
                    ("flow_ref", true),
                    ("expected_revision", true),
                    ("lifecycle", true),
                ],
            ),
            lifecycle_action,
        ),
        (
            descriptor(
                "projectcentral.flow.history",
                "Read Project Flow history",
                "Read exact stored revision receipts for one FlowRef. Current Flow remains refinable while prior bytes remain under derived owner history.",
                MutationClass::LocallyMutating,
                "projectcentral-flow-history",
                &[("project", false), ("flow_ref", true)],
            ),
            history_action,
        ),
        (
            descriptor(
                "projectcentral.flow.now",
                "Read Project Flow NOW view",
                "W1.3 owner read model: live/held/closed Flows, day grouping by embedded local civil filename stamp, caller-supplied current_day DAY facts, and rest-vs-thinking disclosure. A date boundary does not close a Flow; the civil date is caller-supplied, never timezone-derived. No Agent/model invocation.",
                MutationClass::LocallyMutating,
                "projectcentral-flow-now",
                &[("project", false), ("current_day", false)],
            ),
            now_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("Project Flow Action ids are valid");
    }
}
