use central_ctrl::{
    create_personal_action_registry, initialize_central, ActionExecutionContext, CapabilityProbe,
    Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry,
    MutationClass, NotificationAuthorizationState, NotificationCapabilities,
    NotificationCapabilityRequest, NotificationDelivery, NotificationDeliveryState,
    NotificationRequest, PortContract, PortError, RootOptions, UserNotification,
    CONNECTOR_API_VERSION, USER_NOTIFICATION_PORT,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct NotificationFixture {
    manifest: ConnectorManifest,
    seen: Arc<Mutex<Option<NotificationRequest>>>,
}

impl NotificationFixture {
    fn new(seen: Arc<Mutex<Option<NotificationRequest>>>) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "fixture.oi-watch-notification".to_owned(),
                version: "1".to_owned(),
                display_name: "O:I Watch notification fixture".to_owned(),
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
            },
            seen,
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
            supports_callback: false,
            supports_urgency: false,
            supports_category: true,
            provider: "fixture.oi-watch-notification".into(),
            notes: Vec::new(),
        })
    }

    fn deliver(&self, input: &NotificationRequest) -> Result<NotificationDelivery, PortError> {
        *self.seen.lock().expect("fixture lock") = Some(input.clone());
        Ok(NotificationDelivery {
            state: NotificationDeliveryState::Posted,
            provider: "fixture.oi-watch-notification".into(),
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
        "central-oi-watch-notification-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    initialize_central(&root).expect("initialize Central fixture");
    root
}

#[test]
fn oi_watch_availability_invokes_native_personal_notify_port_without_acknowledgement() {
    let root = root();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        configured_root: None,
        home: None,
    };
    let seen = Arc::new(Mutex::new(None));
    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(NotificationFixture::new(Arc::clone(&seen)))
        .expect("register notification fixture");
    let connector_context = ConnectorContext {
        platform: "test".into(),
    };
    let context = ActionExecutionContext {
        root_options: &options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    let registry = create_personal_action_registry();
    let descriptor = registry.get("personal.notify").expect("personal.notify Action");

    assert_eq!(descriptor.mutation_class, MutationClass::ExternallyMutating);
    assert_eq!(descriptor.required_ports, vec![USER_NOTIFICATION_PORT.id.to_owned()]);
    assert_eq!(USER_NOTIFICATION_PORT.id, "UserNotification");
    assert_eq!(USER_NOTIFICATION_PORT.version, "1.0.0");

    let result = registry.execute(
        "personal.notify",
        &json!({
            "title": "Watched subject available",
            "body": "agent:remote is online in field:watch-availability.",
            "subject_ref": "agent:remote",
            "category": "oi.watch-availability",
            "action_ref": "oi.watch-availability.notify",
            "caller_ref": "notification-decision:watch-availability:42",
            "provenance_refs": [
                "watch:agent:remote",
                "availability:watch:agent:remote:a2a-binding:remote:2",
                "encounter:availability:watch:agent:remote:a2a-binding:remote:2",
                "notification-decision:watch-availability:42",
                "a2a-binding:remote"
            ]
        }),
        &context,
    );

    assert!(result.ok, "personal.notify should execute through the registered UserNotification Port");
    let data = result.data.as_ref().expect("notification data");
    assert_eq!(data["delivery"]["provider"], "fixture.oi-watch-notification");
    assert_eq!(data["delivery"]["subject_ref"], "agent:remote");
    assert_eq!(data["delivery"]["action_ref"], "oi.watch-availability.notify");
    assert_eq!(
        data["delivery"]["caller_ref"],
        "notification-decision:watch-availability:42"
    );
    assert_eq!(data["delivery"]["human_acknowledgement_observed"], false);
    assert_eq!(data["notification_delivery_is_human_acknowledgement"], false);

    let request = seen
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("Port delivery request");
    assert_eq!(request.subject_ref.as_deref(), Some("agent:remote"));
    assert_eq!(request.action_ref.as_deref(), Some("oi.watch-availability.notify"));
    assert_eq!(request.caller_ref, "notification-decision:watch-availability:42");
    assert_eq!(request.provenance_refs.len(), 5);
    assert!(request
        .provenance_refs
        .iter()
        .any(|value| value.starts_with("encounter:availability:")));

    let _ = std::fs::remove_dir_all(root);
}
