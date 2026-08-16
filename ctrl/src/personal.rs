use crate::action::{
    create_core_action_registry, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{
    NotificationRequest, PortErrorCode, USER_NOTIFICATION_PORT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
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
    evidence_refs: Vec<String>,
    status: String,
    accepted_by_ref: Option<String>,
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
        output: ActionOutputDefinition { output_type: output_type.to_owned() },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: crate::action::ActionAvailability { available: true, reason: None },
    }
}

fn string_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
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

fn refs(input: &Value, field: &str, action: &str) -> Result<Vec<String>, ActionResult> {
    let Some(value) = input.get(field) else { return Ok(Vec::new()); };
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
        let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
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

fn notification_failure(action: &str, error: central_connector_sdk::PortError, diagnostics: Value) -> ActionResult {
    let status = match error.code {
        PortErrorCode::InvalidInput => ResultStatus::InvalidInput,
        PortErrorCode::UnsupportedEnvironment
        | PortErrorCode::MissingDependency
        | PortErrorCode::CapabilityUnavailable => ResultStatus::UnavailableCapability,
        _ => ResultStatus::ConnectorFailure,
    };
    ActionResult::failure(
        Some(action),
        status,
        error.message.clone(),
        Some(json!({ "port": USER_NOTIFICATION_PORT.id, "provider_error": error, "diagnostics": diagnostics })),
    )
}

fn personal_notify_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let title = optional_text(input, "title").unwrap_or_default();
    let body = optional_text(input, "body").unwrap_or_default();
    let caller_ref = match required_text(input, "caller_ref", "personal.notify") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let provenance_refs = match refs(input, "provenance_refs", "personal.notify") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let request = NotificationRequest {
        title,
        body,
        subject_ref: optional_text(input, "subject_ref"),
        urgency: optional_text(input, "urgency"),
        category: optional_text(input, "category"),
        callback: optional_text(input, "callback"),
        action_ref: optional_text(input, "action_ref"),
        caller_ref,
        provenance_refs,
    };
    if let Err(error) = request.validate() {
        return notification_failure("personal.notify", error, json!({}));
    }

    let resolution = context.connectors.resolve(&USER_NOTIFICATION_PORT, context.connector_context);
    let diagnostics = to_value(&resolution.diagnostics).expect("Connector diagnostics serialise");
    let Some(connector) = resolution.connector else {
        return ActionResult::failure(
            Some("personal.notify"),
            ResultStatus::UnavailableCapability,
            format!("No eligible Connector implements {}.", USER_NOTIFICATION_PORT.id),
            Some(json!({ "port": USER_NOTIFICATION_PORT.id, "diagnostics": diagnostics })),
        );
    };
    let Some(notification) = connector.user_notification() else {
        return ActionResult::failure(
            Some("personal.notify"),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", USER_NOTIFICATION_PORT.id),
            Some(json!({ "port": USER_NOTIFICATION_PORT.id, "connector": connector.manifest().id, "diagnostics": diagnostics })),
        );
    };
    match notification.deliver(&request) {
        Ok(delivery) => ActionResult::success(
            "personal.notify",
            json!({
                "delivery": delivery,
                "notification_delivery_is_human_acknowledgement": false,
                "diagnostics": diagnostics,
            }),
        ),
        Err(error) => notification_failure("personal.notify", error, diagnostics),
    }
}

fn safe_control_target(root: &Path, target: &str) -> Result<PathBuf, String> {
    let relative = Path::new(target);
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err("Control proposal target must be a relative path below user, agents or machines.".to_owned());
    }
    let mut components = relative.components();
    let Some(Component::Normal(first)) = components.next() else {
        return Err("Control proposal target is invalid.".to_owned());
    };
    if !matches!(first.to_str(), Some("user" | "agents" | "machines")) {
        return Err("Control proposal target must begin with user/, agents/ or machines/.".to_owned());
    }
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return Err("Control proposal target must not contain traversal or platform-root components.".to_owned());
    }
    Ok(root.join("Control").join(relative))
}

fn proposal_dir(root: &Path) -> PathBuf {
    root.join(".central").join("proposals")
}

fn proposal_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')) {
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

