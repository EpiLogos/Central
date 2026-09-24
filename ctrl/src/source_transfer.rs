//! Scoped source transfer between grounds that share one world identity.
//!
//! The Source Change Horizon already gives every participating source a
//! world-relative `SourceRef` and a deterministic content revision, so
//! divergence between two grounds is detectable by content alone. These
//! Actions complete the missing receive/apply path of that seam: a caller
//! exports an explicitly scoped, explicitly directed bundle from one ground,
//! moves the bundle as an ordinary file, and applies it on the other ground,
//! where every entry either fast-forwards from its recorded base, is already
//! present, or produces an explicit recorded conflict. Nothing here copies
//! machine identities, credentials, absolute paths or generated runtime
//! state: a bundle names worlds, world-relative paths and content revisions,
//! nothing else. No merge is performed, no transfer is automatic, and a
//! transfer never deletes: removal of authored ground is an owner act on
//! each ground, never a side effect of a sync. The operator moves every
//! bundle, and a divergent source is resolved only by an explicit, recorded
//! resolution.
//!
//! Authority stays with each ground's own law: entries are validated against
//! the receiving ground's bindings with the same write-authority rule as
//! `projectcentral.source.write` before anything is mutated, and sources
//! excluded by `.no-agent-retrieval` are neither exported nor applied. No
//! Agent or model is invoked by anything here.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{read_project_manifest, AGENT_GOVERNANCE_DIR, WIKI_DIR};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{
    read_project_change_horizon, reconcile_project_source_writes, retrieval_allowed, SourceBinding,
    SourceWriteAttribution,
};
use crate::source_safety::{
    content_revision_bytes, lock, read, relative_member, safe_source_member_path,
    validate_actor_kind, MAX_SOURCE,
};
use crate::world_source::{enforce_write_authority, validate_attribution, write_world_source};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SOURCE_TRANSFER_SCHEMA: &str = "central.source-transfer/v1";
pub const SOURCE_TRANSFER_APPLY_RECEIPT_SCHEMA: &str = "central.source-transfer-apply-receipt/v1";
pub const SOURCE_TRANSFER_CONFLICT_SCHEMA: &str = "central.source-transfer-conflict/v1";
pub const SOURCE_TRANSFER_RESOLVE_RECEIPT_SCHEMA: &str =
    "central.source-transfer-resolve-receipt/v1";

const TRANSFER_RECORD_AREA: &str = ".central/source-transfer/records";
const TRANSFER_CONFLICT_AREA: &str = ".central/source-transfer/conflicts";
const MAX_BUNDLE: usize = 32 * 1024 * 1024;
const MAX_SCOPE_REFS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDirection {
    pub from_world_ref: String,
    pub to_world_ref: String,
    /// Operator-declared ground labels. Two grounds of one shared world
    /// carry the same world ref, so the direction between them is stated in
    /// human-chosen names - never in machine identities, which are not
    /// portable authored ground. The labels travel as provenance; the
    /// receiving ground verifies only the world identity.
    pub from_ground: String,
    pub to_ground: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_cursor: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferAttribution {
    pub actor: String,
    pub actor_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferEntry {
    pub source_ref: String,
    pub path: String,
    /// `added` (created within the transferred range), `modified` (changed
    /// within the range), or `state` (current content with no retained
    /// origin history: the precondition is unestablished and the source is
    /// only created on this ground when the caller explicitly acknowledges
    /// the lineage).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub origin_change_refs: Vec<String>,
    pub origin_cursor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTransferBundle {
    pub schema: String,
    pub direction: TransferDirection,
    pub scope: TransferScope,
    pub origin_cursor: u64,
    pub created_at_unix_seconds: u64,
    pub exported_by: TransferAttribution,
    pub payloads_included: bool,
    pub sources: Vec<TransferEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferOutcome {
    pub source_ref: String,
    pub path: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferApplyReceipt {
    pub schema: String,
    pub transfer_ref: String,
    pub world_ref: String,
    pub bundle_revision: String,
    pub direction: TransferDirection,
    pub scope: TransferScope,
    pub origin_cursor: u64,
    pub applied_at_unix_seconds: u64,
    pub actor: String,
    pub actor_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    /// `applied` (every entry landed or was already present), `conflicted`
    /// (at least one explicit conflict was recorded), or `uncertain` (a
    /// mutation did not confirm; inspect the record and conflict areas).
    pub status: String,
    pub outcomes: Vec<TransferOutcome>,
    pub applied_count: usize,
    pub already_applied_count: usize,
    pub conflicted_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_write_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferConflictRecord {
    pub schema: String,
    pub conflict_ref: String,
    pub world_ref: String,
    pub source_ref: String,
    pub path: String,
    pub origin_world_ref: String,
    pub origin_cursor: u64,
    pub kind: String,
    pub base_revision: Option<String>,
    pub incoming_revision: Option<String>,
    pub local_revision: Option<String>,
    /// `open` until an explicit resolution lands; `resolved` afterwards. A
    /// resolved record is retained evidence and is never silently replaced.
    pub status: String,
    pub occurrences: u64,
    pub first_seen_unix_seconds: u64,
    pub last_seen_unix_seconds: u64,
    pub resolution_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Value>,
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn invalid(message: impl AsRef<str>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.as_ref())
}

fn conflict(message: impl AsRef<str>) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, message.as_ref())
}

/// Resolves the project root and its world identity. `project` must stay a
/// plain Work member; the manifest must read, because a transfer is always
/// world-addressed.
fn project_world(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<(PathBuf, String), ActionResult> {
    let project = input
        .get("project")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires project."),
                None,
            )
        })?;
    let member = relative_member(project).map_err(|error| {
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
    crate::source_safety::reject_symlink_components(&root, &Path::new("Work").join(&member))
        .map_err(|error| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                error.to_string(),
                None,
            )
        })?;
    let candidate = root.join("Work").join(&member);
    if !candidate.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", candidate.display()),
            None,
        ));
    }
    let manifest = read_project_manifest(&candidate).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidCentralStructure,
            error.to_string(),
            None,
        )
    })?;
    Ok((candidate, format!("project:{}", manifest.project_id)))
}

