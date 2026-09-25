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
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{
    project_source_bindings, read_project_change_horizon, reconcile_control_source_writes,
    reconcile_control_sources, reconcile_project_source_writes, retrieval_allowed,
    source_ref as horizon_source_ref, SourceBinding, SourceRevision, SourceWriteAttribution,
};
use crate::source_safety::{
    content_revision_bytes, reject_symlink_components, relative_member, safe_source_member_path,
    validate_actor_kind,
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
    matches!(
        binding.provenance.as_str(),
        "human-authored" | "human-adopted"
    )
}

fn authored_human_ground(binding: &SourceBinding) -> bool {
    recognised_human_source(binding)
        || binding.roles.iter().any(|role| {
            role == "agent-governance-source"
                || role == "project-human-source-aperture"
                || role == "personal-human-source-aperture"
        })
}

/// Attribution is declared by the caller, and a declaration has to be coherent:
/// human authorship does not happen inside an agent session, so a write that
/// declares both is refusing to say what it is and is recorded as nothing.
pub(crate) fn validate_attribution(
    actor_kind: &str,
    agent_session_ref: Option<&str>,
) -> io::Result<()> {
    if actor_kind == "human" && agent_session_ref.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a write that declares actor_kind human does not also carry an agent_session_ref; a caller declaring both is not attributable as human authorship, so nothing is written",
        ));
    }
    Ok(())
}

/// A source with native temporal, document or authority ownership: only its
/// authenticated owner operation changes it, never a generic whole-file write
/// and never a source transfer.
pub(crate) fn natively_owned(binding: &SourceBinding) -> bool {
    binding.roles.iter().any(|role| {
        matches!(
            role.as_str(),
            "protected-contribution-document"
                | "human-day"
                | "now-clearing"
                | "work-placement-policy"
                | "civil-time-policy"
                | "native-action-authority"
        )
    })
}

/// Legacy declared attribution remains explicit for existing source clients.
/// Native Day/contribution documents require their authenticated owner operation;
/// a bare actor_kind=human never bypasses document-local contribution protection.
pub(crate) fn enforce_write_authority(
    binding: &SourceBinding,
    actor_kind: &str,
    agent_session_ref: Option<&str>,
) -> io::Result<()> {
    if natively_owned(binding) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "this source has native temporal/document/authority ownership; use its authenticated owner operation or explicit source review, not a generic declared-human whole-file write",
        ));
    }
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
    read_scoped_source(project_root, source_ref, false)
}

/// The root meta-Project uses its existing Control horizon. No ProjectCentral
/// manifest, adoption, copied source, or path-derived SourceRef is needed.
pub fn read_control_world_source(root: &Path, source_ref: &str) -> io::Result<WorldSourceReading> {
    read_scoped_source(root, source_ref, true)
}

fn read_scoped_source(
    project_root: &Path,
    source_ref: &str,
    root_register: bool,
) -> io::Result<WorldSourceReading> {
    let horizon = if root_register {
        reconcile_control_sources(project_root)?.horizon
    } else {
        read_project_change_horizon(project_root, None)?
    };
    let observed = horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "source_ref is not a participating source of the requested World",
            )
        })?;
    require_retrieval(&observed.binding)?;
    let _path = safe_source_member_path(project_root, &observed.binding.path, true)?;
    let content = crate::source_safety::read(project_root, &observed.binding.path)?;
    if crate::source_safety::content_revision_bytes(content.as_bytes())
        != observed.revision.revision
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Source changed while reading its revision",
        ));
    }
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
    write_scoped_source(
        project_root,
        source_ref,
        expected_revision,
        content,
        actor,
        actor_kind,
        agent_session_ref,
        false,
    )
}

