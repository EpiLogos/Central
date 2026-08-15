use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorManifest, ConnectorPortDeclaration, NativeOpen,
    NativeOpenInput, NativeOpenOutput, NativeReveal, NativeRevealInput, NativeRevealOutput,
    PortContract, PortError, CONNECTOR_API_VERSION, NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT,
};
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorContext,
    ConnectorRegistry, FilesystemWorkConnector, ResultStatus, RootOptions, WORK_DISCOVERY_PORT,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestNativeConnector {
    manifest: ConnectorManifest,
}

impl TestNativeConnector {
    fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "test.native".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Test native host".to_owned(),
                ports: [NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT]
                    .iter()
                    .map(|port| ConnectorPortDeclaration {
                        id: port.id.to_owned(),
                        version: port.version.to_owned(),
                    })
                    .collect(),
                platforms: vec!["test".to_owned()],
                entrypoint: "test:TestNativeConnector".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "externally-mutating".to_owned(),
            },
        }
    }
}

impl NativeOpen for TestNativeConnector {
    fn open(&self, input: &NativeOpenInput) -> Result<NativeOpenOutput, PortError> {
        Ok(NativeOpenOutput {
            target: input.target.clone(),
        })
    }
}

impl NativeReveal for TestNativeConnector {
    fn reveal(&self, input: &NativeRevealInput) -> Result<NativeRevealOutput, PortError> {
        Ok(NativeRevealOutput {
            target: input.target.clone(),
        })
    }
}

impl Connector for TestNativeConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform != "test" {
            return CapabilityProbe::unavailable("test Connector only supports the test platform");
        }
        if self
            .manifest
            .ports
            .iter()
            .any(|declaration| declaration.id == port.id && declaration.version == port.version)
        {
            CapabilityProbe::available()
        } else {
            CapabilityProbe::unavailable("unsupported Port")
        }
    }

    fn native_open(&self) -> Option<&dyn NativeOpen> {
        Some(self)
    }

    fn native_reveal(&self) -> Option<&dyn NativeReveal> {
        Some(self)
    }
}

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-work-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn test_connectors() -> ConnectorRegistry {
    let mut connectors = ConnectorRegistry::default();
    connectors.register(FilesystemWorkConnector::new()).unwrap();
    connectors.register(TestNativeConnector::new()).unwrap();
    connectors
}

fn execute(
    root: &PathBuf,
    action: &str,
    input: serde_json::Value,
) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let connectors = test_connectors();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    registry.execute(action, &input, &context)
}

#[test]
fn work_entry_actions_declare_the_public_ports_they_actually_use() {
    let registry = create_core_action_registry();
    assert_eq!(
        registry.get("work.list").unwrap().required_ports,
        vec![WORK_DISCOVERY_PORT.id]
    );
    assert_eq!(
        registry.get("work.search").unwrap().required_ports,
        vec![WORK_DISCOVERY_PORT.id]
    );
    assert_eq!(
        registry.get("work.open").unwrap().required_ports,
        vec![WORK_DISCOVERY_PORT.id, NATIVE_OPEN_PORT.id]
    );
    assert_eq!(
        registry.get("work.reveal").unwrap().required_ports,
        vec![WORK_DISCOVERY_PORT.id, NATIVE_REVEAL_PORT.id]
    );
}

#[test]
fn search_open_and_reveal_operate_on_ordinary_directories_without_project_metadata() {
    let root = temporary_directory("ordinary").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("alpha-notes")).unwrap();
    fs::create_dir(root.join("Work").join("beta")).unwrap();

    let search = execute(&root, "work.search", json!({ "query": "alpha" }));
    assert_eq!(search.status, ResultStatus::Success);
    assert_eq!(
        search.data.as_ref().unwrap()["matches"][0]["name"],
        "alpha-notes"
    );

    let open = execute(&root, "work.open", json!({ "query": "alpha" }));
    assert_eq!(open.status, ResultStatus::Success);
    assert_eq!(open.data.as_ref().unwrap()["item"]["name"], "alpha-notes");
    assert_eq!(open.data.as_ref().unwrap()["match"], "search");
    assert_eq!(
        open.data.as_ref().unwrap()["native"]["port"],
        NATIVE_OPEN_PORT.id
    );
    assert_eq!(
        open.data.as_ref().unwrap()["native"]["diagnostics"]["selected_connector"]["id"],
        "test.native"
    );

    let reveal = execute(&root, "work.reveal", json!({ "query": "alpha-notes" }));
    assert_eq!(reveal.status, ResultStatus::Success);
    assert_eq!(
        reveal.data.as_ref().unwrap()["item"]["name"],
        "alpha-notes"
    );
    assert_eq!(
        reveal.data.as_ref().unwrap()["native"]["port"],
        NATIVE_REVEAL_PORT.id
    );

    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(root.join("Work").join("alpha-notes"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn exact_name_wins_and_new_directories_are_visible_immediately() {
    let root = temporary_directory("exact").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("alpha")).unwrap();
    fs::create_dir(root.join("Work").join("alpha-tools")).unwrap();

    let exact = execute(&root, "work.open", json!({ "query": "alpha" }));
    assert_eq!(exact.status, ResultStatus::Success);
    assert_eq!(exact.data.as_ref().unwrap()["item"]["name"], "alpha");
    assert_eq!(exact.data.as_ref().unwrap()["match"], "exact");

    fs::create_dir(root.join("Work").join("gamma")).unwrap();
    let later = execute(&root, "work.reveal", json!({ "query": "gamma" }));
    assert_eq!(later.status, ResultStatus::Success);
    assert_eq!(later.data.as_ref().unwrap()["item"]["name"], "gamma");
}

#[test]
fn ambiguous_and_missing_work_selection_return_structured_invalid_input_before_native_invocation() {
    let root = temporary_directory("failure").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("alpha-one")).unwrap();
    fs::create_dir(root.join("Work").join("alpha-two")).unwrap();

    let ambiguous = execute(&root, "work.open", json!({ "query": "alpha" }));
    assert_eq!(ambiguous.status, ResultStatus::InvalidInput);
    assert_eq!(
        ambiguous
            .error
            .unwrap()
            .details
            .unwrap()["matches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let missing = execute(&root, "work.reveal", json!({ "query": "omega" }));
    assert_eq!(missing.status, ResultStatus::InvalidInput);
    assert_eq!(
        missing
            .error
            .unwrap()
            .details
            .unwrap()["matches"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
