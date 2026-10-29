//! The person's notifications, installed and kept.
//!
//! `personal.notify` asks for a provider-neutral host notification through the
//! UserNotification Port (connector-sdk notification.rs), exactly as before, and
//! now keeps what was asked and what the provider answered: one
//! `central.notification-record/v1` per request in the root's notification
//! ledger (`.central/notifications/ledger.jsonl`, append-only, serialised by the
//! root's `notifications.lock`). A request the provider refused is kept with its
//! refusal; a delivery is never the person's acknowledgement
//! (`notification_delivery_is_human_acknowledgement: false`).
//!
//! `personal.notifications.list` reads the ledger back, newest first, with the
//! selected provider's own capabilities (the Port's `capabilities` operation):
//! what the shell's Notifications monitor draws. It changes nothing.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::personal::{optional_text, refs, required_text, string_input};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{
    NotificationCapabilityRequest, NotificationDelivery, NotificationRequest, PortErrorCode,
    USER_NOTIFICATION_PORT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const NOTIFICATION_RECORD_SCHEMA: &str = "central.notification-record/v1";
pub const NOTIFICATION_LEDGER_SCHEMA: &str = "central.notification-ledger/v1";
const LEDGER: &str = "ledger.jsonl";
const LOCK: &str = "notifications.lock";
const DEFAULT_LIMIT: usize = 200;
const MAX_LIMIT: usize = 1000;
/// The ledger is read whole; past this size it is read from its tail.
const MAX_READ: u64 = 8 * 1024 * 1024;

/// A provider's refusal of one request, in its own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationRefusal {
    pub status: String,
    pub message: String,
}

/// One kept request: what was asked (the Port's NotificationRequest, verbatim),
/// when, and what the provider answered (its NotificationDelivery) or why it
/// could not deliver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub schema: String,
    pub request_ref: String,
    pub requested_at_unix_ms: u64,
    pub request: NotificationRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<NotificationDelivery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<NotificationRefusal>,
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
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn integer_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "integer".to_owned(),
        required: false,
        choices: None,
        selection: None,
    }
}

fn now() -> io::Result<(u64, u128)> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok((elapsed.as_millis() as u64, elapsed.as_nanos()))
}

fn ledger_path(root: &Path) -> PathBuf {
    root.join(".central").join("notifications").join(LEDGER)
}

/// Append one record to the ledger, under the root's notification lock.
pub fn keep(root: &Path, record: &NotificationRecord) -> io::Result<()> {
    let _lock = crate::source_safety::lock(root, LOCK)?;
    let dir = root.join(".central").join("notifications");
    fs::create_dir_all(&dir)?;
    crate::source_safety::reject_symlink_components(root, Path::new(".central/notifications"))?;
    let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
    line.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(LEDGER))?;
    file.write_all(&line)?;
    file.sync_data()
}

/// The kept records, newest first: at most `limit`, and only those asked at or
/// after `since_unix_ms`. A line that is no record is counted, never dropped
/// silently.
pub fn read_kept(
    root: &Path,
    limit: usize,
    since_unix_ms: Option<u64>,
) -> io::Result<(Vec<NotificationRecord>, usize)> {
    let path = ledger_path(root);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(error) => return Err(error),
    };
    let start = bytes.len().saturating_sub(MAX_READ as usize);
    let text = String::from_utf8_lossy(&bytes[start..]);
    let mut records = Vec::new();
    let mut unreadable = 0;
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<NotificationRecord>(line) {
            Ok(record) if record.schema == NOTIFICATION_RECORD_SCHEMA => {
                if since_unix_ms.is_none_or(|since| record.requested_at_unix_ms >= since) {
                    records.push(record);
                }
            }
            // A line cut by the tail read is not the ledger's fault.
            Err(_) if index == 0 && start > 0 => {}
            _ => unreadable += 1,
        }
    }
    records.reverse();
    records.truncate(limit);
    Ok((records, unreadable))
}