fn required_text(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
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

fn optional_text(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn attribution(action: &str, input: &Value) -> Result<TransferAttribution, ActionResult> {
    let actor = required_text(input, "actor", action)?;
    let actor_kind = required_text(input, "actor_kind", action)?;
    let agent_session_ref = optional_text(input, "agent_session_ref");
    validate_actor_kind(&actor_kind).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    validate_attribution(&actor_kind, agent_session_ref.as_deref()).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    Ok(TransferAttribution {
        actor,
        actor_kind,
        agent_session_ref,
    })
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists => {
            ResultStatus::InvalidInput
        }
        io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn write_record_file(root: &Path, relative: &str, value: &Value) -> io::Result<()> {
    if let Some(parent) = Path::new(relative).parent() {
        fs::create_dir_all(root.join(parent))?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    crate::file_mutation::atomic_record(&root.join(relative), &bytes)
}

fn read_record_value(root: &Path, relative: &str) -> io::Result<Value> {
    let raw = read(root, relative)?;
    serde_json::from_str(&raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn bundle_digest(value: &Value) -> io::Result<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(content_revision_bytes(&bytes))
}

fn tail(revision: &str) -> String {
    let tail = revision.rsplit(':').next().unwrap_or("0000000000000000");
    tail[..tail.len().min(16)].to_owned()
}

// --- Export ---

fn source_refs_input(input: &Value, action: &str) -> Result<Vec<String>, ActionResult> {
    let mut refs = Vec::new();
    match input.get("source_refs") {
        Some(Value::Array(values)) => {
            for value in values {
                match value.as_str().map(str::trim).filter(|v| !v.is_empty()) {
                    Some(reference) => refs.push(reference.to_owned()),
                    None => {
                        return Err(ActionResult::failure(
                            Some(action),
                            ResultStatus::InvalidInput,
                            "source_refs must be non-empty strings".to_owned(),
                            None,
                        ))
                    }
                }
            }
        }
        Some(Value::String(value)) => refs.push(value.trim().to_owned()),
        _ => {
            return Err(ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires source_refs (a non-empty array)."),
                None,
            ))
        }
    }
    if refs.is_empty() || refs.len() > MAX_SCOPE_REFS {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("source_refs must carry 1..{MAX_SCOPE_REFS} refs."),
            None,
        ));
    }
    let unique: BTreeSet<String> = refs.iter().cloned().collect();
    Ok(unique.into_iter().collect())
}

fn export_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.transfer.export";
    let (root, world_ref) = match project_world(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let to_world_ref = match required_text(input, "to_world_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let from_ground = match required_text(input, "from_ground", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let to_ground = match required_text(input, "to_ground", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let requested_refs = match source_refs_input(input, action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let exported_by = match attribution(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let since_cursor = input.get("since_cursor").and_then(Value::as_u64);

    let horizon = match read_project_change_horizon(&root, None) {
        Ok(value) => value,
        Err(error) => return io_failure(action, error),
    };

    let mut entries = Vec::new();
    for reference in &requested_refs {
        let observed = match horizon
            .sources
            .iter()
            .find(|source| &source.binding.source_ref == reference)
        {
            Some(value) => value,
            None => {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!(
                    "source_ref is not a participating World source of this Project: {reference}"
                ),
                    None,
                )
            }
        };
        if !observed.binding.agent_retrieval_allowed {
            return ActionResult::failure(
                Some(action),
                ResultStatus::UnavailableCapability,
                format!(
                    "source {reference} is excluded from disclosure by its .no-agent-retrieval treatment and is therefore not exported; masking is not missing"
                ),
                None,
            );
        }
        if !retrieval_allowed(&root, &root.join(&observed.binding.path)) {
            return ActionResult::failure(
                Some(action),
                ResultStatus::UnavailableCapability,
                format!("source {reference} is masked on this ground and is not exported"),
                None,
            );
        }
        let history: Vec<crate::source_horizon::SourceChange> = horizon
            .changes
            .iter()
            .filter(|change| {
                change.cursor > since_cursor.unwrap_or(0) && &change.source_ref == reference
            })
            .cloned()
            .collect();
        let current = observed.revision.revision.clone();
        let (kind, base_revision) = match history.first() {
            Some(earliest) => {
                let base = earliest.before_revision.clone();
                let kind = if base.is_none() { "added" } else { "modified" };
                (kind.to_owned(), base)
            }
            None => ("state".to_owned(), None),
        };
        let content = match read(&root, &observed.binding.path) {
            Ok(value) => Some(value),
            Err(error) => return io_failure(action, error),
        };
        if let Some(content) = &content {
            if content_revision_bytes(content.as_bytes()) != current {
                return io_failure(
                    action,
                    conflict(format!(
                        "source {reference} changed while its payload was read"
                    )),
                );
            }
        }
        entries.push(TransferEntry {
            source_ref: reference.clone(),
            path: observed.binding.path.clone(),
            kind,
            base_revision,
            after_revision: Some(current),
            content,
            origin_change_refs: history
                .iter()
                .map(|change| change.change_ref.clone())
                .collect(),
            origin_cursor: history
                .last()
                .map(|change| change.cursor)
                .unwrap_or(horizon.cursor),
        });
    }

    let bundle = SourceTransferBundle {
        schema: SOURCE_TRANSFER_SCHEMA.to_owned(),
        direction: TransferDirection {
            from_world_ref: world_ref,
            to_world_ref,
            from_ground,
            to_ground,
        },
        scope: TransferScope {
            source_refs: requested_refs,
            since_cursor,
        },
        origin_cursor: horizon.cursor,
        created_at_unix_seconds: unix_seconds(),
        exported_by,
        payloads_included: entries.iter().any(|entry| entry.content.is_some()),
        sources: entries,
    };
    let value = match serde_json::to_value(&bundle) {
        Ok(value) => value,
        Err(error) => return io_failure(action, io::Error::new(io::ErrorKind::InvalidData, error)),
    };
    let bytes = match serde_json::to_vec(&value) {
        Ok(bytes) => bytes,
        Err(error) => return io_failure(action, io::Error::new(io::ErrorKind::InvalidData, error)),
    };
    if bytes.len() > MAX_BUNDLE {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("transfer bundle exceeds the bounded {MAX_BUNDLE} byte size"),
            None,
        );
    }
    ActionResult::success(action, value)
}

// --- Apply ---

fn load_bundle(action: &str, input: &Value) -> Result<(Value, String), ActionResult> {
    let value = match (input.get("bundle"), input.get("bundle_file")) {
        (Some(bundle), None) => bundle.clone(),
        (None, Some(path)) => {
            let raw_path = path
                .as_str()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    ActionResult::failure(
                        Some(action),
                        ResultStatus::InvalidInput,
                        "bundle_file requires a path".to_owned(),
                        None,
                    )
                })?;
            let metadata = fs::metadata(raw_path).map_err(|error| {
                ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("bundle_file is not readable: {error}"),
                    None,
                )
            })?;
            if !metadata.is_file() {
                return Err(ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    "bundle_file must name a regular file".to_owned(),
                    None,
                ));
            }
            if metadata.len() as usize > MAX_BUNDLE {
                return Err(ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("transfer bundle exceeds the bounded {MAX_BUNDLE} byte size"),
                    None,
                ));
            }
            let bytes = fs::read(raw_path).map_err(|error| {
                ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("bundle_file is not readable: {error}"),
                    None,
                )
            })?;
            serde_json::from_slice(&bytes).map_err(|error| {
                ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("bundle_file is not a JSON transfer bundle: {error}"),
                    None,
                )
            })?
        }
        (Some(_), Some(_)) => {
            return Err(ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                "supply either bundle or bundle_file, not both".to_owned(),
                None,
            ))
        }
        (None, None) => {
            return Err(ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires bundle (object) or bundle_file (path)."),
                None,
            ))
        }
    };
    let digest = bundle_digest(&value).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    Ok((value, digest))
}

