use crate::action::{
    create_core_action_registry, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PROPOSAL_SCHEMA: &str = "central.control-proposal/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ControlProposal {
    schema: String,
    id: String,
    target: String,
    reason: String,
    classification: String,
    proposed_content: String,
    #[serde(default)]
    basis_revision: Option<String>,
    #[serde(default)]
    evidence_refs: Vec<String>,
    status: String,
    accepted_by_ref: Option<String>,
    /// The revision the target stands on once the proposal is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result_revision: Option<String>,
    /// The person's native acceptance the application was made on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authorization: Option<ProposalAuthorization>,
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs: Vec::new(),
        output: ActionOutputDefinition {
            output_type: output_type.to_owned(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: crate::action::ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

pub(crate) fn string_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

pub(crate) fn required_text(
    input: &Value,
    field: &str,
    action: &str,
) -> Result<String, ActionResult> {
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

pub(crate) fn optional_text(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn refs(input: &Value, field: &str, action: &str) -> Result<Vec<String>, ActionResult> {
    let Some(value) = input.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} {field} must be an array of string refs."),
            None,
        ));
    };
    let mut refs = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} {field} must contain only non-empty string refs."),
                None,
            ));
        };
        refs.push(value.to_owned());
    }
    Ok(refs)
}

fn personal_show_action(
    _registry: &ActionRegistry,
    _input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("personal.show"),
                ResultStatus::InvalidInput,
                message,
                None,
            )
        }
    };
    let entries = [
        ("You", "Control/user", "human-authored"),
        ("Agents", "Control/agents", "human-authored"),
        ("Machines", "Control/machines", "human-authored"),
        ("Work", "Work", "ordinary-work"),
    ]
    .into_iter()
    .map(|(name, relative, source_class)| {
        let path = root.path.join(relative);
        json!({
            "name": name,
            "relative_path": relative,
            "path": path,
            "exists": path.exists(),
            "source_class": source_class,
            "derived": false,
        })
    })
    .collect::<Vec<_>>();

    ActionResult::success(
        "personal.show",
        json!({
            "root": root.path,
            "entries": entries,
            "profile_database": false,
            "orchestration_database": false,
            "silent_preference_learning": false,
        }),
    )
}

fn safe_control_target(root: &Path, target: &str) -> Result<PathBuf, String> {
    let relative = Path::new(target);
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(
            "Control proposal target must be a relative path below user, agents or machines."
                .to_owned(),
        );
    }
    let mut components = relative.components();
    let Some(Component::Normal(first)) = components.next() else {
        return Err("Control proposal target is invalid.".to_owned());
    };
    if !matches!(first.to_str(), Some("user" | "agents" | "machines")) {
        return Err(
            "Control proposal target must begin with user/, agents/ or machines/.".to_owned(),
        );
    }
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return Err(
            "Control proposal target must not contain traversal or platform-root components."
                .to_owned(),
        );
    }
    crate::source_safety::reject_symlink_components(root, &Path::new("Control").join(relative))
        .map_err(|e| e.to_string())?;
    Ok(root.join("Control").join(relative))
}

fn proposal_dir(root: &Path) -> PathBuf {
    root.join(".central").join("proposals")
}

fn proposal_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Proposal id contains unsupported characters.".to_owned());
    }
    Ok(proposal_dir(root).join(format!("{id}.json")))
}

fn new_proposal_id() -> Result<String, io::Error> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    Ok(format!("proposal-{nanos}-{}", std::process::id()))
}

fn write_proposal(root: &Path, path: &Path, proposal: &ControlProposal) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(proposal).map_err(io::Error::other)?;
    crate::file_mutation::atomic_record(
        root,
        path.strip_prefix(root).map_err(io::Error::other)?,
        &bytes,
        crate::file_mutation::RecordDisposition::CreateNew,
    )
}