fn notification_failure(
    action: &str,
    error: central_connector_sdk::PortError,
    diagnostics: Value,
    request_ref: Option<&str>,
) -> ActionResult {
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
        Some(
            json!({ "port": USER_NOTIFICATION_PORT.id, "provider_error": error, "diagnostics": diagnostics, "request_ref": request_ref }),
        ),
    )
}

fn root_failure(action: &str, message: String) -> ActionResult {
    ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
}

/// Keep one record where the request was asked, and say whether it was kept.
/// Delivery never waits on the ledger: a request asked with no Central root
/// (or a ledger that cannot be written) is still delivered, and the answer says
/// it was not kept and why.
fn keep_into(root: &Result<PathBuf, String>, record: &NotificationRecord) -> Value {
    match root {
        Ok(root) => match keep(root, record) {
            Ok(()) => json!({ "kept": true }),
            Err(error) => json!({
                "kept": false,
                "kept_error": format!("The notification ledger could not keep the request: {error}"),
            }),
        },
        Err(message) => json!({ "kept": false, "kept_error": message }),
    }
}

fn with_keeping(mut data: Value, keeping: Value) -> Value {
    if let (Some(data), Value::Object(keeping)) = (data.as_object_mut(), keeping) {
        data.extend(keeping);
    }
    data
}

fn personal_notify_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "personal.notify";
    let title = optional_text(input, "title").unwrap_or_default();
    let body = optional_text(input, "body").unwrap_or_default();
    let caller_ref = match required_text(input, "caller_ref", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let provenance_refs = match refs(input, "provenance_refs", ACTION) {
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
    // A request the Port would not accept is refused before anything is kept.
    if let Err(error) = request.validate() {
        return notification_failure(ACTION, error, json!({}), None);
    }
    let root = resolve_central_root(context.root_options).map(|root| root.path);
    let (at, nanos) = match now() {
        Ok(now) => now,
        Err(error) => {
            return ActionResult::failure(
                Some(ACTION),
                ResultStatus::InternalFailure,
                format!("The clock could not date the request: {error}"),
                None,
            )
        }
    };
    let request_ref = format!("central:notification:{nanos}-{}", std::process::id());
    let mut record = NotificationRecord {
        schema: NOTIFICATION_RECORD_SCHEMA.to_owned(),
        request_ref: request_ref.clone(),
        requested_at_unix_ms: at,
        request,
        delivery: None,
        refusal: None,
    };
    let refused = |mut record: NotificationRecord, failed: ActionResult| -> ActionResult {
        record.refusal = Some(NotificationRefusal {
            status: failed.status.as_str().to_owned(),
            message: failed
                .error
                .as_ref()
                .map(|error| error.message.clone())
                .unwrap_or_default(),
        });
        let keeping = keep_into(&root, &record);
        let mut failed = failed;
        if let Some(error) = failed.error.as_mut() {
            error.details = error
                .details
                .take()
                .map(|details| with_keeping(details, keeping));
        }
        failed
    };

    let resolution = context
        .connectors
        .resolve(&USER_NOTIFICATION_PORT, context.connector_context);
    let diagnostics = to_value(&resolution.diagnostics).expect("Connector diagnostics serialise");
    let Some(connector) = resolution.connector else {
        let failed = ActionResult::failure(
            Some(ACTION),
            ResultStatus::UnavailableCapability,
            format!(
                "No eligible Connector implements {}.",
                USER_NOTIFICATION_PORT.id
            ),
            Some(
                json!({ "port": USER_NOTIFICATION_PORT.id, "diagnostics": diagnostics, "request_ref": request_ref }),
            ),
        );
        return refused(record, failed);
    };
    let Some(notification) = connector.user_notification() else {
        let failed = ActionResult::failure(
            Some(ACTION),
            ResultStatus::ConnectorFailure,
            format!(
                "Selected Connector does not expose {} implementation.",
                USER_NOTIFICATION_PORT.id
            ),
            Some(
                json!({ "port": USER_NOTIFICATION_PORT.id, "connector": connector.manifest().id, "diagnostics": diagnostics, "request_ref": request_ref }),
            ),
        );
        return refused(record, failed);
    };
    match notification.deliver(&record.request) {
        Ok(delivery) => {
            record.delivery = Some(delivery.clone());
            let keeping = keep_into(&root, &record);
            ActionResult::success(
                ACTION,
                with_keeping(
                    json!({
                        "request_ref": request_ref,
                        "delivery": delivery,
                        "notification_delivery_is_human_acknowledgement": false,
                        "diagnostics": diagnostics,
                    }),
                    keeping,
                ),
            )
        }
        Err(error) => refused(
            record,
            notification_failure(ACTION, error, diagnostics, Some(&request_ref)),
        ),
    }
}

fn notifications_list_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "personal.notifications.list";
    let limit = match input.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(value) => match value.as_u64() {
            Some(n) if (1..=MAX_LIMIT as u64).contains(&n) => n as usize,
            _ => {
                return ActionResult::failure(
                    Some(ACTION),
                    ResultStatus::InvalidInput,
                    format!("{ACTION} limit must be an integer from 1 to {MAX_LIMIT}."),
                    None,
                )
            }
        },
    };
    let since = match input.get("since_unix_ms") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_u64() {
            Some(n) => Some(n),
            None => {
                return ActionResult::failure(
                    Some(ACTION),
                    ResultStatus::InvalidInput,
                    format!("{ACTION} since_unix_ms must be a non-negative integer."),
                    None,
                )
            }
        },
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root.path,
        Err(message) => return root_failure(ACTION, message),
    };
    let (requests, unreadable) = match read_kept(&root, limit, since) {
        Ok(read) => read,
        Err(error) => {
            return ActionResult::failure(
                Some(ACTION),
                ResultStatus::InternalFailure,
                format!("The notification ledger could not be read: {error}"),
                None,
            )
        }
    };
    // The selected provider's own capabilities; none selected is said, never guessed.
    let resolution = context
        .connectors
        .resolve(&USER_NOTIFICATION_PORT, context.connector_context);
    let diagnostics = to_value(&resolution.diagnostics).expect("Connector diagnostics serialise");
    let (provider, provider_error) = match resolution
        .connector
        .and_then(|connector| connector.user_notification())
    {
        Some(notification) => match notification.capabilities(&NotificationCapabilityRequest) {
            Ok(capabilities) => (Some(capabilities), None),
            Err(error) => (None, Some(error.message)),
        },
        None => (
            None,
            Some(format!(
                "No eligible Connector implements {}.",
                USER_NOTIFICATION_PORT.id
            )),
        ),
    };
    ActionResult::success(
        ACTION,
        json!({
            "schema": NOTIFICATION_LEDGER_SCHEMA,
            "requests": requests,
            "unreadable_records": unreadable,
            "provider": provider,
            "provider_error": provider_error,
            "notification_delivery_is_human_acknowledgement": false,
            "diagnostics": diagnostics,
        }),
    )
}