fn parsed_bundle(action: &str, value: &Value) -> Result<SourceTransferBundle, ActionResult> {
    let bundle: SourceTransferBundle = serde_json::from_value(value.clone()).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("bundle is not a {SOURCE_TRANSFER_SCHEMA} document: {error}"),
            None,
        )
    })?;
    if bundle.schema != SOURCE_TRANSFER_SCHEMA {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("bundle schema is not {SOURCE_TRANSFER_SCHEMA}"),
            None,
        ));
    }
    Ok(bundle)
}

/// A transferred path may only land where the receiving ground's own binding
/// law gives it a home: the human source aperture, Agent governance or the
/// Agent Wiki trees. The receiving ground's own roles apply — the origin's
/// binding travels as provenance, never as authority.
fn would_be_binding(project_root: &Path, path: &str) -> io::Result<Option<SourceBinding>> {
    let member_of = |dir: &str| path.starts_with(&format!("{dir}/"));
    let manifest = read_project_manifest(project_root)?;
    let (role, provenance, treatment) = if member_of(&manifest.human_source) {
        (
            "project-human-source-aperture",
            "unresolved",
            "projectcentral-user",
        )
    } else if member_of(AGENT_GOVERNANCE_DIR) {
        (
            "agent-governance-source",
            "unresolved",
            "projectcentral-agent-governance",
        )
    } else if member_of(WIKI_DIR) {
        (
            "agent-wiki-source",
            "agent-maintained",
            "projectcentral-agent-wiki",
        )
    } else {
        return Ok(None);
    };
    Ok(Some(SourceBinding {
        source_ref: crate::source_horizon::source_ref(
            &format!("project:{}", manifest.project_id),
            path,
        ),
        path: path.to_owned(),
        roles: vec![role.to_owned()],
        provenance: provenance.to_owned(),
        standing: "unspecified".to_owned(),
        treatment: treatment.to_owned(),
        agent_retrieval_allowed: true,
    }))
}