pub fn write_control_world_source(
    root: &Path,
    source_ref: &str,
    expected_revision: &str,
    content: &str,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<WorldSourceWriteReceipt> {
    write_scoped_source(
        root,
        source_ref,
        expected_revision,
        content,
        actor,
        actor_kind,
        agent_session_ref,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_scoped_source(
    project_root: &Path,
    source_ref: &str,
    expected_revision: &str,
    content: &str,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
    root_register: bool,
) -> io::Result<WorldSourceWriteReceipt> {
    let _lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    validate_actor_kind(actor_kind)?;
    validate_attribution(actor_kind, agent_session_ref.as_deref())?;
    if expected_revision.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected_revision is required for a World source write",
        ));
    }
    let horizon = if root_register {
        reconcile_control_sources(project_root)?.horizon
    } else {
        read_project_change_horizon(project_root, None)?
    };
    let basis = horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "source_ref is not a participating source of the requested World",
            )
        })?;
    let binding = basis.binding.clone();
    let previous_revision = basis.revision.revision.clone();
    require_retrieval(&binding)?;
    enforce_write_authority(&binding, actor_kind, agent_session_ref.as_deref())?;
    if previous_revision != expected_revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("World source revision conflict: expected {expected_revision}, current {previous_revision}"),
        ));
    }
    let _path = safe_source_member_path(project_root, &binding.path, true)?;
    crate::source_safety::replace(project_root, &binding.path, expected_revision, content)?;
    let mut attributions = BTreeMap::new();
    attributions.insert(
        source_ref.to_owned(),
        SourceWriteAttribution {
            actor: actor.to_owned(),
            actor_kind: actor_kind.to_owned(),
            agent_session_ref: agent_session_ref.clone(),
        },
    );
    let report = if root_register {
        reconcile_control_source_writes(project_root, &attributions)?
    } else {
        reconcile_project_source_writes(project_root, &attributions)?
    };
    let observed = report
        .horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "written World source left its requested horizon",
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
fn project_root(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
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
    crate::source_safety::reject_symlink_components(&root, &Path::new("Work").join(&project))
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
/// Absent/null is the explicit root scope; malformed or empty project input
/// is not absence. The legacy action spelling remains wire compatible.
fn source_scope(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<(PathBuf, bool), ActionResult> {
    if input.get("project").is_none_or(Value::is_null) {
        let root = resolve_central_root(context.root_options)
            .map_err(|message| {
                ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
            })?
            .path;
        let scope = crate::continuous_work::source::Scope::resolve(&root, None)
            .map_err(|error| io_failure(action, error))?;
        Ok((scope.root, true))
    } else {
        project_root(action, input, context).map(|root| (root, false))
    }
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
fn read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.read";
    let (root, root_register) = match source_scope(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let source_ref = match required(input, "source_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    read_scoped_source(&root, &source_ref, root_register)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("World source reading serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}
fn write_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.write";
    let (root, root_register) = match source_scope(action, input, context) {
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
    write_scoped_source(&root,&source_ref,&expected_revision,content,&actor,&actor_kind,optional(input, "agent_session_ref"),root_register)
        .map(|value| ActionResult::success(action,json!({"receipt":serde_json::to_value(value).expect("World source write receipt serialises"),"automatic_agent_or_model_invocation":false})))
        .unwrap_or_else(|error| io_failure(action, error))
}
/// Create one absent document in a Project's own human ground — the door
/// behind the desktop's "Write it" for a project vision page or mockup.
///
/// The aperture treatment is the only standing this door grants: the created
/// file joins the World source horizon as unresolved
/// project-human-source-aperture material, every later revision goes through
/// `projectcentral.source.write` compare-and-swap, and a declared non-human
/// caller is refused exactly as any aperture write is. Atomic no-overwrite
/// admission mirrors `central.files.create`; the recorded change lands in the
/// source horizon (Added, carrying the declared attribution), never in an
/// ordinary file history. The root register has no door here: Control ground
/// creates through `central.files.create` under `Control/user/flows`.
#[allow(clippy::too_many_arguments)]
fn create_scoped_source(
    project_root: &Path,
    path: &str,
    content: &str,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
) -> io::Result<WorldSourceWriteReceipt> {
    use std::ffi::CString;
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::AsRawFd;

    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "project manifest is invalid: {}",
                validation.errors.join("; ")
            ),
        ));
    }
    validate_actor_kind(actor_kind)?;
    validate_attribution(actor_kind, agent_session_ref.as_deref())?;
    if content.len() > crate::source_safety::MAX_SOURCE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source exceeds bounded text constraints",
        ));
    }
    if content.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source content must be UTF-8 text without NUL",
        ));
    }
    let relative = relative_member(path)?;
    reject_symlink_components(project_root, &relative)?;
    let within = relative
        .strip_prefix(Path::new(&manifest.human_source))
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "this door creates documents only inside the Project human ground ({})",
                    manifest.human_source
                ),
            )
        })?;
    if within.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a document path below the human ground is required",
        ));
    }
    if within.components().count() > 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a created human-ground document sits at most one directory deep; its parent must already exist",
        ));
    }
    let relative_str = relative
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "path must be UTF-8"))?;
    if !retrieval_allowed(project_root, &project_root.join(&relative)) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "this location is excluded by .no-agent-retrieval and is therefore neither read, written nor created through this Action",
        ));
    }
    let world_ref = format!("project:{}", manifest.project_id);
    let reference = horizon_source_ref(&world_ref, relative_str);
    // Authority is decided on the aperture standing this door grants —
    // before anything exists.
    let aperture = SourceBinding {
        source_ref: reference.clone(),
        path: relative_str.to_owned(),
        roles: vec!["project-human-source-aperture".to_owned()],
        provenance: "unresolved".to_owned(),
        standing: "unspecified".to_owned(),
        treatment: "projectcentral-user".to_owned(),
        agent_retrieval_allowed: true,
    };
    enforce_write_authority(&aperture, actor_kind, agent_session_ref.as_deref())?;

    let _lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;
    // Under the owner lock: the parent must already be a native directory,
    // the destination must still be absent, and both must hold at link time.
    let parent_relative = relative
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?
        .to_path_buf();
    let parent = crate::file_mutation::directory(project_root, &parent_relative)?;
    if parent.metadata()?.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the human ground directory is read-only",
        ));
    }
    match fs::symlink_metadata(project_root.join(&relative)) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the document already exists; creation never overwrites — open it instead",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let revision = content_revision_bytes(content.as_bytes());
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let staging = format!(".central-source-create-{}-{nonce}", std::process::id());
    let file_name = relative
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing file name"))?
        .to_os_string();
    let mut staged = crate::file_mutation::create_in(&parent, &staging, 0o600)?;
    let c_staging = CString::new(staging.as_str()).map_err(io::Error::other)?;
    let c_name = CString::new(file_name.as_bytes()).map_err(io::Error::other)?;
    let mut committed = false;
    let result = (|| -> io::Result<WorldSourceWriteReceipt> {
        staged.write_all(content.as_bytes())?;
        staged.sync_all()?;
        // Unlike rename, linkat atomically REFUSES an existing destination.
        if unsafe {
            libc::linkat(
                parent.as_raw_fd(),
                c_staging.as_ptr(),
                parent.as_raw_fd(),
                c_name.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        committed = true;
        if unsafe { libc::unlinkat(parent.as_raw_fd(), c_staging.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        parent.sync_all()?;
        // Independent readback before any record claims the creation.
        let written = crate::source_safety::read(project_root, relative_str)?;
        if written != content {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the newly created document changed before independent readback",
            ));
        }
        // The horizon is the creation's record: the new aperture source
        // arrives as an Added change carrying the declared attribution.
        let mut attributions = BTreeMap::new();
        attributions.insert(
            reference.clone(),
            SourceWriteAttribution {
                actor: actor.to_owned(),
                actor_kind: actor_kind.to_owned(),
                agent_session_ref: agent_session_ref.clone(),
            },
        );
        let report = reconcile_project_source_writes(project_root, &attributions)?;
        let change_ref = report
            .new_changes
            .iter()
            .find(|change| change.source_ref == reference)
            .map(|change| change.change_ref.clone());
        let binding = project_source_bindings(project_root)?
            .into_iter()
            .find(|binding| binding.source_ref == reference)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "the created document did not join the World source horizon",
                )
            })?;
        Ok(WorldSourceWriteReceipt {
            schema: WORLD_SOURCE_WRITE_RECEIPT_SCHEMA.to_owned(),
            world_ref,
            source: binding,
            previous_revision: String::new(),
            revision: SourceRevision {
                revision,
                byte_len: content.len() as u64,
            },
            changed: true,
            change_ref,
            actor: actor.to_owned(),
            actor_kind: actor_kind.to_owned(),
            agent_session_ref,
            automatic_agent_or_model_invocation: false,
        })
    })();
    if !committed {
        unsafe {
            libc::unlinkat(parent.as_raw_fd(), c_staging.as_ptr(), 0);
        }
    }
    result
}

