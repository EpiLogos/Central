//! Owner Actions for opening and revising live World source.
//!
//! The Source Change Horizon already gives every participating Project source a
//! stable `SourceRef` and a deterministic content revision, and it deliberately
//! exposes no source payloads. These Actions complete that seam without turning
//! the Horizon into a payload carrier: a caller opens one named source with its
//! exact revision, revises it under compare-and-swap, and the emitted horizon
//! change carries the declared actor. Nothing here reads `.central` state as
//! truth, bypasses the Horizon, or invokes an Agent or model.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::read_project_manifest;
use crate::projectcentral_flow::{
    relative_member, safe_source_member_path, validate_actor_kind,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{
    read_project_change_horizon, reconcile_project_source_writes, SourceBinding, SourceRevision,
    SourceWriteAttribution,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub const WORLD_SOURCE_READING_SCHEMA: &str = "central.project-world-source-reading/v1";
pub const WORLD_SOURCE_WRITE_RECEIPT_SCHEMA: &str = "central.project-world-source-write-receipt/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorldSourceReading {
    pub schema: String,
    pub world_ref: String,
    pub source: SourceBinding,
    pub revision: SourceRevision,
    pub content: String,
    pub content_encoding: String,
    pub automatic_agent_or_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorldSourceWriteReceipt {
    pub schema: String,
    pub world_ref: String,
    pub source: SourceBinding,
    pub previous_revision: String,
    pub revision: SourceRevision,
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_ref: Option<String>,
    pub actor: String,
    pub actor_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    pub automatic_agent_or_model_invocation: bool,
}

fn require_retrieval(binding: &SourceBinding) -> io::Result<()> {
    if !binding.agent_retrieval_allowed {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "this source is excluded from disclosure by its .no-agent-retrieval treatment and is therefore neither read nor written through this Action",
        ));
    }
    Ok(())
}

fn recognised_human_source(binding: &SourceBinding) -> bool {
    matches!(binding.provenance.as_str(), "human-authored" | "human-adopted")
}

fn authored_human_ground(binding: &SourceBinding) -> bool {
    recognised_human_source(binding)
        || binding.roles.iter().any(|role| {
            role == "agent-governance-source" || role == "project-human-source-aperture"
        })
}

/// Attribution is declared by the caller, and a declaration has to be coherent:
/// human authorship does not happen inside an agent session, so a write that
/// declares both is refusing to say what it is and is recorded as nothing.
pub(crate) fn validate_attribution(actor_kind: &str, agent_session_ref: Option<&str>) -> io::Result<()> {
    if actor_kind == "human" && agent_session_ref.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a write that declares actor_kind human does not also carry an agent_session_ref; a caller declaring both is not attributable as human authorship, so nothing is written",
        ));
    }
    Ok(())
}

/// Human-authored ground keeps human authorship. A declared non-human caller —
/// and any write carrying an agent session — may propose a change and have it
/// recognised; it does not revise that source in place. The provenance and role
/// recognisers are machine-checked from the Project's ground relations; what
/// this gate guarantees is the refusal of declared agents and agent sessions,
/// not of an unattested bare self-declaration of human authorship.
pub(crate) fn enforce_write_authority(
    binding: &SourceBinding,
    actor_kind: &str,
    agent_session_ref: Option<&str>,
) -> io::Result<()> {
    let declared_human = actor_kind == "human" && agent_session_ref.is_none();
    if declared_human || !authored_human_ground(binding) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "source {} is authored human ground (provenance {}, roles {:?}); a declared non-human caller and any agent-session write propose rather than write it, and the human authorship or an accepted relation is what changes it",
            binding.source_ref, binding.provenance, binding.roles
        ),
    ))
}

pub fn read_world_source(project_root: &Path, source_ref: &str) -> io::Result<WorldSourceReading> {
    let horizon = read_project_change_horizon(project_root, None)?;
    let observed = horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "source_ref is not a participating World source of this Project",
            )
        })?;
    require_retrieval(&observed.binding)?;
    let _path = safe_source_member_path(project_root, &observed.binding.path, true)?;
    let content=crate::source_safety::read(project_root,&observed.binding.path)?;
    if crate::projectcentral_flow::content_revision_bytes(content.as_bytes())!=observed.revision.revision {return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Source changed while reading its revision"));}
    Ok(WorldSourceReading {
        schema: WORLD_SOURCE_READING_SCHEMA.to_owned(),
        world_ref: horizon.world_ref,
        source: observed.binding.clone(),
        revision: observed.revision.clone(),
        content,
        content_encoding: "utf-8".to_owned(),
        automatic_agent_or_model_invocation: false,
    })
}