fn conflict_stem(
    path: &str,
    base: Option<&String>,
    incoming: Option<&String>,
    local: Option<&String>,
) -> String {
    let path_key: String = path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let material = format!(
        "{}\u{0}{}\u{0}{}",
        base.unwrap_or(&String::from("~")),
        incoming.unwrap_or(&String::from("~")),
        local.unwrap_or(&String::from("~"))
    );
    format!(
        "{path_key}-{}",
        tail(&content_revision_bytes(material.as_bytes()))
    )
}

const RESOLUTION_PATH_TEXT: &str = "resolve through projectcentral.source.transfer.resolve with disposition keep-local or accept-incoming; accept-incoming requires expected_local_revision equal to the recorded local_revision";

fn record_conflict(
    root: &Path,
    world_ref: &str,
    bundle: &SourceTransferBundle,
    entry: &TransferEntry,
    local_revision: Option<String>,
    now: u64,
) -> io::Result<String> {
    let stem = conflict_stem(
        &entry.path,
        entry.base_revision.as_ref(),
        entry.after_revision.as_ref(),
        local_revision.as_ref(),
    );
    let mut stem = stem;
    let relative = format!("{TRANSFER_CONFLICT_AREA}/{stem}/record.json");
    if let Ok(previous) = read_record_value(root, &relative) {
        if previous.get("status").and_then(Value::as_str) == Some("resolved") {
            // A resolved record is retained evidence; a fresh divergence
            // against the same facts becomes its own timestamped record.
            stem = format!("{stem}-{now}");
        }
    }
    let record = TransferConflictRecord {
        schema: SOURCE_TRANSFER_CONFLICT_SCHEMA.to_owned(),
        conflict_ref: format!("central:transfer-conflict:{world_ref}:{stem}"),
        world_ref: world_ref.to_owned(),
        source_ref: entry.source_ref.clone(),
        path: entry.path.clone(),
        origin_world_ref: bundle.direction.from_world_ref.clone(),
        origin_cursor: entry.origin_cursor,
        kind: entry.kind.clone(),
        base_revision: entry.base_revision.clone(),
        incoming_revision: entry.after_revision.clone(),
        local_revision: local_revision.clone(),
        status: "open".to_owned(),
        occurrences: 1,
        first_seen_unix_seconds: now,
        last_seen_unix_seconds: now,
        resolution_path: RESOLUTION_PATH_TEXT.to_owned(),
        resolution: None,
    };
    let mut value = serde_json::to_value(&record)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let relative = format!("{TRANSFER_CONFLICT_AREA}/{stem}/record.json");
    if read_record_value(root, &relative).is_ok() {
        // Same divergence seen again: refresh the evidence instead of
        // resetting it.
        if let Some(previous_occurrences) = read_record_value(root, &relative)
            .ok()
            .and_then(|previous| previous.get("occurrences").and_then(Value::as_u64))
        {
            value["occurrences"] = json!(previous_occurrences + 1);
        }
        value["first_seen_unix_seconds"] = read_record_value(root, &relative)
            .ok()
            .and_then(|previous| previous.get("first_seen_unix_seconds").cloned())
            .unwrap_or(json!(now));
    }
    write_record_file(root, &relative, &value)?;
    // Both sides of the divergence are snapshotted byte-exactly: a
    // resolution may adopt the incoming content, and the record must keep
    // what this ground held so nothing is overwritten without a trace.
    if let Some(content) = &entry.content {
        write_record_file(
            root,
            &format!("{TRANSFER_CONFLICT_AREA}/{stem}/incoming-source.txt"),
            &json!({ "content": content }),
        )?;
    }
    if local_revision.is_some() {
        let local_content = read(root, &entry.path)?;
        write_record_file(
            root,
            &format!("{TRANSFER_CONFLICT_AREA}/{stem}/local-source.txt"),
            &json!({ "content": local_content }),
        )?;
    }
    Ok(record.conflict_ref)
}

fn create_source(
    root: &Path,
    path: &str,
    content: &str,
    source_ref: &str,
    actor: &TransferAttribution,
) -> io::Result<()> {
    let _mutation = lock(root, "source-mutation.lock")?;
    safe_source_member_path(root, path, false)?;
    if safe_source_member_path(root, path, true).is_ok() {
        return Err(conflict(format!(
            "source {source_ref} appeared while it was being created"
        )));
    }
    let relative = Path::new(path);
    let parent = crate::file_mutation::directory(
        root,
        relative
            .parent()
            .ok_or_else(|| invalid("Missing source parent"))?,
    )?;
    let name = format!(
        ".central-source-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    let mut staged = crate::file_mutation::create_in(&parent, &name, 0o644)?;
    let result = (|| {
        staged.write_all(content.as_bytes())?;
        staged.sync_all()?;
        crate::file_mutation::rename_in(
            &parent,
            &name,
            relative
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| invalid("Invalid source filename"))?,
        )
    })();
    let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    unsafe {
        libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0);
    }
    result?;
    let mut attributions = BTreeMap::new();
    attributions.insert(
        source_ref.to_owned(),
        SourceWriteAttribution {
            actor: actor.actor.clone(),
            actor_kind: actor.actor_kind.clone(),
            agent_session_ref: actor.agent_session_ref.clone(),
        },
    );
    let report = reconcile_project_source_writes(root, &attributions)?;
    if !report
        .horizon
        .sources
        .iter()
        .any(|source| source.binding.source_ref == source_ref)
    {
        return Err(invalid(format!(
            "transferred source {source_ref} did not become a participating World source of this ground"
        )));
    }
    Ok(())
}

