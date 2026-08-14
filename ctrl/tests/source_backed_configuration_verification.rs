use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, CapabilityProbe,
    ConfigurationManager, ConfigurationStateRequest, Connector, ConnectorContext,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, MachineInspectionInput,
    MachineInspectionOutput, MachineInspector, ObservedConfiguration, PortContract, PortError,
    ResultStatus, RootOptions, StateChangePreview, StateChangeResult, CONFIGURATION_MANAGER_PORT,
    CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-source-config-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_role(root: &PathBuf) {
    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": "test-role",
        "capabilities": [],
        "requirements": {
            "packages": [],
            "configurations": [{
                "id": "fixture-config",
                "state": "present",
                "source": { "kind": "fixture", "reference": "authored-source-v1" }
            }],
            "services": []
        }
    });
    fs::write(
        root.join("Control/machines/test-role.json"),
        serde_json::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
}

#[derive(Clone)]
struct SourceState {
    matches_authored_source: Arc<Mutex<bool>>,
}

impl SourceState {
    fn new(matches_authored_source: bool) -> Self {
        Self {
            matches_authored_source: Arc::new(Mutex::new(matches_authored_source)),
        }
    }

    fn matches(&self) -> bool {
        *self.matches_authored_source.lock().unwrap()
    }

    fn set_matches(&self, value: bool) {
        *self.matches_authored_source.lock().unwrap() = value;
    }
}

struct SourceAwareConfigurationConnector {
    manifest: ConnectorManifest,
    state: SourceState,
}

impl SourceAwareConfigurationConnector {
    fn new(matches_authored_source: bool) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "test.source-aware-configuration".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Source-aware configuration fixture".to_owned(),
                ports: [MACHINE_INSPECTOR_PORT, CONFIGURATION_MANAGER_PORT]
                    .iter()
                    .map(|port| ConnectorPortDeclaration {
                        id: port.id.to_owned(),
                        version: port.version.to_owned(),
                    })
                    .collect(),
                platforms: vec!["test".to_owned()],
                entrypoint: "test:source-aware-configuration".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "locally-mutating".to_owned(),
            },
            state: SourceState::new(matches_authored_source),
        }
    }
}

impl MachineInspector for SourceAwareConfigurationConnector {
    fn inspect(&self, _input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        Ok(MachineInspectionOutput {
            platform: "test-os".to_owned(),
            architecture: "test-arch".to_owned(),
            capabilities: Vec::new(),
            packages: Vec::new(),
            configurations: vec![ObservedConfiguration {
                id: "fixture-config".to_owned(),
                present: true,
            }],
            services: Vec::new(),
        })
    }
}

impl ConfigurationManager for SourceAwareConfigurationConnector {
    fn preview(&self, input: &ConfigurationStateRequest) -> Result<StateChangePreview, PortError> {
        assert_eq!(input.id, "fixture-config");
        assert!(input.present);
        let source = input.source.as_ref().expect("source-backed request must preserve source");
        assert_eq!(source.kind, "fixture");
        assert_eq!(source.reference, "authored-source-v1");
        let changed = !self.state.matches();
        Ok(StateChangePreview {
            changed,
            summary: if changed {
                "fixture-config differs from authored source".to_owned()
            } else {
                "fixture-config matches authored source".to_owned()
            },
        })
    }

    fn apply(&self, input: &ConfigurationStateRequest) -> Result<StateChangeResult, PortError> {
        let changed = !self.state.matches();
        if input.present {
            self.state.set_matches(true);
        }
        Ok(StateChangeResult {
            changed,
            summary: "fixture-config reconciled to authored source".to_owned(),
        })
    }
}

impl Connector for SourceAwareConfigurationConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }

    fn configuration_manager(&self) -> Option<&dyn ConfigurationManager> {
        Some(self)
    }
}

fn execute(
    root: &PathBuf,
    connectors: &ConnectorRegistry,
    action: &str,
) -> central_ctrl::ActionResult {
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors,
        connector_context: &connector_context,
    };
    create_core_action_registry().execute(action, &json!({ "role": "test-role" }), &context)
}

#[test]
fn source_backed_configuration_drift_is_not_hidden_by_target_presence() {
    let fixture = temporary_directory("drift");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_role(&root);

    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(SourceAwareConfigurationConnector::new(false))
        .unwrap();

    let plan = execute(&root, &connectors, "machine.plan");
    assert_eq!(plan.status, ResultStatus::Success);
    let data = plan.data.unwrap();
    let config = data["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "configuration")
        .unwrap();
    assert_eq!(config["observed"]["present"], true);
    assert_eq!(config["status"], "changeable");
    assert_eq!(config["port"], CONFIGURATION_MANAGER_PORT.id);
    assert_eq!(
        config["connector"]["id"],
        "test.source-aware-configuration"
    );
    assert_eq!(config["preview"]["changed"], true);

    let verify = execute(&root, &connectors, "machine.verify");
    assert_eq!(verify.status, ResultStatus::VerificationFailure);

    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn apply_reconciles_source_content_then_verifies_and_repeats_stably() {
    let fixture = temporary_directory("apply");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_role(&root);

    let connector = SourceAwareConfigurationConnector::new(false);
    let state = connector.state.clone();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();

    let first = execute(&root, &connectors, "machine.apply");
    assert_eq!(first.status, ResultStatus::Success);
    let data = first.data.unwrap();
    assert_eq!(data["outcome"], "complete");
    assert_eq!(data["operations"].as_array().unwrap().len(), 1);
    assert_eq!(data["operations"][0]["kind"], "configuration");
    assert_eq!(data["operations"][0]["port"], CONFIGURATION_MANAGER_PORT.id);
    assert_eq!(data["verification"]["satisfied"], true);
    assert!(state.matches());

    let repeated = execute(&root, &connectors, "machine.apply");
    assert_eq!(repeated.status, ResultStatus::Success);
    let repeated = repeated.data.unwrap();
    assert_eq!(repeated["outcome"], "complete");
    assert_eq!(repeated["operations"].as_array().unwrap().len(), 0);
    assert_eq!(repeated["verification"]["satisfied"], true);

    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn source_backed_configuration_is_unverifiable_without_configuration_manager() {
    let fixture = temporary_directory("unavailable");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_role(&root);

    let observation = MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: Vec::new(),
        packages: Vec::new(),
        configurations: vec![ObservedConfiguration {
            id: "fixture-config".to_owned(),
            present: true,
        }],
        services: Vec::new(),
    };
    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(central_ctrl::StaticMachineInspectorConnector::new(observation))
        .unwrap();

    let plan = execute(&root, &connectors, "machine.plan");
    assert_eq!(plan.status, ResultStatus::Success);
    let data = plan.data.unwrap();
    let config = data["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "configuration")
        .unwrap();
    assert_eq!(config["status"], "missing");
    assert_eq!(config["port"], CONFIGURATION_MANAGER_PORT.id);
    assert!(config["reason"]
        .as_str()
        .unwrap()
        .contains("cannot be verified"));

    let verify = execute(&root, &connectors, "machine.verify");
    assert_eq!(verify.status, ResultStatus::VerificationFailure);

    fs::remove_dir_all(fixture).unwrap();
}