fn read_proposal(path: &Path) -> Result<ControlProposal, io::Error> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn control_propose_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let target = match required_text(input, "target", "control.propose-change") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let reason = match required_text(input, "reason", "control.propose-change") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let proposed_content = match input.get("proposed_content").and_then(Value::as_str) {
        Some(value) => value.to_owned(),
        None => {
            return ActionResult::failure(
                Some("control.propose-change"),
                ResultStatus::InvalidInput,
                "control.propose-change requires proposed_content.",
                None,
            )
        }
    };
    let evidence_refs = match refs(input, "evidence_refs", "control.propose-change") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("control.propose-change"),
                ResultStatus::InvalidInput,
                message,
                None,
            )
        }
    };
    if let Err(message) = safe_control_target(&root.path, &target) {
        return ActionResult::failure(
            Some("control.propose-change"),
            ResultStatus::InvalidInput,
            message,
            None,
        );
    }
    let id = match new_proposal_id() {
        Ok(id) => id,
        Err(error) => {
            return ActionResult::failure(
                Some("control.propose-change"),
                ResultStatus::InternalFailure,
                error.to_string(),
                None,
            )
        }
    };
    if proposed_content.len() > crate::source_safety::MAX_SOURCE || proposed_content.contains('\0')
    {
        return ActionResult::failure(
            Some("control.propose-change"),
            ResultStatus::InvalidInput,
            "Control proposal must be bounded UTF-8 text",
            None,
        );
    }
    let basis_revision = match crate::source_safety::read(&root.path.join("Control"), &target) {
        Ok(content) => crate::source_safety::content_revision_bytes(content.as_bytes()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => "central.absent/v1".into(),
        Err(e) => {
            return ActionResult::failure(
                Some("control.propose-change"),
                ResultStatus::InvalidInput,
                e.to_string(),
                None,
            )
        }
    };
    let proposal = ControlProposal {
        schema: PROPOSAL_SCHEMA.to_owned(),
        id: id.clone(),
        target,
        reason,
        classification: optional_text(input, "classification")
            .unwrap_or_else(|| "authored-context".to_owned()),
        proposed_content,
        basis_revision: Some(basis_revision),
        evidence_refs,
        status: "proposed".to_owned(),
        accepted_by_ref: None,
        result_revision: None,
        authorization: None,
    };
    let path = proposal_dir(&root.path).join(format!("{id}.json"));
    match write_proposal(&root.path, &path, &proposal) {
        Ok(()) => ActionResult::success(
            "control.propose-change",
            json!({ "proposal": proposal, "proposal_path": path, "authored_source_mutated": false }),
        ),
        Err(error)
            if crate::file_mutation::record_failure_result("control.propose-change", &error)
                .is_some() =>
        {
            crate::file_mutation::record_failure_result("control.propose-change", &error)
                .expect("matched native record failure")
        }
        Err(error) => ActionResult::failure(
            Some("control.propose-change"),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

fn control_review_proposal_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let id = match required_text(input, "proposal_id", "control.review-proposal") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("control.review-proposal"),
                ResultStatus::InvalidInput,
                message,
                None,
            )
        }
    };
    let path = match proposal_path(&root.path, &id) {
        Ok(path) => path,
        Err(message) => {
            return ActionResult::failure(
                Some("control.review-proposal"),
                ResultStatus::InvalidInput,
                message,
                None,
            )
        }
    };
    match read_proposal(&path) {
        // The preview of an application: whether the target still stands on the proposal's basis, and what applying needs.
        Ok(proposal) => {
            let (current, applicable) = applicability(&root.path, &proposal);
            ActionResult::success(
                "control.review-proposal",
                json!({
                    "proposal": proposal,
                    "proposal_path": path,
                    "current_revision": current,
                    "applicable": applicable,
                    "apply_requires": {"action": APPLY_PROPOSAL, "scope_ref": "control:root",
                        "human_principal": true, "accepted_by_ref": "the authenticated principal_ref"},
                    "authored_source_mutated": false,
                }),
            )
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => ActionResult::failure(
            Some("control.review-proposal"),
            ResultStatus::InvalidInput,
            format!("Unknown Control proposal: {id}"),
            None,
        ),
        Err(error) => ActionResult::failure(
            Some("control.review-proposal"),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

/// The Action a Control proposal is applied as: the grant the person's native credential must carry.
pub const APPLY_PROPOSAL: &str = "control.apply-proposal";

/// The person's acceptance of one Control proposal, recorded on the proposal once it is applied: who accepted, under which
/// authority source at which revision, and the revision the target now stands on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ProposalAuthorization {
    principal_ref: String,
    authority_ref: String,
    authority_revision: String,
    accepted_at_unix_seconds: u64,
}

fn current_revision(root: &Path, target: &str) -> io::Result<String> {
    match crate::source_safety::read(&root.join("Control"), target) {
        Ok(content) => Ok(crate::source_safety::content_revision_bytes(
            content.as_bytes(),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok("central.absent/v1".into()),
        Err(e) => Err(e),
    }
}

fn save_proposal(root: &Path, path: &Path, proposal: &ControlProposal) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(proposal).map_err(io::Error::other)?;
    crate::file_mutation::atomic_record(
        root,
        path.strip_prefix(root).map_err(io::Error::other)?,
        &bytes,
        crate::file_mutation::RecordDisposition::ReplaceOrCreate,
    )
}

/// Apply one pending Control proposal on the person's native authority (the same production path as the Action, with an
/// explicit credential channel and clock for tests; the credential never appears in input or output). The proposal's basis must
/// still be the target's revision; the credential must name a human principal granted `control.apply-proposal` at `control:root`;
/// and `accepted_by_ref` must be that principal. The authority source itself is never a proposal's target. The target is then
/// replaced (or created, when the proposal was made against an absent file) on that basis, and the proposal records its acceptance.
pub fn apply_proposal_with_token_at(
    central: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    use crate::continuous_work::{authority, source};
    let text = |field: &str| -> io::Result<String> {
        input
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 4096)
            .map(str::to_owned)
            .ok_or_else(|| source::invalid(format!("{APPLY_PROPOSAL} requires {field}.")))
    };
    let id = text("proposal_id")?;
    let accepted_by_ref = text("accepted_by_ref")?;
    let expected_authority = match input.get("expected_authority_revision") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= 4096 => {
            Some(value.clone())
        }
        Some(_) => {
            return Err(source::invalid(
                "expected_authority_revision requires bounded nonempty text",
            ))
        }
    };
    let scope = source::Scope::resolve(central, None)?;
    let root = scope.central_root.clone();
    let proposal_file = proposal_path(&root, &id).map_err(source::invalid)?;
    // Order: the root's source-authority lock, then the proposal is read and checked, as Control acceptance does elsewhere.
    let _locks = source::lock(&scope)?;
    let mut proposal = match read_proposal(&proposal_file) {
        Ok(proposal) => proposal,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(source::invalid(format!("Unknown Control proposal: {id}")))
        }
        Err(error) => return Err(error),
    };
    if proposal.status != "proposed" {
        return Err(source::invalid(format!(
            "Proposal {} is not pending.",
            proposal.id
        )));
    }
    safe_control_target(&root, &proposal.target).map_err(source::invalid)?;
    let Some(basis) = proposal.basis_revision.clone() else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Legacy Control proposal has no source basis; create a fresh native proposal before review",
        ));
    };
    let current = current_revision(&root, &proposal.target)?;
    if current != basis {
        return Ok(
            json!({"outcome":"conflict","proposal":proposal,"expected_revision":basis,"current_revision":current,
            "authored_source_mutated":false}),
        );
    }
    let principal = authority::authenticate(
        &scope,
        token,
        APPLY_PROPOSAL,
        expected_authority.as_deref(),
        now,
    )?;
    principal.require_human()?;
    if accepted_by_ref != principal.principal_ref {
        return Err(source::denied(
            "accepted_by_ref differs from the authenticated accepting principal",
        ));
    }
    // The authority source is not changed through the authority it defines: a proposal against it is refused, whoever accepts.
    let (_, relation, _) = authority::recognised_source(&scope, authority::AUTHORITY_ROLE)?;
    let target_path = format!("Control/{}", proposal.target);
    if relation["path"].as_str() == Some(target_path.as_str()) {
        return Err(source::denied(
            "The native-action-authority source is not changed through a Control proposal; the owner edits it directly",
        ));
    }
    proposal.status = "applying".into();
    proposal.accepted_by_ref = Some(accepted_by_ref);
    save_proposal(&root, &proposal_file, &proposal)?;
    let written = if basis == "central.absent/v1" {
        source::put_new(&root, &target_path, &proposal.proposed_content).map(|_| ())
    } else {
        crate::source_safety::replace(&root, &target_path, &basis, &proposal.proposed_content)
    };
    match written {
        Ok(()) => {
            let revision =
                crate::source_safety::content_revision_bytes(proposal.proposed_content.as_bytes());
            proposal.status = "accepted".into();
            proposal.result_revision = Some(revision.clone());
            proposal.authorization = Some(ProposalAuthorization {
                principal_ref: principal.principal_ref.clone(),
                authority_ref: principal.authority_ref.clone(),
                authority_revision: principal.authority_revision.clone(),
                accepted_at_unix_seconds: now,
            });
            save_proposal(&root, &proposal_file, &proposal).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("{error}; the Control source was written at {revision}"),
                )
            })?;
            Ok(
                json!({"outcome":"accepted","proposal":proposal,"target_path":target_path,"revision":revision,
                "authored_source_mutated":true}),
            )
        }
        Err(primary) => {
            // The target did not move: the proposal stands pending again. A changed target keeps the record as it is.
            if current_revision(&root, &proposal.target).ok().as_deref() == Some(basis.as_str()) {
                proposal.status = "proposed".into();
                proposal.accepted_by_ref = None;
                let _ = save_proposal(&root, &proposal_file, &proposal);
            }
            Err(primary)
        }
    }
}