fn apply_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.transfer.apply";
    let (root, world_ref) = match project_world(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let actor = match attribution(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let (bundle_value, bundle_revision) = match load_bundle(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let bundle = match parsed_bundle(action, &bundle_value) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let acknowledged: BTreeSet<String> = input
        .get("accept_unestablished_lineage")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let horizon = match read_project_change_horizon(&root, None) {
        Ok(value) => value,
        Err(error) => return io_failure(action, error),
    };
    if bundle.direction.to_world_ref != world_ref {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "this transfer is directed to ground {} of world {}, not to this ground of world {}",
                bundle.direction.to_ground, bundle.direction.to_world_ref, world_ref
            ),
            None,
        );
    }

    // Validate every entry against this ground before mutating anything: a
    // refusal leaves the receiving ground byte-identical.
    let mut prepared: Vec<(&TransferEntry, Option<String>)> = Vec::new();
    for entry in &bundle.sources {
        let expected_ref = crate::source_horizon::source_ref(&world_ref, &entry.path);
        if entry.source_ref != expected_ref {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!(
                    "entry source_ref {} does not name path {} on this ground ({})",
                    entry.source_ref, entry.path, expected_ref
                ),
                None,
            );
        }
        if entry.kind != "added" && entry.kind != "modified" && entry.kind != "state" {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!(
                    "entry {} carries unsupported kind {:?}; transfers carry added, modified or state entries and never delete",
                    entry.source_ref, entry.kind
                ),
                None,
            );
        }
        if let Some(content) = &entry.content {
            if content.len() > MAX_SOURCE {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("entry {} exceeds bounded source size", entry.source_ref),
                    None,
                );
            }
            let hash = content_revision_bytes(content.as_bytes());
            if entry.after_revision.as_deref() != Some(hash.as_str()) {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::VerificationFailure,
                    format!(
                        "entry {} payload does not hash to its recorded revision ({hash} vs {:?})",
                        entry.source_ref, entry.after_revision
                    ),
                    None,
                );
            }
        } else {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!(
                    "entry {} carries no payload; transfers never delete",
                    entry.source_ref
                ),
                None,
            );
        }
        let local = horizon
            .sources
            .iter()
            .find(|source| source.binding.source_ref == entry.source_ref);
        let local_revision = local.map(|source| source.revision.revision.clone());
        if let Some(local) = local {
            if !local.binding.agent_retrieval_allowed {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::UnavailableCapability,
                    format!(
                        "source {} is excluded from disclosure on this ground by its .no-agent-retrieval treatment and is therefore not applied",
                        entry.source_ref
                    ),
                    None,
                );
            }
            if let Err(error) = enforce_write_authority(
                &local.binding,
                &actor.actor_kind,
                actor.agent_session_ref.as_deref(),
            ) {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::UnavailableCapability,
                    format!(
                        "entry {} refused by this ground's write authority: {error}",
                        entry.source_ref
                    ),
                    None,
                );
            }
        } else {
            match would_be_binding(&root, &entry.path) {
                Ok(Some(binding)) => {
                    if !retrieval_allowed(&root, &root.join(&entry.path)) {
                        return ActionResult::failure(
                            Some(action),
                            ResultStatus::UnavailableCapability,
                            format!(
                                "path {} is masked on this ground by a .no-agent-retrieval treatment and is not applied",
                                entry.path
                            ),
                            None,
                        );
                    }
                    if let Err(error) = enforce_write_authority(
                        &binding,
                        &actor.actor_kind,
                        actor.agent_session_ref.as_deref(),
                    ) {
                        return ActionResult::failure(
                            Some(action),
                            ResultStatus::UnavailableCapability,
                            format!(
                                "entry {} refused by this ground's write authority: {error}",
                                entry.source_ref
                            ),
                            None,
                        );
                    }
                }
                Ok(None) => {
                    return ActionResult::failure(
                        Some(action),
                        ResultStatus::InvalidInput,
                        format!(
                            "path {} is not a participating source location of this ground; transfers land only where the receiving ground's own binding law gives them a home",
                            entry.path
                        ),
                        None,
                    )
                }
                Err(error) => return io_failure(action, error),
            }
        }
        prepared.push((entry, local_revision));
    }

    let now = unix_seconds();
    let mut outcomes: Vec<TransferOutcome> = Vec::new();
    let mut status = "applied".to_owned();
    let mut last_error = None;
    for (entry, local_revision) in &prepared {
        if status == "uncertain" {
            break;
        }
        let after = entry
            .after_revision
            .clone()
            .unwrap_or_else(|| String::from("~"));
        let content = entry.content.clone().unwrap_or_default();
        let outcome = if let Some(local) = &local_revision {
            if local == &after {
                TransferOutcome {
                    source_ref: entry.source_ref.clone(),
                    path: entry.path.clone(),
                    outcome: "already-applied".to_owned(),
                    revision: Some(after.clone()),
                    conflict_ref: None,
                }
            } else if entry.base_revision.as_deref() == Some(local.as_str()) {
                match write_world_source(
                    &root,
                    &entry.source_ref,
                    local,
                    &content,
                    &actor.actor,
                    &actor.actor_kind,
                    actor.agent_session_ref.clone(),
                ) {
                    Ok(receipt) => TransferOutcome {
                        source_ref: entry.source_ref.clone(),
                        path: entry.path.clone(),
                        outcome: "applied".to_owned(),
                        revision: Some(receipt.revision.revision),
                        conflict_ref: None,
                    },
                    Err(error) => {
                        status = "uncertain".to_owned();
                        last_error = Some(error.to_string());
                        break;
                    }
                }
            } else {
                let conflict_ref = match record_conflict(
                    &root,
                    &world_ref,
                    &bundle,
                    entry,
                    Some(local.clone()),
                    now,
                ) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        status = "uncertain".to_owned();
                        last_error = Some(error.to_string());
                        break;
                    }
                };
                TransferOutcome {
                    source_ref: entry.source_ref.clone(),
                    path: entry.path.clone(),
                    outcome: "conflicted".to_owned(),
                    revision: Some(local.clone()),
                    conflict_ref,
                }
            }
        } else if entry.kind == "added" || acknowledged.contains(&entry.source_ref) {
            // An origin creation within the transferred range fast-forwards
            // onto an absent ground; any other absent-local entry lands only
            // when the caller explicitly acknowledges the lineage.
            match create_source(&root, &entry.path, &content, &entry.source_ref, &actor) {
                Ok(()) => TransferOutcome {
                    source_ref: entry.source_ref.clone(),
                    path: entry.path.clone(),
                    outcome: if entry.kind == "added" {
                        "applied-added".to_owned()
                    } else {
                        "established".to_owned()
                    },
                    revision: entry.after_revision.clone(),
                    conflict_ref: None,
                },
                Err(error) => {
                    status = "uncertain".to_owned();
                    last_error = Some(error.to_string());
                    break;
                }
            }
        } else {
            // Modified or unacknowledged state on an absent local source:
            // the origin's base was never established here, which is
            // divergence like any other.
            let conflict_ref = match record_conflict(&root, &world_ref, &bundle, entry, None, now) {
                Ok(value) => Some(value),
                Err(error) => {
                    status = "uncertain".to_owned();
                    last_error = Some(error.to_string());
                    break;
                }
            };
            TransferOutcome {
                source_ref: entry.source_ref.clone(),
                path: entry.path.clone(),
                outcome: "conflicted".to_owned(),
                revision: None,
                conflict_ref,
            }
        };
        if outcome.outcome == "conflicted" && status == "applied" {
            status = "conflicted".to_owned();
        }
        outcomes.push(outcome);
    }

    let applied_count = outcomes
        .iter()
        .filter(|o| o.outcome.starts_with("applied") || o.outcome == "established")
        .count();
    let already_applied_count = outcomes
        .iter()
        .filter(|o| o.outcome == "already-applied")
        .count();
    let conflicted_count = outcomes
        .iter()
        .filter(|o| o.outcome == "conflicted")
        .count();
    let transfer_ref = format!("central:transfer:{}:{}", world_ref, tail(&bundle_revision));
    let receipt = TransferApplyReceipt {
        schema: SOURCE_TRANSFER_APPLY_RECEIPT_SCHEMA.to_owned(),
        transfer_ref: transfer_ref.clone(),
        world_ref,
        bundle_revision,
        direction: bundle.direction.clone(),
        scope: bundle.scope.clone(),
        origin_cursor: bundle.origin_cursor,
        applied_at_unix_seconds: now,
        actor: actor.actor.clone(),
        actor_kind: actor.actor_kind.clone(),
        agent_session_ref: actor.agent_session_ref.clone(),
        status: status.clone(),
        outcomes,
        applied_count,
        already_applied_count,
        conflicted_count,
        last_error,
        record_write_error: None,
    };
    let mut receipt_value = serde_json::to_value(&receipt)
        .unwrap_or_else(|_| json!({"schema": SOURCE_TRANSFER_APPLY_RECEIPT_SCHEMA}));
    let record_relative = format!("{TRANSFER_RECORD_AREA}/{}.json", tail(&transfer_ref));
    if let Err(error) = write_record_file(&root, &record_relative, &receipt_value) {
        // The mutations already happened; the receipt is returned and its
        // recording failure is named instead of hidden.
        receipt_value["record_write_error"] = json!(error.to_string());
    }
    ActionResult::success(action, receipt_value)
}