fn write_proposal(path: &Path, proposal: &ControlProposal) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(proposal).map_err(io::Error::other)?;
    fs::write(path, bytes)
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
        Err(message) => return ActionResult::failure(Some("control.propose-change"), ResultStatus::InvalidInput, message, None),
    };
    if let Err(message) = safe_control_target(&root.path, &target) {
        return ActionResult::failure(Some("control.propose-change"), ResultStatus::InvalidInput, message, None);
    }
    let id = match new_proposal_id() {
        Ok(id) => id,
        Err(error) => return ActionResult::failure(Some("control.propose-change"), ResultStatus::InternalFailure, error.to_string(), None),
    };
    let proposal = ControlProposal {
        schema: PROPOSAL_SCHEMA.to_owned(),
        id: id.clone(),
        target,
        reason,
        classification: optional_text(input, "classification").unwrap_or_else(|| "authored-context".to_owned()),
        proposed_content,
        evidence_refs,
        status: "proposed".to_owned(),
        accepted_by_ref: None,
    };
    let path = proposal_dir(&root.path).join(format!("{id}.json"));
    match write_proposal(&path, &proposal) {
        Ok(()) => ActionResult::success(
            "control.propose-change",
            json!({ "proposal": proposal, "proposal_path": path, "authored_source_mutated": false }),
        ),
        Err(error) => ActionResult::failure(Some("control.propose-change"), ResultStatus::InternalFailure, error.to_string(), None),
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
        Err(message) => return ActionResult::failure(Some("control.review-proposal"), ResultStatus::InvalidInput, message, None),
    };
    let path = match proposal_path(&root.path, &id) {
        Ok(path) => path,
        Err(message) => return ActionResult::failure(Some("control.review-proposal"), ResultStatus::InvalidInput, message, None),
    };
    match read_proposal(&path) {
        Ok(proposal) => ActionResult::success("control.review-proposal", json!({ "proposal": proposal, "proposal_path": path })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => ActionResult::failure(
            Some("control.review-proposal"), ResultStatus::InvalidInput, format!("Unknown Control proposal: {id}"), None,
        ),
        Err(error) => ActionResult::failure(Some("control.review-proposal"), ResultStatus::InternalFailure, error.to_string(), None),
    }
}

fn control_apply_proposal_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let id = match required_text(input, "proposal_id", "control.apply-proposal") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let accepted_by_ref = match required_text(input, "accepted_by_ref", "control.apply-proposal") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::InvalidInput, message, None),
    };
    let proposal_file = match proposal_path(&root.path, &id) {
        Ok(path) => path,
        Err(message) => return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::InvalidInput, message, None),
    };
    let mut proposal = match read_proposal(&proposal_file) {
        Ok(proposal) => proposal,
        Err(error) => return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::InvalidInput, error.to_string(), None),
    };
    if proposal.status != "proposed" {
        return ActionResult::failure(
            Some("control.apply-proposal"), ResultStatus::InvalidInput,
            format!("Proposal {} is not pending.", proposal.id), None,
        );
    }
    let target = match safe_control_target(&root.path, &proposal.target) {
        Ok(target) => target,
        Err(message) => return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::InvalidInput, message, None),
    };
    if let Some(parent) = target.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::InternalFailure, error.to_string(), None);
        }
    }
    if let Err(error) = fs::write(&target, proposal.proposed_content.as_bytes()) {
        return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::InternalFailure, error.to_string(), None);
    }
    proposal.status = "applied".to_owned();
    proposal.accepted_by_ref = Some(accepted_by_ref.clone());
    if let Err(error) = write_proposal(&proposal_file, &proposal) {
        return ActionResult::failure(Some("control.apply-proposal"), ResultStatus::PartialCompletion, error.to_string(), Some(json!({ "target": target })));
    }
    ActionResult::success(
        "control.apply-proposal",
        json!({
            "proposal": proposal,
            "target": target,
            "accepted_by_ref": accepted_by_ref,
            "authored_source_mutated": true,
            "silent_learning": false,
        }),
    )
}

pub fn register_personal_actions(registry: &mut ActionRegistry) {
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

    let mut notify = descriptor(
        "personal.notify",
        "Notify user",
        "Request a provider-neutral user notification while keeping delivery distinct from human acknowledgement or approval.",
        MutationClass::ExternallyMutating,
        "notification-delivery",
    );
    notify.inputs = vec![
        string_input("title", false),
        string_input("body", false),
        string_input("subject_ref", false),
        string_input("urgency", false),
        string_input("category", false),
        string_input("callback", false),
        string_input("action_ref", false),
        string_input("caller_ref", true),
    ];
    notify.required_ports = vec![USER_NOTIFICATION_PORT.id.to_owned()];
    registry.register(notify, personal_notify_action).expect("personal Action ids are valid");

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
    registry.register(propose, control_propose_action).expect("personal Action ids are valid");

    let mut review = descriptor(
        "control.review-proposal",
        "Review Control proposal",
        "Read an explicit Control proposal and its provenance before any authored-source mutation.",
        MutationClass::ReadOnly,
        "control-proposal",
    );
    review.inputs = vec![string_input("proposal_id", true)];
    registry.register(review, control_review_proposal_action).expect("personal Action ids are valid");

    let mut apply = descriptor(
        "control.apply-proposal",
        "Apply Control proposal",
        "Apply one explicitly accepted proposal to the authored Control filesystem source.",
        MutationClass::LocallyMutating,
        "control-proposal-application",
    );
    apply.inputs = vec![string_input("proposal_id", true), string_input("accepted_by_ref", true)];
    registry.register(apply, control_apply_proposal_action).expect("personal Action ids are valid");
}