/// The application preview of one proposal: the target's current revision, and whether the proposal still applies.
fn applicability(root: &Path, proposal: &ControlProposal) -> (Option<String>, bool) {
    let current = safe_control_target(root, &proposal.target)
        .ok()
        .and_then(|_| current_revision(root, &proposal.target).ok());
    let applicable = proposal.status == "proposed"
        && proposal.basis_revision.is_some()
        && current.is_some()
        && current == proposal.basis_revision;
    (current, applicable)
}

/// `control.list-proposals`: every Control proposal kept under `.central/proposals`, oldest first, each with its application
/// preview; `status` narrows the list (`proposed` for the pending ones). A file that cannot be read is named, never dropped.
fn control_list_proposals_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("control.list-proposals"),
                ResultStatus::InvalidInput,
                message,
                None,
            )
        }
    };
    let status = optional_text(input, "status");
    let directory = proposal_dir(&root.path);
    let mut paths = match fs::read_dir(&directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return ActionResult::failure(
                Some("control.list-proposals"),
                ResultStatus::InternalFailure,
                format!("cannot read {}: {error}", directory.display()),
                None,
            )
        }
    };
    paths.sort();
    let mut proposals = Vec::new();
    let mut unreadable = Vec::new();
    for path in paths {
        match read_proposal(&path) {
            Ok(proposal)
                if status
                    .as_deref()
                    .is_none_or(|status| status == proposal.status) =>
            {
                let (current, applicable) = applicability(&root.path, &proposal);
                proposals.push(
                    json!({"proposal": proposal, "proposal_path": path, "current_revision": current,
                    "applicable": applicable}),
                );
            }
            Ok(_) => {}
            Err(error) => unreadable.push(json!({"path": path, "reason": error.to_string()})),
        }
    }
    ActionResult::success(
        "control.list-proposals",
        json!({"proposals": proposals, "unreadable": unreadable, "apply_action": APPLY_PROPOSAL,
            "authored_source_mutated": false}),
    )
}