// --- Conflict surfacing and resolution ---

fn conflicts_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.transfer.conflicts";
    let (root, world_ref) = match project_world(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let status_filter = optional_text(input, "status");
    if let Some(status) = &status_filter {
        if status != "open" && status != "resolved" {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                "status filter is open, resolved, or absent".to_owned(),
                None,
            );
        }
    }
    let entries = match fs::read_dir(root.join(TRANSFER_CONFLICT_AREA)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return ActionResult::success(
                action,
                json!({
                    "schema": SOURCE_TRANSFER_CONFLICT_SCHEMA,
                    "world_ref": world_ref,
                    "open_conflicts": 0,
                    "conflicts": [],
                    "automatic_agent_or_model_invocation": false,
                }),
            )
        }
        Err(error) => return io_failure(action, error),
    };
    let mut conflicts = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return io_failure(action, error),
        };
        let record_path = entry.path().join("record.json");
        if !record_path.is_file() {
            continue;
        }
        let Ok(relative) = record_path.strip_prefix(&root) else {
            continue;
        };
        let relative = relative.to_string_lossy().into_owned();
        let record = match read_record_value(&root, &relative) {
            Ok(value) => value,
            Err(error) => return io_failure(action, error),
        };
        if record.get("schema").and_then(Value::as_str) != Some(SOURCE_TRANSFER_CONFLICT_SCHEMA) {
            continue;
        }
        if let Some(filter) = &status_filter {
            if record.get("status").and_then(Value::as_str) != Some(filter.as_str()) {
                continue;
            }
        }
        conflicts.push(record);
    }
    conflicts.sort_by(|left, right| {
        let key = |value: &Value| {
            (
                value
                    .get("source_ref")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                value
                    .get("conflict_ref")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            )
        };
        key(left).cmp(&key(right))
    });
    let open_conflicts = conflicts
        .iter()
        .filter(|c| c.get("status").and_then(Value::as_str) == Some("open"))
        .count();
    ActionResult::success(
        action,
        json!({
            "schema": SOURCE_TRANSFER_CONFLICT_SCHEMA,
            "world_ref": world_ref,
            "open_conflicts": open_conflicts,
            "conflicts": conflicts,
            "automatic_agent_or_model_invocation": false,
        }),
    )
}