pub fn write_world_source(
    project_root: &Path,
    source_ref: &str,
    expected_revision: &str,
    content: &str,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<WorldSourceWriteReceipt> {
    let _lock=crate::source_safety::lock(project_root,"source-mutation.lock")?;
    validate_actor_kind(actor_kind)?;
    validate_attribution(actor_kind, agent_session_ref.as_deref())?;
    if expected_revision.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected_revision is required for a World source write",
        ));
    }
    // Reading the horizon first reconciles derived state so the compare-and-swap
    // basis and the emitted change come from the same reconciliation.
    let horizon = read_project_change_horizon(project_root, None)?;
    let basis = horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "source_ref is not a participating World source of this Project",
            )
        })?;
    let binding = basis.binding.clone();
    let previous_revision = basis.revision.revision.clone();
    require_retrieval(&binding)?;
    enforce_write_authority(&binding, actor_kind, agent_session_ref.as_deref())?;

    if previous_revision != expected_revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "World source revision conflict: expected {expected_revision}, current {previous_revision}"
            ),
        ));
    }

    let _path = safe_source_member_path(project_root, &binding.path, true)?;
    crate::source_safety::replace(project_root,&binding.path,expected_revision,content)?;

    let mut attributions = BTreeMap::new();
    attributions.insert(
        source_ref.to_owned(),
        SourceWriteAttribution {
            actor: actor.to_owned(),
            actor_kind: actor_kind.to_owned(),
            agent_session_ref: agent_session_ref.clone(),
        },
    );
    let report = reconcile_project_source_writes(project_root, &attributions)?;
    let observed = report
        .horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "written World source left its Project horizon",
            )
        })?;
    let change = report
        .new_changes
        .iter()
        .find(|change| change.source_ref == source_ref);
    Ok(WorldSourceWriteReceipt {
        schema: WORLD_SOURCE_WRITE_RECEIPT_SCHEMA.to_owned(),
        world_ref: report.horizon.world_ref,
        source: binding,
        previous_revision,
        revision: observed.revision.clone(),
        changed: change.is_some(),
        change_ref: change.map(|change| change.change_ref.clone()),
        actor: actor.to_owned(),
        actor_kind: actor_kind.to_owned(),
        agent_session_ref,
        automatic_agent_or_model_invocation: false,
    })
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

fn project_root(action: &str, input: &Value, context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    let project = relative_member(&project).map_err(|error| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
    })?;
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    crate::projectcentral_flow::reject_symlink_components(&root,&Path::new("Work").join(&project)).map_err(|e|ActionResult::failure(Some(action),ResultStatus::InvalidInput,e.to_string(),None))?;
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
        io::ErrorKind::AlreadyExists => ResultStatus::InvalidInput,
        io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn read_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.source.read";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let source_ref = match required(input, "source_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    read_world_source(&root, &source_ref)
        .map(|value| ActionResult::success(action, serde_json::to_value(value).expect("World source reading serialises")))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn write_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.source.write";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let source_ref = match required(input, "source_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let expected_revision = match required(input, "expected_revision", action) {
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
    let content = input.get("content").and_then(Value::as_str).unwrap_or("");
    write_world_source(
        &root,
        &source_ref,
        &expected_revision,
        content,
        &actor,
        &actor_kind,
        optional(input, "agent_session_ref"),
    )
    .map(|value| {
        ActionResult::success(
            action,
            json!({
                "receipt": serde_json::to_value(value).expect("World source write receipt serialises"),
                "automatic_agent_or_model_invocation": false,
            }),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

fn text_input(name: &str, required: bool) -> ActionInputDefinition {
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
    output_type: &str,
    inputs: &[(&str, bool)],
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs: inputs
            .iter()
            .map(|(name, required)| text_input(name, *required))
            .collect(),
        output: ActionOutputDefinition { output_type: output_type.to_owned() },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability { available: true, reason: None },
    }
}

pub fn register_world_source_actions(registry: &mut ActionRegistry) {
    crate::source_return::register(registry);
    let actions = [
        (
            descriptor(
                "projectcentral.source.read",
                "Read live World source",
                "Read one participating Project World source by SourceRef with its exact content revision, provenance, standing and treatment. Reconciles the Source Change Horizon (derived .central state only) and never invokes an Agent or model. Sources excluded by .no-agent-retrieval are not disclosed here; masking is not missing.",
                MutationClass::LocallyMutating,
                "projectcentral-world-source-reading",
                &[("project", true), ("source_ref", true)],
            ),
            read_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.source.write",
                "Write live World source revision",
                "Revision-safe canonical whole-file write on one participating Project World source: a stale expected_revision fails without mutating, and the emitted Source Change Horizon change carries the declared actor, actor_kind and optional agent_session_ref. Attribution is declared, not proven: a write declaring actor_kind human never carries an agent_session_ref, and recognised human-authored or human-adopted sources, human-source aperture material and agent-governance sources refuse declared non-human callers and refuse every agent-session write — those callers propose instead of writing. Provenance and role recognisers are machine-checked from the Project's ground relations; a bare self-declaration of human authorship is recorded verbatim as declared. Never invokes an Agent or model.",
                MutationClass::LocallyMutating,
                "projectcentral-world-source-write-receipt",
                &[
                    ("project", true),
                    ("source_ref", true),
                    ("expected_revision", true),
                    ("content", false),
                    ("actor", true),
                    ("actor_kind", true),
                    ("agent_session_ref", false),
                ],
            ),
            write_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("World source Action ids are valid");
    }
}