fn control_apply_proposal_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let result = (|| {
        let root = resolve_central_root(context.root_options)
            .map_err(crate::continuous_work::source::invalid)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
        apply_proposal_with_token_at(&root.path, input, token.as_deref(), now)
    })();
    match result {
        Ok(data) if data["outcome"] == "conflict" => ActionResult::failure(
            Some(APPLY_PROPOSAL),
            ResultStatus::VerificationFailure,
            "Control proposal source basis changed; concurrent source is preserved",
            Some(data),
        ),
        Ok(data) => ActionResult::success(APPLY_PROPOSAL, data),
        Err(error) => {
            let (status, code) = match error.kind() {
                io::ErrorKind::PermissionDenied => (
                    ResultStatus::UnavailableCapability,
                    "control_proposal_authority_denied",
                ),
                io::ErrorKind::Unsupported => (
                    ResultStatus::UnavailableCapability,
                    "control_proposal_legacy",
                ),
                io::ErrorKind::AlreadyExists => {
                    (ResultStatus::VerificationFailure, "control_proposal_stale")
                }
                io::ErrorKind::InvalidInput => {
                    (ResultStatus::InvalidInput, "control_proposal_invalid")
                }
                _ => (ResultStatus::InternalFailure, "control_proposal_failed"),
            };
            ActionResult::failure_coded(
                Some(APPLY_PROPOSAL),
                status,
                code,
                error.to_string(),
                Some(json!({"authored_source_mutated":false,
                    "setup":{"owner":"central","section":"native-authority","action":APPLY_PROPOSAL,
                        "credential_channel":"native-protected-process","raw_credentials_accepted":false},
                    "automatic_retry":false})),
            )
        }
    }
}

/// Install the person's ground reading, `personal.show` (read-only). The runtime catalogue (cli.rs
/// `create_runtime_action_registry`) installs it with the Control proposal actions through `register_personal_actions`,
/// so the CLI, the macOS host and the O:I kernel reach them.
pub fn register_personal_show_action(registry: &mut ActionRegistry) {
    registry
        .register(
            descriptor(
                "personal.show",
                "Show Personal ground",
                "Project the actual authored Central ground as You, Agents, Machines and Work without a profile database.",
                MutationClass::ReadOnly,
                "personal-root-world",
            ),
            personal_show_action,
        )
        .expect("personal Action ids are valid");
}