fn resolve_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.transfer.resolve";
    let (root, world_ref) = match project_world(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source_ref = match required_text(input, "source_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let disposition = match required_text(input, "disposition", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if disposition != "keep-local" && disposition != "accept-incoming" {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "disposition is keep-local or accept-incoming".to_owned(),
            None,
        );
    }
    let actor = match attribution(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let horizon = match read_project_change_horizon(&root, None) {
        Ok(value) => value,
        Err(error) => return io_failure(action, error),
    };
    let entries = match fs::read_dir(root.join(TRANSFER_CONFLICT_AREA)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("no open transfer conflict is recorded for {source_ref}"),
                None,
            );
        }
        Err(error) => return io_failure(action, error),
    };
    let mut open: Vec<(String, Value)> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return io_failure(action, error),
        };
        let record_path = entry.path().join("record.json");
        if !record_path.is_file() {
            continue;
        }
        let Ok(relative) = record_path.strip_prefix(&root) else {
            continue;
        };
        let relative = relative.to_string_lossy().into_owned();
        let record = match read_record_value(&root, &relative) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if record.get("schema").and_then(Value::as_str) != Some(SOURCE_TRANSFER_CONFLICT_SCHEMA) {
            continue;
        }
        if record.get("source_ref").and_then(Value::as_str) == Some(source_ref.as_str())
            && record.get("status").and_then(Value::as_str) == Some("open")
        {
            open.push((relative, record));
        }
    }
    if open.is_empty() {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("no open transfer conflict is recorded for {source_ref}"),
            None,
        );
    }
    if open.len() > 1 {
        let refs = open
            .iter()
            .filter_map(|(_, record)| record.get("conflict_ref").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "multiple open transfer conflicts are recorded for {source_ref}; each divergence is resolved at its own record: {refs}"
            ),
            None,
        );
    }
    let (relative, mut record) = open.remove(0);
    let now = unix_seconds();
    let stem_dir = Path::new(&relative)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut resolution = json!({
        "disposition": disposition,
        "actor": actor.actor,
        "actor_kind": actor.actor_kind,
        "agent_session_ref": actor.agent_session_ref,
        "resolved_at_unix_seconds": now,
    });
    if disposition == "keep-local" {
        record["status"] = json!("resolved");
        record["resolution"] = resolution;
        if let Err(error) = write_record_file(&root, &relative, &record) {
            return io_failure(action, error);
        }
        return ActionResult::success(
            action,
            json!({
                "schema": SOURCE_TRANSFER_RESOLVE_RECEIPT_SCHEMA,
                "world_ref": world_ref,
                "source_ref": source_ref,
                "disposition": "keep-local",
                "conflict_ref": record.get("conflict_ref"),
                "retained_revision": record.get("local_revision"),
                "automatic_agent_or_model_invocation": false,
            }),
        );
    }
    let expected_local_revision = match required_text(input, "expected_local_revision", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let recorded_local = record
        .get("local_revision")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if recorded_local.as_deref() != Some(expected_local_revision.as_str()) {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "expected_local_revision does not match the recorded divergence basis (recorded local {:?})",
                recorded_local
            ),
            None,
        );
    }
    let Some(local) = horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
    else {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("source {source_ref} is not a participating World source of this ground"),
            None,
        );
    };
    if local.revision.revision != expected_local_revision {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "the source moved since the conflict was recorded (current {}, recorded {}); the next apply re-surfaces the divergence",
                local.revision.revision, expected_local_revision
            ),
            None,
        );
    }
    let incoming_revision = record
        .get("incoming_revision")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let snapshot = match read_record_value(&root, &format!("{stem_dir}/incoming-source.txt")) {
        Ok(value) => value,
        Err(error) => return io_failure(action, error),
    };
    let content = snapshot
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let hash = content_revision_bytes(content.as_bytes());
    if hash != incoming_revision {
        return io_failure(
            action,
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the recorded incoming snapshot does not hash to the recorded incoming revision ({hash} vs {incoming_revision})"
                ),
            ),
        );
    }
    if let Err(error) = enforce_write_authority(
        &local.binding,
        &actor.actor_kind,
        actor.agent_session_ref.as_deref(),
    ) {
        return ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            format!("resolution refused by this ground's write authority: {error}"),
            None,
        );
    }
    let receipt = match write_world_source(
        &root,
        &source_ref,
        &expected_local_revision,
        &content,
        &actor.actor,
        &actor.actor_kind,
        actor.agent_session_ref.clone(),
    ) {
        Ok(receipt) => receipt,
        Err(error) => return io_failure(action, error),
    };
    record["status"] = json!("resolved");
    resolution["resolved_revision"] = json!(receipt.revision.revision);
    record["resolution"] = resolution;
    if let Err(error) = write_record_file(&root, &relative, &record) {
        return io_failure(action, error);
    }
    ActionResult::success(
        action,
        json!({
            "schema": SOURCE_TRANSFER_RESOLVE_RECEIPT_SCHEMA,
            "world_ref": world_ref,
            "source_ref": source_ref,
            "disposition": "accept-incoming",
            "conflict_ref": record.get("conflict_ref"),
            "revision": receipt.revision.revision,
            "change_ref": receipt.change_ref,
            "automatic_agent_or_model_invocation": false,
        }),
    )
}