fn create_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.source.create";
    let (project_root, root_register) = match source_scope(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if root_register {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "the root register has no human-ground creation door here; Control ground creates through central.files.create under Control/user/flows",
            None,
        );
    }
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
    let content = input.get("content").and_then(Value::as_str).unwrap_or("");
    create_scoped_source(
        &project_root,
        &path,
        content,
        &actor,
        &actor_kind,
        optional(input, "agent_session_ref"),
    )
    .map(|value| {
        ActionResult::success(
            action,
            json!({
                "receipt": serde_json::to_value(value).expect("World source create receipt serialises"),
                "automatic_agent_or_model_invocation": false
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
pub fn register_world_source_actions(registry: &mut ActionRegistry) {
    crate::source_return::register(registry);
    let mut actions: Vec<(
        ActionDescriptor,
        fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
    )> = vec![
        (
            descriptor(
                "projectcentral.source.read",
                "Read live World source",
                "Read one participating World source (omit project for the Central root meta-Project) by SourceRef with its exact content revision, provenance, standing and treatment. Reconciles the Source Change Horizon (derived .central state only) and never invokes an Agent or model. Sources excluded by .no-agent-retrieval are not disclosed here; masking is not missing.",
                MutationClass::LocallyMutating,
                "projectcentral-world-source-reading",
                &[("project", false), ("source_ref", true)],
            ),
            read_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.source.write",
                "Write live World source revision",
                "Revision-safe canonical whole-file write on one participating World source (omit project for the Central root meta-Project): a stale expected_revision fails without mutating, and the emitted Source Change Horizon change carries the declared actor, actor_kind and optional agent_session_ref. Attribution is declared, not proven: a write declaring actor_kind human never carries an agent_session_ref, and recognised human-authored or human-adopted sources, human-source aperture material and agent-governance sources refuse declared non-human callers and refuse every agent-session write — those callers propose instead of writing. Native Day, NOW, contribution and authority sources require their dedicated authenticated owner operations even for a declared-human caller. Never invokes an Agent or model.",
                MutationClass::LocallyMutating,
                "projectcentral-world-source-write-receipt",
                &[("project",false),("source_ref",true),("expected_revision",true),("content",false),("actor",true),("actor_kind",true),("agent_session_ref",false)],
            ),
            write_action,
        ),
    ];
    actions.push((
        descriptor(
            "projectcentral.source.create",
            "Create a Project human-ground document",
            "Create one absent document in a Project's own human ground (ProjectCentral/user) — the door behind a project vision page or mockup. Atomic no-overwrite admission: the parent must already exist, the destination must be absent, and the created file joins the World source horizon as project-human-source-aperture material whose later revisions go through projectcentral.source.write. A declared non-human caller is refused, as for every aperture write. The root register has no door here (Control ground creates through central.files.create). Never invokes an Agent or model.",
            MutationClass::LocallyMutating,
            "projectcentral-world-source-write-receipt",
            &[("project", true), ("path", true), ("content", true), ("actor", true), ("actor_kind", true), ("agent_session_ref", false)],
        ),
        create_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
    ));
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("World source Action ids are valid");
    }
}