/// Install the person's notifications: `personal.notify` and
/// `personal.notifications.list`.
pub fn register_notification_actions(registry: &mut ActionRegistry) {
    let mut notify = descriptor(
        "personal.notify",
        "Notify user",
        "Request a provider-neutral user notification and keep the request with its delivery, while keeping delivery distinct from human acknowledgement or approval.",
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
    registry
        .register(notify, personal_notify_action)
        .expect("notification Action ids are valid");

    let mut list = descriptor(
        "personal.notifications.list",
        "List notification requests",
        "Read the kept notification requests, newest first, each with its delivery or refusal, and the selected provider's capabilities. Reading changes nothing.",
        MutationClass::ReadOnly,
        "central.notification-ledger/v1",
    );
    list.inputs = vec![integer_input("limit"), integer_input("since_unix_ms")];
    registry
        .register(list, notifications_list_action)
        .expect("notification Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::{initialize_central, RootOptions};
    use central_connector_sdk::{
        CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
        ConnectorRegistry, NotificationAuthorizationState, NotificationCapabilities,
        NotificationDeliveryState, PortContract, PortError, UserNotification,
        CONNECTOR_API_VERSION,
    };

    struct Provider {
        manifest: ConnectorManifest,
        refuse: bool,
    }

    impl Provider {
        fn new(refuse: bool) -> Self {
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
                refuse,
            }
        }
    }

    impl UserNotification for Provider {
        fn capabilities(
            &self,
            _input: &NotificationCapabilityRequest,
        ) -> Result<NotificationCapabilities, PortError> {
            Ok(NotificationCapabilities {
                available: true,
                authorization: NotificationAuthorizationState::ProviderManaged,
                supports_callback: false,
                supports_urgency: false,
                supports_category: false,
                provider: "fixture".into(),
                notes: vec!["Posting is not seeing.".into()],
            })
        }

        fn deliver(&self, input: &NotificationRequest) -> Result<NotificationDelivery, PortError> {
            if self.refuse {
                return Err(PortError::provider(
                    "The fixture's notification centre rejected the request.",
                ));
            }
            Ok(NotificationDelivery {
                state: NotificationDeliveryState::Posted,
                provider: "fixture".into(),
                subject_ref: input.subject_ref.clone(),
                action_ref: input.action_ref.clone(),
                caller_ref: input.caller_ref.clone(),
                human_acknowledgement_observed: false,
                unsupported_requested_features: input
                    .urgency
                    .as_ref()
                    .map(|_| vec!["urgency".to_owned()])
                    .unwrap_or_default(),
                provenance_refs: input.provenance_refs.clone(),
            })
        }
    }

    impl Connector for Provider {
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
        // Tests run side by side and the clock can read the same instant twice: each root is its own.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let (_, nanos) = now().unwrap();
        let root = std::env::temp_dir().join(format!(
            "central-notification-test-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        initialize_central(&root).unwrap();
        root
    }

    fn run(
        root: &Path,
        connectors: &ConnectorRegistry,
        action: &str,
        input: Value,
    ) -> ActionResult {
        let options = RootOptions {
            explicit_root: Some(root.to_path_buf()),
            configured_root: None,
            home: None,
        };
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors,
            connector_context: &connector_context,
        };
        let mut registry = ActionRegistry::default();
        register_notification_actions(&mut registry);
        registry.execute(action, &input, &context)
    }

    #[test]
    fn a_request_is_kept_with_its_delivery_and_read_back_newest_first_with_the_provider() {
        let root = root();
        let mut connectors = ConnectorRegistry::default();
        connectors.register(Provider::new(false)).unwrap();
        let first = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": "Morning checks failed", "body": "2 of 14", "caller_ref": "routine:morning-checks",
                "subject_ref": "factory:run:1", "urgency": "critical", "category": "routine", "provenance_refs": ["factory:run:1"]}),
        );
        assert!(first.ok, "{first:?}");
        let first_data = first.data.as_ref().unwrap();
        assert_eq!(
            first_data["notification_delivery_is_human_acknowledgement"],
            false
        );
        assert_eq!(first_data["kept"], true);
        let first_ref = first_data["request_ref"].as_str().unwrap().to_owned();
        assert!(first_ref.starts_with("central:notification:"));
        let second = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"body": "Face 5 holds no synthesis yet.", "caller_ref": "central:day"}),
        );
        assert!(second.ok);

        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        assert!(listed.ok, "{listed:?}");
        let data = listed.data.unwrap();
        assert_eq!(data["schema"], NOTIFICATION_LEDGER_SCHEMA);
        let requests = data["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0]["request"]["caller_ref"], "central:day",
            "newest first"
        );
        assert_eq!(requests[1]["request_ref"], first_ref.as_str());
        assert_eq!(requests[1]["request"]["urgency"], "critical");
        assert_eq!(requests[1]["delivery"]["state"], "posted");
        assert_eq!(
            requests[1]["delivery"]["unsupported_requested_features"],
            json!(["urgency"])
        );
        assert_eq!(
            requests[1]["delivery"]["human_acknowledgement_observed"],
            false
        );
        assert_eq!(data["provider"]["provider"], "fixture");
        assert_eq!(data["provider"]["authorization"], "provider_managed");
        assert_eq!(data["unreadable_records"], 0);
        let limited = run(
            &root,
            &connectors,
            "personal.notifications.list",
            json!({"limit": 1}),
        );
        assert_eq!(
            limited.data.unwrap()["requests"].as_array().unwrap().len(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_refused_request_is_kept_with_the_providers_words_and_no_provider_is_said() {
        let root = root();
        let mut refusing = ConnectorRegistry::default();
        refusing.register(Provider::new(true)).unwrap();
        let refused = run(
            &root,
            &refusing,
            "personal.notify",
            json!({"title": "Kev down", "caller_ref": "workcell:mac"}),
        );
        assert!(!refused.ok);
        assert_eq!(refused.status, ResultStatus::ConnectorFailure);
        let details = refused.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["kept"], true);
        assert!(details["request_ref"]
            .as_str()
            .unwrap()
            .starts_with("central:notification:"));
        let none = ConnectorRegistry::default();
        let unavailable = run(
            &root,
            &none,
            "personal.notify",
            json!({"title": "No provider", "caller_ref": "workcell:mac"}),
        );
        assert_eq!(unavailable.status, ResultStatus::UnavailableCapability);

        let listed = run(&root, &none, "personal.notifications.list", json!({}));
        let data = listed.data.unwrap();
        let requests = data["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["refusal"]["status"], "unavailable_capability");
        assert_eq!(
            requests[1]["refusal"]["message"],
            "The fixture's notification centre rejected the request."
        );
        assert!(requests[1].get("delivery").is_none());
        assert!(data["provider"].is_null());
        assert_eq!(
            data["provider_error"],
            "No eligible Connector implements UserNotification."
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_invalid_request_is_refused_before_anything_is_kept_and_reading_rejects_a_bad_limit() {
        let root = root();
        let connectors = ConnectorRegistry::default();
        let empty = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"caller_ref": "x"}),
        );
        assert_eq!(empty.status, ResultStatus::InvalidInput);
        let callback = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": "t", "caller_ref": "x", "callback": "file:///etc"}),
        );
        assert_eq!(callback.status, ResultStatus::InvalidInput);
        assert!(!ledger_path(&root).exists(), "nothing asked, nothing kept");
        let bad = run(
            &root,
            &connectors,
            "personal.notifications.list",
            json!({"limit": 0}),
        );
        assert_eq!(bad.status, ResultStatus::InvalidInput);
        let since = run(
            &root,
            &connectors,
            "personal.notifications.list",
            json!({"since_unix_ms": "soon"}),
        );
        assert_eq!(since.status, ResultStatus::InvalidInput);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_request_with_no_root_is_still_delivered_and_says_it_was_not_kept() {
        let mut connectors = ConnectorRegistry::default();
        connectors.register(Provider::new(false)).unwrap();
        let options = RootOptions {
            explicit_root: None,
            configured_root: None,
            home: None,
        };
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let mut registry = ActionRegistry::default();
        register_notification_actions(&mut registry);
        let delivered = registry.execute(
            "personal.notify",
            &json!({"title": "No root", "caller_ref": "agent:remote"}),
            &context,
        );
        assert!(delivered.ok, "{delivered:?}");
        let data = delivered.data.unwrap();
        assert_eq!(data["delivery"]["state"], "posted");
        assert_eq!(data["kept"], false);
        assert!(data["kept_error"]
            .as_str()
            .unwrap()
            .contains("Central root cannot be resolved"));
        let listed = registry.execute("personal.notifications.list", &json!({}), &context);
        assert_eq!(
            listed.status,
            ResultStatus::InvalidInput,
            "no root, nothing to read"
        );
    }

    #[test]
    fn an_unreadable_line_is_counted_never_dropped_silently() {
        let root = root();
        let dir = root.join(".central").join("notifications");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(LEDGER), "not a record\n").unwrap();
        let connectors = ConnectorRegistry::default();
        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        let data = listed.data.unwrap();
        assert_eq!(data["requests"].as_array().unwrap().len(), 0);
        assert_eq!(data["unreadable_records"], 1);
        let _ = fs::remove_dir_all(root);
    }
}