// --- Registration ---

fn text_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn array_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "array".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

#[allow(clippy::too_many_lines)]
pub fn register_source_transfer_actions(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    let actions: Vec<(ActionDescriptor, Handler)> = vec![
        (
            descriptor_read(
                "projectcentral.source.transfer.export",
                "Export scoped source transfer bundle",
                "Export an explicitly scoped, explicitly directed source transfer bundle from one ground of a world to another: the caller names the source refs, the destination world, the declared from/to ground labels (human-chosen names - machine identities are not portable authored ground), an optional horizon cursor window, and the declared exporter attribution. The bundle carries world-relative paths and content revisions only - never machine identities, credentials, absolute paths or generated runtime state - and refuses sources masked by .no-agent-retrieval. The bundle moves as an ordinary file; nothing is sent automatically. Never invokes an Agent or model.",
                vec![
                    text_input("project", true),
                    array_input("source_refs", true),
                    text_input("to_world_ref", true),
                    text_input("from_ground", true),
                    text_input("to_ground", true),
                    text_input("since_cursor", false),
                    text_input("actor", true),
                    text_input("actor_kind", true),
                    text_input("agent_session_ref", false),
                ],
                "central-source-transfer",
            ),
            export_action,
        ),
        (
            descriptor_mutation(
                "projectcentral.source.transfer.apply",
                "Apply a source transfer bundle",
                "Apply an exported bundle on the ground its direction names: every entry either fast-forwards from its recorded base revision through the ordinary compare-and-swap write, is already present, or produces an explicit recorded conflict naming both revisions - a divergent source is never overwritten and a transfer never deletes. Entries are validated against this ground's write authority, masking and participation law before anything is mutated, and payload hashes are verified against their recorded revisions. Unestablished lineage is created only for sources the caller explicitly acknowledges. The receipt is recorded under .central/source-transfer. Never invokes an Agent or model.",
                vec![
                    text_input("project", true),
                    text_input("actor", true),
                    text_input("actor_kind", true),
                    text_input("agent_session_ref", false),
                    text_input("bundle_file", false),
                    text_input("accept_unestablished_lineage", false),
                    text_input("bundle", false),
                ],
                "central-source-transfer-apply-receipt",
            ),
            apply_action,
        ),
        (
            descriptor_read(
                "projectcentral.source.transfer.conflicts",
                "Read recorded transfer conflicts",
                "Read the transfer conflict records of this ground: each names the source, the origin ground and cursor, the base, incoming and local content revisions, and the resolution path. Both sides of a divergence are snapshotted beside the record, so resolving never destroys evidence. Never invokes an Agent or model.",
                vec![text_input("project", true), text_input("status", false)],
                "central-source-transfer-conflicts",
            ),
            conflicts_action,
        ),
        (
            descriptor_mutation(
                "projectcentral.source.transfer.resolve",
                "Resolve a recorded transfer conflict",
                "Resolve one recorded transfer conflict explicitly: keep-local retains the receiving ground's content and records the decision; accept-incoming writes the snapshotted incoming content through the ordinary compare-and-swap write and requires expected_local_revision to equal the exact revision the conflict recorded - a source that moved since the conflict is refused and re-surfaced by the next apply. This ground's write authority governs every resolution. Never invokes an Agent or model.",
                vec![
                    text_input("project", true),
                    text_input("source_ref", true),
                    text_input("disposition", true),
                    text_input("expected_local_revision", false),
                    text_input("actor", true),
                    text_input("actor_kind", true),
                    text_input("agent_session_ref", false),
                ],
                "central-source-transfer-resolve-receipt",
            ),
            resolve_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("Source Transfer Action ids are valid");
    }
}

fn descriptor_read(
    id: &str,
    title: &str,
    description: &str,
    inputs: Vec<ActionInputDefinition>,
    output_type: &str,
) -> ActionDescriptor {
    build_descriptor(
        id,
        title,
        description,
        MutationClass::ReadOnly,
        inputs,
        output_type,
    )
}

fn descriptor_mutation(
    id: &str,
    title: &str,
    description: &str,
    inputs: Vec<ActionInputDefinition>,
    output_type: &str,
) -> ActionDescriptor {
    build_descriptor(
        id,
        title,
        description,
        MutationClass::LocallyMutating,
        inputs,
        output_type,
    )
}

fn build_descriptor(
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
        output: ActionOutputDefinition {
            output_type: output_type.to_owned(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}
