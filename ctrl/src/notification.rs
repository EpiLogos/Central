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
/// The refusal of a request whose title or body is given and shows nothing.
const SHOWS_NOTHING: &str = "Notification title and body hold only characters that do not show.";
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

/// The request as the provider is handed it to show: its title and body are a
/// caller's words, and the operating system draws them as given, so they go
/// without the characters that draw nothing (`names::shown`; be2j ruling 2)
/// and without what a notification centre must not be handed (`shown_words`).
/// The ledger keeps the request as it was asked. The request's other words
/// (`subject_ref`, `category`, `action_ref`, `callback`) go as asked: no
/// provider draws them.
fn shown_request(request: &NotificationRequest) -> NotificationRequest {
    NotificationRequest {
        title: shown_words(&request.title, ' '),
        body: shown_words(&request.body, '\n'),
        ..request.clone()
    }
}

/// A caller's words as a notification shows them (DLW build rulings 2026-11-10,
/// be2j.c2 item 2, and 2026-11-11, be2j.c3 item 3), cleaned before the shared
/// `names::shown`:
/// - what parts words is not taken out, or the words either side would fuse:
///   a line or paragraph separator (U+2028, U+2029), a vertical tab, a form
///   feed, a next line (U+0085) and a \r that no \n follows each become
///   `separator`, a space in a title and a line break in a body. A \r before
///   its \n goes, and the \n is the line break;
/// - an ANSI escape sequence is taken out whole (`escape_end`), and an ESC
///   that opens none takes only itself and the character that named its form;
/// - any other control character but a line break and a tab is taken out. The
///   one-character openings U+009B and U+009D open nothing: each is a control
///   like any other, so the words after one stay.
fn shown_words(text: &str, separator: char) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut plain = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '\u{1b}' {
            at = escape_end(&chars, at);
            continue;
        }
        at += 1;
        match c {
            '\u{2028}' | '\u{2029}' | '\u{0b}' | '\u{0c}' | '\u{85}' => plain.push(separator),
            '\r' if chars.get(at) != Some(&'\n') => plain.push(separator),
            '\n' | '\t' => plain.push(c),
            c if !c.is_control() => plain.push(c),
            _ => {}
        }
    }
    crate::names::shown(&plain)
}