pub fn register_personal_actions(registry: &mut ActionRegistry) {
    register_personal_show_action(registry);

    let mut propose = descriptor(
        "control.propose-change",
        "Propose Control change",
        "Create explicit derived proposal material without mutating human-authored Control source.",
        MutationClass::LocallyMutating,
        "control-proposal",
    );
    propose.inputs = vec![
        string_input("target", true),
        string_input("reason", true),
        string_input("classification", false),
        string_input("proposed_content", true),
    ];
    registry
        .register(propose, control_propose_action)
        .expect("personal Action ids are valid");

    let mut review = descriptor(
        "control.review-proposal",
        "Review Control proposal",
        "Read an explicit Control proposal and its provenance before any authored-source mutation.",
        MutationClass::ReadOnly,
        "control-proposal",
    );
    review.inputs = vec![string_input("proposal_id", true)];
    registry
        .register(review, control_review_proposal_action)
        .expect("personal Action ids are valid");

    let mut list = descriptor(
        "control.list-proposals",
        "List Control proposals",
        "List every Control proposal with its application preview (current target revision, applicable); status narrows it.",
        MutationClass::ReadOnly,
        "control-proposal-list",
    );
    list.inputs = vec![string_input("status", false)];
    registry
        .register(list, control_list_proposals_action)
        .expect("personal Action ids are valid");

    let mut apply = descriptor(
        APPLY_PROPOSAL,
        "Apply Control proposal",
        "Apply one pending proposal to the authored Control filesystem source on the person's native authority (a human \
         principal granted control.apply-proposal at control:root), on the revision it was proposed against.",
        MutationClass::LocallyMutating,
        "control-proposal-application",
    );
    apply.inputs = vec![
        string_input("proposal_id", true),
        string_input("accepted_by_ref", true),
        string_input("expected_authority_revision", false),
    ];
    registry
        .register(apply, control_apply_proposal_action)
        .expect("personal Action ids are valid");
}