pub fn create_personal_action_registry() -> ActionRegistry {
    let mut registry = create_core_action_registry();
    register_personal_actions(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::{initialize_central, RootOptions};
    use central_connector_sdk::{
        CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
        NotificationCapabilities, NotificationCapabilityRequest, NotificationDelivery,
        NotificationDeliveryState, NotificationAuthorizationState, PortContract, PortError,
        UserNotification, CONNECTOR_API_VERSION,
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
                    ports: vec![ConnectorPortDeclaration { id: USER_NOTIFICATION_PORT.id.to_owned(), version: USER_NOTIFICATION_PORT.version.to_owned() }],
                    platforms: vec!["test".to_owned()],
                    entrypoint: "test".to_owned(),
                    runtime_requirements: Vec::new(),
                    dependency_probes: Vec::new(),
                    configuration_requirements: Vec::new(),
                    mutation_scope: "externally-mutating".to_owned(),
                },
            }
        }
    }

    impl UserNotification for NotificationFixture {
        fn capabilities(&self, _input: &NotificationCapabilityRequest) -> Result<NotificationCapabilities, PortError> {
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
        fn manifest(&self) -> &ConnectorManifest { &self.manifest }
        fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe { CapabilityProbe::available() }
        fn user_notification(&self) -> Option<&dyn UserNotification> { Some(self) }
    }

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("central-personal-test-{}-{}", std::process::id(), new_proposal_id().unwrap()));
        initialize_central(&root).unwrap();
        root
    }

    #[test]
    fn personal_read_model_is_filesystem_projection_not_profile_database() {
        let root = root();
        let options = RootOptions { explicit_root: Some(root.clone()), configured_root: None, home: None };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext { platform: "test".into() };
        let context = ActionExecutionContext { root_options: &options, connectors: &connectors, connector_context: &connector_context };
        let result = create_personal_action_registry().execute("personal.show", &json!({}), &context);
        assert!(result.ok);
        let data = result.data.unwrap();
        assert_eq!(data["profile_database"], false);
        assert_eq!(data["entries"][0]["relative_path"], "Control/user");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn proposal_does_not_mutate_authored_source_until_explicit_apply() {
        let root = root();
        let options = RootOptions { explicit_root: Some(root.clone()), configured_root: None, home: None };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext { platform: "test".into() };
        let context = ActionExecutionContext { root_options: &options, connectors: &connectors, connector_context: &connector_context };
        let registry = create_personal_action_registry();
        let proposed = registry.execute("control.propose-change", &json!({
            "target": "agents/collaboration.md",
            "reason": "Make durable collaboration preference inspectable",
            "classification": "agent-preference",
            "proposed_content": "Prefer evidence-backed changes.\n"
        }), &context);
        assert!(proposed.ok);
        assert!(!root.join("Control/agents/collaboration.md").exists());
        let id = proposed.data.as_ref().unwrap()["proposal"]["id"].as_str().unwrap().to_owned();
        let applied = registry.execute("control.apply-proposal", &json!({
            "proposal_id": id,
            "accepted_by_ref": "human:local-author"
        }), &context);
        assert!(applied.ok);
        assert_eq!(fs::read_to_string(root.join("Control/agents/collaboration.md")).unwrap(), "Prefer evidence-backed changes.\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn notification_delivery_keeps_acknowledgement_false_and_caller_lineage() {
        let root = root();
        let options = RootOptions { explicit_root: Some(root.clone()), configured_root: None, home: None };
        let mut connectors = central_connector_sdk::ConnectorRegistry::default();
        connectors.register(NotificationFixture::new()).unwrap();
        let connector_context = ConnectorContext { platform: "test".into() };
        let context = ActionExecutionContext { root_options: &options, connectors: &connectors, connector_context: &connector_context };
        let result = create_personal_action_registry().execute("personal.notify", &json!({
            "title": "Ready",
            "body": "Candidate B",
            "caller_ref": "factory:run:42",
            "subject_ref": "factory:candidate:B"
        }), &context);
        assert!(result.ok);
        assert_eq!(result.data.as_ref().unwrap()["delivery"]["caller_ref"], "factory:run:42");
        assert_eq!(result.data.as_ref().unwrap()["delivery"]["human_acknowledgement_observed"], false);
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