/// Where the escape that the ESC at `at` opens ends: the place to read on
/// from. The rules are the ones AIKit's cleaning of a run's reason keeps
/// (be2j.a2 item 4), so both sides read one text alike, and where a sequence
/// is not whole the words a person would read stay:
/// - a CSI in its strict form goes whole: ESC `[`, parameter characters
///   (U+0030–003F), then intermediate ones (U+0020–002F), then one final
///   character (U+0040–007E). Anything else after ESC `[` is not a CSI, and
///   only the ESC and the `[` go;
/// - a string sequence (OSC ESC `]`, DCS ESC `P`, APC ESC `_`, PM ESC `^`,
///   SOS ESC `X`) goes whole when it is ended on its own line: by ESC `\`, by
///   a BEL for an OSC, or by another ESC, which is then read as an opening of
///   its own. Its line ends where the shown words break: at a \n, a \r, a
///   vertical tab, a form feed, a next line, a line or paragraph separator.
///   Without an end only its two opening characters go;
/// - ESC, any intermediate characters (U+0020–002F) and one final character
///   (U+0030–007E) goes whole: with none between, a two-character escape
///   (ESC `7`, ESC `c`, ESC `=`), its letter with it (be2j.c3 item 3); with
///   some, a character set selection and the like;
/// - any other ESC goes alone.
fn escape_end(chars: &[char], at: usize) -> usize {
    let run = |from: usize, low: char, high: char| {
        from + chars[from.min(chars.len())..]
            .iter()
            .take_while(|c| (low..=high).contains(*c))
            .count()
    };
    let is =
        |at: usize, low: char, high: char| chars.get(at).is_some_and(|c| (low..=high).contains(c));
    match chars.get(at + 1) {
        Some('[') => {
            let end = run(run(at + 2, '\u{30}', '\u{3f}'), '\u{20}', '\u{2f}');
            if is(end, '\u{40}', '\u{7e}') {
                end + 1
            } else {
                at + 2
            }
        }
        Some(&form @ (']' | 'P' | '_' | '^' | 'X')) => {
            for (end, &c) in chars.iter().enumerate().skip(at + 2) {
                match c {
                    '\u{07}' if form == ']' => return end + 1,
                    '\u{1b}' if chars.get(end + 1) == Some(&'\\') => return end + 2,
                    '\u{1b}' => return end,
                    '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => break,
                    _ => {}
                }
            }
            at + 2
        }
        _ => {
            let end = run(at + 1, '\u{20}', '\u{2f}');
            if is(end, '\u{30}', '\u{7e}') {
                end + 1
            } else {
                at + 1
            }
        }
    }
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
    // A request the Port would not accept is refused before anything is kept;
    // so is one whose words show nothing once cleaned (`shown_request`), in
    // its own words: its title or body is not empty, it only shows nothing.
    let shows_nothing = || {
        let shown = shown_request(&request);
        (shown.title.is_empty() && shown.body.is_empty()).then(|| {
            central_connector_sdk::PortError::new(PortErrorCode::InvalidInput, SHOWS_NOTHING)
        })
    };
    if let Some(error) = request.validate().err().or_else(shows_nothing) {
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
    match notification.deliver(&shown_request(&record.request)) {
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
        /// The title and body of each request the provider was handed to show.
        delivered: std::sync::Mutex<Vec<(String, String)>>,
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
                delivered: std::sync::Mutex::new(Vec::new()),
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
            self.delivered
                .lock()
                .unwrap()
                .push((input.title.clone(), input.body.clone()));
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

    /// be2j ruling 2 (DLW build ruling 2026-11-09): what the operating system
    /// shows of a request is its words without the characters that draw nothing;
    /// the ledger keeps them as asked, and a request that would show nothing is
    /// refused before anything is kept.
    #[test]
    fn a_requests_words_are_shown_without_what_draws_nothing_and_kept_as_asked() {
        let root = root();
        let provider = std::sync::Arc::new(Provider::new(false));
        struct Shared(std::sync::Arc<Provider>);
        impl Connector for Shared {
            fn manifest(&self) -> &ConnectorManifest {
                self.0.manifest()
            }
            fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
                self.0.probe(port, context)
            }
            fn user_notification(&self) -> Option<&dyn UserNotification> {
                Some(self.0.as_ref())
            }
        }
        let mut connectors = ConnectorRegistry::default();
        connectors.register(Shared(provider.clone())).unwrap();
        let title = "Checks \u{202E}deliaf\u{202C}\u{200B}";
        let asked = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": title, "body": "2\u{2066} of\u{2069} 14\u{3164}", "caller_ref": "routine:checks"}),
        );
        assert!(asked.ok, "{asked:?}");
        assert_eq!(
            *provider.delivered.lock().unwrap(),
            vec![("Checks deliaf".to_owned(), "2 of 14".to_owned())]
        );
        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        let kept = listed.data.unwrap()["requests"][0]["request"].clone();
        assert_eq!(kept["title"], title, "the ledger keeps the words as asked");
        let blank = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": "\u{200B}\u{2066}", "body": "\u{3164}", "caller_ref": "routine:checks"}),
        );
        assert!(!blank.ok, "words that show nothing are refused");
        assert_eq!(
            blank.error.unwrap().message,
            "Notification title and body hold only characters that do not show."
        );
        assert_eq!(
            provider.delivered.lock().unwrap().len(),
            1,
            "and nothing is shown"
        );
        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        assert_eq!(
            listed.data.unwrap()["requests"].as_array().unwrap().len(),
            1,
            "nor kept"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// be2j.c2 item 2 (DLW build ruling 2026-11-10): the reviewer's notification
    /// probe as assertions. A line or paragraph separator parts words, so it
    /// stays as a space in a title and a line break in a body; an ANSI escape
    /// sequence goes whole; no other control but a line break and a tab reaches
    /// the provider; and a request that shows nothing is refused in its own
    /// words, with nothing shown and nothing kept.
    #[test]
    fn a_request_reaches_the_provider_without_escapes_or_controls_and_its_separators_part_words() {
        let root = root();
        let provider = std::sync::Arc::new(Provider::new(false));
        struct Shared(std::sync::Arc<Provider>);
        impl Connector for Shared {
            fn manifest(&self) -> &ConnectorManifest {
                self.0.manifest()
            }
            fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
                self.0.probe(port, context)
            }
            fn user_notification(&self) -> Option<&dyn UserNotification> {
                Some(self.0.as_ref())
            }
        }
        let mut connectors = ConnectorRegistry::default();
        connectors.register(Shared(provider.clone())).unwrap();
        // (title, body) as asked, then as the provider is handed them.
        type Words = (&'static str, &'static str);
        let rows: Vec<(Words, Words)> = vec![
            // The reviewer's rows: separators fused words, an escape went through.
            (("A\u{2028}B", "para\u{2029}sep"), ("A B", "para\nsep")),
            (("ESC", "a\u{1b}[31mred\u{1b}[0m"), ("ESC", "ared")),
            // Each separator in the other word too.
            (("A\u{2029}B", "line\u{2028}sep"), ("A B", "line\nsep")),
            // An escape in a title; a CSI with a private parameter and with an intermediate character.
            (
                ("\u{1b}[1;31mRed\u{1b}[0m alert", "a\u{1b}[?25lb\u{1b}[0 qc"),
                ("Red alert", "abc"),
            ),
            // What is not a CSI in its strict form keeps its words: only the ESC and the [ go. A one-character opening
            // (U+009B, U+009D) opens nothing and goes as the control it is.
            (
                (
                    "\u{1b}[ 12 tests failed",
                    "cost \u{9b} 12 345 euros, note \u{9d} kept",
                ),
                ("12 tests failed", "cost  12 345 euros, note  kept"),
            ),
            // A CSI cut short is not one either.
            (("Cut", "a\u{1b}[31"), ("Cut", "a31")),
            // OSC, ended by a BEL and by ESC \: the whole sequence goes, its text with it. U+009C ends nothing.
            (
                (
                    "\u{1b}]0;window title\u{07}Done",
                    "a\u{1b}]8;;https://example.test\u{1b}\\link\u{9c}b",
                ),
                ("Done", "alinkb"),
            ),
            // An OSC ended by the ESC of a sequence after it goes whole, and that sequence is read as its own.
            (
                ("Next", "boom \u{1b}]0;t\u{1b}[31mred\u{1b}[0m"),
                ("Next", "boom red"),
            ),
            // An OSC that nothing ends on its line loses its opening, never the words after it; an end on a later line
            // is not its end.
            (
                (
                    "\u{1b}]0;never ended",
                    "a\u{1b}]0;first line\nsecond\u{07} line",
                ),
                ("0;never ended", "a0;first line\nsecond line"),
            ),
            // The other string sequences (DCS, APC, PM, SOS) go whole when ESC \ ends them; a BEL ends only an OSC.
            (
                (
                    "\u{1b}Pq#0\u{1b}\\Done, \u{1b}Pnot\u{07} ended",
                    "a\u{1b}_hidden\u{1b}\\b\u{1b}^x\u{1b}\\c\u{1b}Xy\u{1b}\\d",
                ),
                ("Done, not ended", "abcd"),
            ),
            // A character set selection goes whole; so does an ESC and one final character, its letter with it (be2j.c3
            // item 3).
            (
                ("Set", "text \u{1b}(B more, f\u{1b}g"),
                ("Set", "text  more, f"),
            ),
            // So does any ESC, intermediate characters, one final character: the final may be a digit.
            (("\u{1b}(0Line", "p\u{1b}#8q\u{1b} Fr"), ("Line", "pqr")),
            // Each line end closes the line a string sequence may end on: a \r, a line and a paragraph separator. The \r
            // parts the words either side of it (be2j.c3 item 3).
            (
                (
                    "\u{1b}]0;a\rb\u{07}c",
                    "x\u{1b}]0;a\u{2028}b\u{07}c\u{1b}]0;d\u{2029}e\u{07}f",
                ),
                ("0;a bc", "x0;a\nbc0;d\nef"),
            ),
            // Any other control goes; a line break and a tab stay, a \r goes from before its \n. A next line (U+0085) parts
            // words as a separator does (be2j.c3 item 3).
            (
                ("Bell\u{07} and\u{0} null", "a\u{08}b\u{7f}c\u{85}d\r\ne\tf"),
                ("Bell and null", "abc\nd\ne\tf"),
            ),
            // A zero-width joiner and non-joiner are taken out as ruled (be2j rulings 2 and 9): unchanged.
            (
                ("Fam\u{200D}ily", "\u{0633}\u{200C}\u{062F}"),
                ("Family", "\u{0633}\u{062F}"),
            ),
        ];
        let last = *rows.last().unwrap();
        for &((title, body), shown) in &rows {
            let asked = run(
                &root,
                &connectors,
                "personal.notify",
                json!({"title": title, "body": body, "caller_ref": "routine:probe"}),
            );
            assert!(asked.ok, "{title:?} {body:?}: {asked:?}");
            let delivered = provider.delivered.lock().unwrap().last().cloned().unwrap();
            assert_eq!(
                (delivered.0.as_str(), delivered.1.as_str()),
                shown,
                "{title:?} {body:?}"
            );
        }
        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        let kept = listed.data.unwrap()["requests"][0]["request"].clone();
        assert_eq!(
            (kept["title"].as_str(), kept["body"].as_str()),
            (Some(last.0 .0), Some(last.0 .1)),
            "the ledger keeps the words as asked"
        );
        // Given, and showing nothing: refused in its own words, not as an empty request.
        for (title, body) in [
            ("\u{200B}", "\u{200B}"),
            ("\u{1b}[0m", ""),
            ("", "\u{2028}\u{07}"),
        ] {
            let blank = run(
                &root,
                &connectors,
                "personal.notify",
                json!({"title": title, "body": body, "caller_ref": "routine:probe"}),
            );
            assert!(!blank.ok, "{title:?} {body:?}");
            assert_eq!(
                blank.error.unwrap().message,
                "Notification title and body hold only characters that do not show.",
                "{title:?} {body:?}"
            );
        }
        // A request with no words at all is still the Port's own refusal.
        let empty = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": " ", "body": "", "caller_ref": "routine:probe"}),
        );
        assert_eq!(
            empty.error.unwrap().message,
            "Notification requires a non-empty title or body."
        );
        assert_eq!(
            provider.delivered.lock().unwrap().len(),
            rows.len(),
            "nothing refused is shown"
        );
        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        assert_eq!(
            listed.data.unwrap()["requests"].as_array().unwrap().len(),
            rows.len(),
            "nor kept"
        );
        // One visible word is enough: the other may show nothing.
        let half = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": "Done", "body": "\u{200B}", "caller_ref": "routine:probe"}),
        );
        assert!(half.ok, "{half:?}");
        assert_eq!(
            provider.delivered.lock().unwrap().last().cloned().unwrap(),
            ("Done".to_owned(), String::new())
        );
        let _ = fs::remove_dir_all(root);
    }

    /// A provider that keeps what it was handed, and the registry it stands in.
    fn kept_provider() -> (std::sync::Arc<Provider>, ConnectorRegistry) {
        struct Shared(std::sync::Arc<Provider>);
        impl Connector for Shared {
            fn manifest(&self) -> &ConnectorManifest {
                self.0.manifest()
            }
            fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
                self.0.probe(port, context)
            }
            fn user_notification(&self) -> Option<&dyn UserNotification> {
                Some(self.0.as_ref())
            }
        }
        let provider = std::sync::Arc::new(Provider::new(false));
        let mut connectors = ConnectorRegistry::default();
        connectors.register(Shared(provider.clone())).unwrap();
        (provider, connectors)
    }

    /// be2j.c3 item 3 (DLW build ruling 2026-11-11): what the cleaning leaves of words a person reads. A lone \r, a vertical
    /// tab, a form feed and a next line part words as a separator does, so the words either side do not fuse; a two-character
    /// escape goes whole, its letter with it; U+009B and U+009D open nothing, so the words after one stay; and an opening that
    /// is whole by the rules takes what a terminal would take, up to its end.
    #[test]
    fn a_requests_words_stay_parted_and_a_short_escape_goes_whole() {
        let root = root();
        let (provider, connectors) = kept_provider();
        // The words as asked, then as a title and as a body show them.
        let rows: Vec<(&str, &str, &str)> = vec![
            // A \r that no \n follows parts words: a space in a title, a line break in a body.
            ("Step 1\rStep 2", "Step 1 Step 2", "Step 1\nStep 2"),
            // A \r before its \n goes, and the \n is the one line break; two \r are two.
            ("one\r\ntwo", "one\ntwo", "one\ntwo"),
            ("one\r\r\ntwo", "one \ntwo", "one\n\ntwo"),
            // A vertical tab, a form feed, a next line: each parts words.
            ("a\u{0b}b", "a b", "a\nb"),
            ("a\u{0c}b", "a b", "a\nb"),
            ("a\u{85}b", "a b", "a\nb"),
            // A two-character escape goes whole: save and restore the cursor, reverse index, reset, keypad mode, and the
            // string end that ends nothing.
            (
                "a\u{1b}7b\u{1b}8c\u{1b}Md\u{1b}ce\u{1b}=f\u{1b}\\g",
                "abcdefg",
                "abcdefg",
            ),
            // An ESC that no final character follows goes alone: at the end, before a character past U+007E, and before an
            // intermediate character that no final one follows.
            ("abc\u{1b}", "abc", "abc"),
            ("a\u{1b}\u{e9}b", "a\u{e9}b", "a\u{e9}b"),
            ("a\u{1b}!\u{e9}b", "a!\u{e9}b", "a!\u{e9}b"),
            // U+009B and U+009D open nothing: each goes as the control it is, and what follows stays.
            ("a\u{9b}31mred", "a31mred", "a31mred"),
            ("a\u{9d}0;title\u{07}b", "a0;titleb", "a0;titleb"),
            // An opening that is whole takes what a terminal would: a DCS to the next ESC, a CSI to its final character.
            (
                "Press \u{1b}P to pause, \u{1b}[1mnow",
                "Press now",
                "Press now",
            ),
            ("\u{1b}[12 tests failed", "ests failed", "ests failed"),
            // A string sequence nothing ends on its line loses its opening only. Its line ends at each character that
            // parts lines: an end past one is not its end.
            ("a\u{1b}]0;never ended", "a0;never ended", "a0;never ended"),
            ("a\u{1b}]0;t\u{0b}b\u{07}c", "a0;t bc", "a0;t\nbc"),
            ("a\u{1b}]0;t\u{0c}b\u{07}c", "a0;t bc", "a0;t\nbc"),
            ("a\u{1b}]0;t\u{85}b\u{07}c", "a0;t bc", "a0;t\nbc"),
        ];
        for &(asked, title, body) in &rows {
            let sent = run(
                &root,
                &connectors,
                "personal.notify",
                json!({"title": asked, "body": asked, "caller_ref": "routine:probe"}),
            );
            assert!(sent.ok, "{asked:?}: {sent:?}");
            let delivered = provider.delivered.lock().unwrap().last().cloned().unwrap();
            assert_eq!(
                (delivered.0.as_str(), delivered.1.as_str()),
                (title, body),
                "{asked:?}"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    /// The sha256 of `escape-cases.json`, the cases AIKit holds too (`crates/aikit-cli/src/escape-cases.json`, read by its
    /// day-move runner's tests), byte for byte: a rule that moves on one side fails that side's test of the cases, and a file
    /// that moves on one side without the other fails here or there (be2j.a3 item 5, DLW build ruling 2026-11-11).
    const ESCAPE_CASES_SHA256: &str =
        "c2d19384dfacf5f39610c48494b2a45ba4010a655c549d30945844edd2ea9db8";

    #[test]
    fn a_notifications_words_are_cleaned_of_escapes_as_the_cases_aikit_holds_too_say() {
        use sha2::{Digest, Sha256};
        let file = include_str!("escape-cases.json");
        assert_eq!(
            format!("{:x}", Sha256::digest(file.as_bytes())),
            ESCAPE_CASES_SHA256,
            "the cases are the ones AIKit holds"
        );
        let cases: Value = serde_json::from_str(file).unwrap();
        let cases = cases["cases"].as_array().unwrap();
        assert!(cases.len() >= 30, "{}", cases.len());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            // A title is the words on one line, a body the words in a block.
            assert_eq!(
                shown_words(text, ' '),
                case["line"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                shown_words(text, '\n'),
                case["block"].as_str().unwrap(),
                "{name}"
            );
        }
    }

    /// be2j.c3 item 2 (DLW build ruling 2026-11-11): a request of white space alone is the Port's empty request, a line or a
    /// paragraph separator included, and gets the Port's words; nothing is shown and nothing is kept. The words for a request
    /// that shows nothing are for one that holds something else.
    #[test]
    fn a_request_of_white_space_alone_is_the_ports_empty_request() {
        let root = root();
        let (provider, connectors) = kept_provider();
        for (title, body) in [
            ("", "\u{2028}"),
            ("\u{2028}", "\u{2029}"),
            (" \t", "\n"),
            ("\r\n", "\u{85}\u{0b}\u{0c}"),
            ("\u{a0}\u{3000}", ""),
        ] {
            let asked = run(
                &root,
                &connectors,
                "personal.notify",
                json!({"title": title, "body": body, "caller_ref": "routine:probe"}),
            );
            assert!(!asked.ok, "{title:?} {body:?}");
            assert_eq!(
                asked.error.unwrap().message,
                "Notification requires a non-empty title or body.",
                "{title:?} {body:?}"
            );
        }
        // White space beside a character that draws nothing is not empty as given: it shows nothing.
        let hidden = run(
            &root,
            &connectors,
            "personal.notify",
            json!({"title": " \u{200B} ", "body": "\u{2028}", "caller_ref": "routine:probe"}),
        );
        assert_eq!(hidden.error.unwrap().message, SHOWS_NOTHING);
        assert!(
            provider.delivered.lock().unwrap().is_empty(),
            "nothing is shown"
        );
        let listed = run(&root, &connectors, "personal.notifications.list", json!({}));
        assert!(
            listed.data.unwrap()["requests"]
                .as_array()
                .unwrap()
                .is_empty(),
            "nor kept"
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