pub fn create_personal_action_registry() -> ActionRegistry {
    let mut registry = create_core_action_registry();
    register_personal_actions(&mut registry);
    // The person's notifications are installed in the runtime catalogue too (cli.rs create_runtime_action_registry).
    crate::notification::register_notification_actions(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::{initialize_central, RootOptions};
    use central_connector_sdk::{
        CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
        NotificationAuthorizationState, NotificationCapabilities, NotificationCapabilityRequest,
        NotificationDelivery, NotificationDeliveryState, NotificationRequest, PortContract,
        PortError, UserNotification, CONNECTOR_API_VERSION, USER_NOTIFICATION_PORT,
    };

    struct NotificationFixture {
        manifest: ConnectorManifest,
    }

    impl NotificationFixture {
        fn new() -> Self {
            Self {
                manifest: ConnectorManifest {
                    api_version: CONNECTOR_API_VERSION.to_owned(),
                    id: "fixture.notification".to_owned(),
                    version: "1".to_owned(),
                    display_name: "Notification fixture".to_owned(),
                    ports: vec![ConnectorPortDeclaration {
                        id: USER_NOTIFICATION_PORT.id.to_owned(),
                        version: USER_NOTIFICATION_PORT.version.to_owned(),
                    }],
                    platforms: vec!["test".to_owned()],
                    entrypoint: "test".to_owned(),
                    runtime_requirements: Vec::new(),
                    dependency_probes: Vec::new(),
                    configuration_requirements: Vec::new(),
                    mutation_scope: "externally-mutating".to_owned(),
                    known_limitations: Vec::new(),
                },
            }
        }
    }

    impl UserNotification for NotificationFixture {
        fn capabilities(
            &self,
            _input: &NotificationCapabilityRequest,
        ) -> Result<NotificationCapabilities, PortError> {
            Ok(NotificationCapabilities {
                available: true,
                authorization: NotificationAuthorizationState::Granted,
                supports_callback: true,
                supports_urgency: true,
                supports_category: true,
                provider: "fixture".into(),
                notes: Vec::new(),
            })
        }

        fn deliver(&self, input: &NotificationRequest) -> Result<NotificationDelivery, PortError> {
            Ok(NotificationDelivery {
                state: NotificationDeliveryState::Posted,
                provider: "fixture".into(),
                subject_ref: input.subject_ref.clone(),
                action_ref: input.action_ref.clone(),
                caller_ref: input.caller_ref.clone(),
                human_acknowledgement_observed: false,
                unsupported_requested_features: Vec::new(),
                provenance_refs: input.provenance_refs.clone(),
            })
        }
    }

    impl Connector for NotificationFixture {
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }
        fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
            CapabilityProbe::available()
        }
        fn user_notification(&self) -> Option<&dyn UserNotification> {
            Some(self)
        }
    }

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "central-personal-test-{}-{}",
            std::process::id(),
            new_proposal_id().unwrap()
        ));
        initialize_central(&root).unwrap();
        root
    }

    #[test]
    fn personal_show_and_the_control_proposals_are_installed_in_the_runtime_catalogue() {
        let registry = crate::cli::create_runtime_action_registry();
        let show = registry
            .get("personal.show")
            .expect("personal.show installed");
        assert_eq!(show.mutation_class, MutationClass::ReadOnly);
        // REPLACED: the proposal actions were library-only while apply had no human-authority route; apply now
        // authenticates the person's native credential, so the CLI, the host and the O:I kernel reach all three.
        for (installed, class) in [
            ("control.propose-change", MutationClass::LocallyMutating),
            ("control.review-proposal", MutationClass::ReadOnly),
            ("control.list-proposals", MutationClass::ReadOnly),
            (APPLY_PROPOSAL, MutationClass::LocallyMutating),
        ] {
            let descriptor = registry
                .get(installed)
                .unwrap_or_else(|| panic!("{installed} installed"));
            assert_eq!(descriptor.mutation_class, class, "{installed}");
        }
        let root = root();
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let shown = registry.execute("personal.show", &json!({}), &context);
        assert!(shown.ok, "{shown:?}");
        let data = shown.data.unwrap();
        assert_eq!(
            data["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["You", "Agents", "Machines", "Work"]
        );
        assert_eq!(data["profile_database"], false);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn personal_read_model_is_filesystem_projection_not_profile_database() {
        let root = root();
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let result =
            create_personal_action_registry().execute("personal.show", &json!({}), &context);
        assert!(result.ok);
        let data = result.data.unwrap();
        assert_eq!(data["profile_database"], false);
        assert_eq!(data["entries"][0]["relative_path"], "Control/user");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn proposal_acceptance_text_does_not_grant_control_source_authority() {
        let root = root();
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = create_personal_action_registry();
        let proposed = registry.execute(
            "control.propose-change",
            &json!({
                "target": "agents/collaboration.md",
                "reason": "Make durable collaboration preference inspectable",
                "classification": "agent-preference",
                "proposed_content": "Prefer evidence-backed changes.\n"
            }),
            &context,
        );
        assert!(proposed.ok);
        assert!(!root.join("Control/agents/collaboration.md").exists());
        let id = proposed.data.as_ref().unwrap()["proposal"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let applied = registry.execute(
            "control.apply-proposal",
            &json!({
                "proposal_id": id,
                "accepted_by_ref": "human:local-author"
            }),
            &context,
        );
        assert!(!applied.ok);
        assert_eq!(applied.status, ResultStatus::UnavailableCapability);
        assert!(!root.join("Control/agents/collaboration.md").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn control_proposals_keep_basis_and_refuse_stale_or_legacy_application() {
        let root = root();
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = create_personal_action_registry();
        let source = root.join("Control/user/intent.md");
        fs::write(&source, "basis").unwrap();
        let proposal=registry.execute("control.propose-change",&json!({"target":"user/intent.md","reason":"return test","proposed_content":"proposal"}),&context);
        assert!(proposal.ok, "{proposal:?}");
        let id = proposal.data.unwrap()["proposal"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        fs::write(&source, "concurrent human text").unwrap();
        let stale = registry.execute(
            "control.apply-proposal",
            &json!({"proposal_id":id,"accepted_by_ref":"human:claimed"}),
            &context,
        );
        assert_eq!(stale.status, ResultStatus::VerificationFailure);
        assert_eq!(
            fs::read_to_string(&source).unwrap(),
            "concurrent human text"
        );
        let file = proposal_path(&root, &id).unwrap();
        let mut legacy: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("basis_revision");
        fs::write(&file, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let legacy = registry.execute(
            "control.apply-proposal",
            &json!({"proposal_id":id,"accepted_by_ref":"human:claimed"}),
            &context,
        );
        assert_eq!(legacy.status, ResultStatus::UnavailableCapability);
        assert_eq!(
            fs::read_to_string(&source).unwrap(),
            "concurrent human text"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn notification_delivery_keeps_acknowledgement_false_and_caller_lineage() {
        let root = root();
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        };
        let mut connectors = central_connector_sdk::ConnectorRegistry::default();
        connectors.register(NotificationFixture::new()).unwrap();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let result = create_personal_action_registry().execute(
            "personal.notify",
            &json!({
                "title": "Ready",
                "body": "Candidate B",
                "caller_ref": "factory:run:42",
                "subject_ref": "factory:candidate:B"
            }),
            &context,
        );
        assert!(result.ok);
        assert_eq!(
            result.data.as_ref().unwrap()["delivery"]["caller_ref"],
            "factory:run:42"
        );
        assert_eq!(
            result.data.as_ref().unwrap()["delivery"]["human_acknowledgement_observed"],
            false
        );
        let _ = fs::remove_dir_all(root);
    }

    const HUMAN_TOKEN: &str = "control-proposal-human-controlled-test-not-a-live-secret";
    const AGENT_TOKEN: &str = "control-proposal-agent-controlled-test-not-a-live-secret";
    const OTHER_TOKEN: &str = "control-proposal-human-without-the-grant-not-a-live-secret";

    /// A disposable root whose recognised native-action-authority source grants `control.apply-proposal` to one human
    /// and one agent credential, and grants a second human only another action.
    fn authority_root() -> PathBuf {
        use sha2::{Digest, Sha256};
        let root = root();
        let scope = crate::continuous_work::source::Scope::resolve(&root, None).unwrap();
        let grant = |token: &str, kind: &str, principal: &str, actions: &[&str]| {
            json!({"principal_ref": principal, "actor_kind": kind,
                "token_sha256": format!("{:x}", Sha256::digest(token.as_bytes())),
                "scope_refs": ["control:root"], "actions": actions, "expires_at_unix_seconds": 200})
        };
        let grants = vec![
            grant(
                HUMAN_TOKEN,
                "human",
                "human:controlled-test",
                &[APPLY_PROPOSAL],
            ),
            grant(
                AGENT_TOKEN,
                "agent",
                "agent:controlled-test",
                &[APPLY_PROPOSAL],
            ),
            grant(
                OTHER_TOKEN,
                "human",
                "human:other-test",
                &["central.now.lifecycle"],
            ),
        ];
        fs::write(
            root.join("Control/user/proposal-authority.json"),
            serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants}))
                .unwrap(),
        )
        .unwrap();
        fs::create_dir_all(root.join("Control/relations")).unwrap();
        fs::write(
            root.join("Control/relations/source-relations.json"),
            serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[
                {"ref":scope.source_ref("Control/user/proposal-authority.json"),"path":"Control/user/proposal-authority.json",
                 "roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract",
                 "treatment":"projectcentral-user","recognition":"controlled-test-only-not-personal-adoption",
                 "recorded_at_unix_seconds":1}]}))
            .unwrap(),
        )
        .unwrap();
        root
    }

    fn propose(root: &Path, target: &str, content: &str) -> String {
        let options = RootOptions {
            explicit_root: Some(root.to_path_buf()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let proposed = create_personal_action_registry().execute(
            "control.propose-change",
            &json!({"target": target, "reason": "controlled test", "proposed_content": content}),
            &context,
        );
        assert!(proposed.ok, "{proposed:?}");
        proposed.data.unwrap()["proposal"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn apply(root: &Path, id: &str, accepted_by: &str, token: Option<&str>) -> io::Result<Value> {
        apply_proposal_with_token_at(
            root,
            &json!({"proposal_id": id, "accepted_by_ref": accepted_by}),
            token,
            100,
        )
    }

    #[test]
    fn a_human_credential_granted_the_apply_writes_the_proposal_and_nothing_else_does() {
        let root = authority_root();
        let target = root.join("Control/user/intent.md");
        fs::write(&target, "basis\n").unwrap();
        let id = propose(&root, "user/intent.md", "accepted words\n");

        // Neither a missing credential, an agent's, a human's without this grant, nor a name that is not the
        // authenticated principal writes anything; the proposal stays pending each time.
        for (token, accepted_by) in [
            (None, "human:controlled-test"),
            (Some(AGENT_TOKEN), "agent:controlled-test"),
            (Some(OTHER_TOKEN), "human:other-test"),
            (Some(HUMAN_TOKEN), "human:someone-else"),
        ] {
            let refused = apply(&root, &id, accepted_by, token).unwrap_err();
            assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
            assert_eq!(fs::read_to_string(&target).unwrap(), "basis\n");
            let pending = read_proposal(&proposal_path(&root, &id).unwrap()).unwrap();
            assert_eq!(pending.status, "proposed");
            assert_eq!(pending.accepted_by_ref, None);
        }

        let applied = apply(&root, &id, "human:controlled-test", Some(HUMAN_TOKEN)).unwrap();
        assert_eq!(applied["outcome"], "accepted", "{applied}");
        assert_eq!(applied["authored_source_mutated"], true);
        assert_eq!(fs::read_to_string(&target).unwrap(), "accepted words\n");
        let recorded = read_proposal(&proposal_path(&root, &id).unwrap()).unwrap();
        assert_eq!(recorded.status, "accepted");
        assert_eq!(
            recorded.accepted_by_ref.as_deref(),
            Some("human:controlled-test")
        );
        assert_eq!(
            recorded.result_revision.as_deref(),
            Some(crate::source_safety::content_revision_bytes(b"accepted words\n").as_str())
        );
        let authorization = recorded.authorization.unwrap();
        assert_eq!(authorization.principal_ref, "human:controlled-test");
        assert!(authorization
            .authority_ref
            .ends_with("Control/user/proposal-authority.json"));
        assert_eq!(authorization.accepted_at_unix_seconds, 100);
        // Applied once: a second press is refused, not replayed.
        let again = apply(&root, &id, "human:controlled-test", Some(HUMAN_TOKEN)).unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::InvalidInput);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_absent_target_is_created_a_moved_one_conflicts_and_the_authority_source_is_never_a_target(
    ) {
        let root = authority_root();
        let created = propose(&root, "agents/notes/new.md", "new words\n");
        let applied = apply(&root, &created, "human:controlled-test", Some(HUMAN_TOKEN)).unwrap();
        assert_eq!(applied["outcome"], "accepted", "{applied}");
        assert_eq!(
            fs::read_to_string(root.join("Control/agents/notes/new.md")).unwrap(),
            "new words\n"
        );

        let target = root.join("Control/user/intent.md");
        fs::write(&target, "basis\n").unwrap();
        let moved = propose(&root, "user/intent.md", "proposal\n");
        fs::write(&target, "concurrent human text\n").unwrap();
        let conflict = apply(&root, &moved, "human:controlled-test", Some(HUMAN_TOKEN)).unwrap();
        assert_eq!(conflict["outcome"], "conflict");
        assert_eq!(conflict["authored_source_mutated"], false);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "concurrent human text\n"
        );

        let authority = propose(&root, "user/proposal-authority.json", "{}");
        let refused = apply(
            &root,
            &authority,
            "human:controlled-test",
            Some(HUMAN_TOKEN),
        )
        .unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
        assert!(
            fs::read_to_string(root.join("Control/user/proposal-authority.json"))
                .unwrap()
                .contains("native-action-authority")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_previews_whether_the_proposal_still_applies() {
        let root = authority_root();
        let target = root.join("Control/user/intent.md");
        fs::write(&target, "basis\n").unwrap();
        let id = propose(&root, "user/intent.md", "proposal\n");
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = create_personal_action_registry();
        let review = |registry: &ActionRegistry| {
            let reviewed = registry.execute(
                "control.review-proposal",
                &json!({"proposal_id": id}),
                &context,
            );
            assert!(reviewed.ok, "{reviewed:?}");
            reviewed.data.unwrap()
        };
        let before = review(&registry);
        assert_eq!(before["applicable"], true, "{before}");
        assert_eq!(
            before["current_revision"],
            before["proposal"]["basis_revision"]
        );
        assert_eq!(before["apply_requires"]["action"], APPLY_PROPOSAL);
        let pending = registry.execute(
            "control.list-proposals",
            &json!({"status": "proposed"}),
            &context,
        );
        assert!(pending.ok, "{pending:?}");
        let pending = pending.data.unwrap();
        assert_eq!(
            pending["proposals"].as_array().unwrap().len(),
            1,
            "{pending}"
        );
        assert_eq!(pending["proposals"][0]["proposal"]["id"], id.as_str());
        assert_eq!(pending["proposals"][0]["applicable"], true);
        let none = registry.execute(
            "control.list-proposals",
            &json!({"status": "accepted"}),
            &context,
        );
        assert_eq!(none.data.unwrap()["proposals"], json!([]));
        fs::write(&target, "moved\n").unwrap();
        let after = review(&registry);
        assert_eq!(after["applicable"], false, "{after}");
        let listed = registry
            .execute("control.list-proposals", &json!({}), &context)
            .data
            .unwrap();
        assert_eq!(listed["proposals"][0]["applicable"], false);
        assert_eq!(fs::read_to_string(&target).unwrap(), "moved\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn proposal_target_rejects_path_escape() {
        let root = root();
        assert!(safe_control_target(&root, "../outside").is_err());
        assert!(safe_control_target(&root, "agents/../user/x").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
