use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext, CapabilityProbe,
    CliEnvironment, ConfigurationManager, ConfigurationStateRequest, Connector, ConnectorContext,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, MachineInspectionOutput,
    ObservedConfiguration, ObservedPackage, ObservedService, PackageManager, PackageStateRequest,
    PortContract, PortError, ResultStatus, RootOptions, ServiceManager, ServiceStateRequest,
    StateChangePreview, StateChangeResult, StaticMachineInspectorConnector,
    CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT,
    PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-plan-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_role(root: &PathBuf, capabilities: &[&str]) {
    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": "home-server",
        "capabilities": capabilities,
        "requirements": {
            "packages": [{ "id": "git", "state": "present" }],
            "configurations": [{ "id": "remote-access-policy", "state": "present" }],
            "services": [{ "id": "ssh", "running": true, "enabled": true }]
        }
    });
    fs::write(
        root.join("Control/machines/home-server.json"),
        serde_json::to_string_pretty(&declaration).unwrap(),
    ).unwrap();
}

fn observation(capabilities: &[&str], package: bool, configuration: bool, running: bool, enabled: bool) -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: capabilities.iter().map(|value| (*value).to_owned()).collect(),
        packages: vec![ObservedPackage { id: "git".to_owned(), present: package }],
        configurations: vec![ObservedConfiguration { id: "remote-access-policy".to_owned(), present: configuration }],
        services: vec![ObservedService {
            id: "ssh".to_owned(),
            present: true,
            running,
            enabled,
        }],
    }
}

struct PlanningConnector {
    manifest: ConnectorManifest,
}

impl PlanningConnector {
    fn new(ports: &[PortContract]) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "test.machine-reconciler".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Test machine plan reconciler".to_owned(),
                ports: ports.iter().map(|port| ConnectorPortDeclaration {
                    id: port.id.to_owned(),
                    version: port.version.to_owned(),
                }).collect(),
                platforms: vec!["test".to_owned()],
                entrypoint: "test:machine-plan".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "locally-mutating".to_owned(),
            },
        }
    }

    fn declares(&self, port: &PortContract) -> bool {
        self.manifest.ports.iter().any(|candidate| candidate.id == port.id)
    }
}

impl PackageManager for PlanningConnector {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError> {
        Ok(StateChangePreview { changed: true, summary: format!("package {} preview", input.id) })
    }

    fn apply(&self, input: &PackageStateRequest) -> Result<StateChangeResult, PortError> {
        Ok(StateChangeResult { changed: true, summary: format!("package {} applied", input.id) })
    }
}

impl ConfigurationManager for PlanningConnector {
    fn preview(&self, input: &ConfigurationStateRequest) -> Result<StateChangePreview, PortError> {
        Ok(StateChangePreview { changed: true, summary: format!("configuration {} preview", input.id) })
    }

    fn apply(&self, input: &ConfigurationStateRequest) -> Result<StateChangeResult, PortError> {
        Ok(StateChangeResult { changed: true, summary: format!("configuration {} applied", input.id) })
    }
}

impl ServiceManager for PlanningConnector {
    fn preview(&self, input: &ServiceStateRequest) -> Result<StateChangePreview, PortError> {
        Ok(StateChangePreview { changed: true, summary: format!("service {} preview", input.id) })
    }

    fn apply(&self, input: &ServiceStateRequest) -> Result<StateChangeResult, PortError> {
        Ok(StateChangeResult { changed: true, summary: format!("service {} applied", input.id) })
    }
}

impl Connector for PlanningConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn package_manager(&self) -> Option<&dyn PackageManager> {
        self.declares(&PACKAGE_MANAGER_PORT).then_some(self)
    }

    fn configuration_manager(&self) -> Option<&dyn ConfigurationManager> {
        self.declares(&CONFIGURATION_MANAGER_PORT).then_some(self)
    }

    fn service_manager(&self) -> Option<&dyn ServiceManager> {
        self.declares(&SERVICE_MANAGER_PORT).then_some(self)
    }
}

fn execute(root: &PathBuf, observation: MachineInspectionOutput, change_ports: &[PortContract]) -> central_ctrl::ActionResult {
    let mut connectors = ConnectorRegistry::default();
    connectors.register(StaticMachineInspectorConnector::new(observation)).unwrap();
    if !change_ports.is_empty() {
        connectors.register(PlanningConnector::new(change_ports)).unwrap();
    }
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    create_core_action_registry().execute("machine.plan", &json!({ "role": "home-server" }), &context)
}

#[test]
fn machine_inspect_returns_structured_observation_through_public_port_with_observed_provenance() {
    let root = temporary_directory("inspect").join("Central");
    let mut connectors = ConnectorRegistry::default();
    connectors.register(StaticMachineInspectorConnector::new(observation(&["remote-shell"], true, true, true, true))).unwrap();
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    let registry = create_core_action_registry();
    assert_eq!(registry.get("machine.inspect").unwrap().required_ports, vec![MACHINE_INSPECTOR_PORT.id]);
    let result = registry.execute("machine.inspect", &json!({}), &context);
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["observation"]["platform"], "test-os");
    assert_eq!(data["source"]["source_class"], "observed");
    assert_eq!(data["source"]["connector"]["id"], "reference.machine-static");
}

#[test]
fn satisfied_plan_keeps_authored_and_observed_state_in_separate_envelopes() {
    let root = temporary_directory("satisfied").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &["remote-shell"]);
    let result = execute(&root, observation(&["remote-shell"], true, true, true, true), &[]);
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["summary"]["satisfied"], 4);
    assert_eq!(data["summary"]["missing"], 0);
    assert_eq!(data["summary"]["changeable"], 0);
    assert_eq!(data["summary"]["unsupported"], 0);
    assert_eq!(data["authored"]["source"]["source_class"], "authored");
    assert_eq!(data["observed"]["source"]["source_class"], "observed");
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
}

#[test]
fn several_differences_are_changeable_only_when_their_abstract_ports_resolve() {
    let root = temporary_directory("changeable").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &["remote-shell"]);
    let result = execute(
        &root,
        observation(&["remote-shell"], false, false, false, false),
        &[PACKAGE_MANAGER_PORT, CONFIGURATION_MANAGER_PORT, SERVICE_MANAGER_PORT],
    );
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["summary"]["satisfied"], 1);
    assert_eq!(data["summary"]["changeable"], 3);
    let entries = data["entries"].as_array().unwrap();
    for (kind, port) in [
        ("package", PACKAGE_MANAGER_PORT.id),
        ("configuration", CONFIGURATION_MANAGER_PORT.id),
        ("service", SERVICE_MANAGER_PORT.id),
    ] {
        let entry = entries.iter().find(|entry| entry["kind"] == kind).unwrap();
        assert_eq!(entry["status"], "changeable");
        assert_eq!(entry["port"], port);
        assert_eq!(entry["connector"]["id"], "test.machine-reconciler");
        assert_eq!(entry["preview"]["changed"], true);
        assert!(!entry["preview"]["summary"].as_str().unwrap().is_empty());
    }
}

#[test]
fn known_difference_without_eligible_change_connector_is_missing_and_explained() {
    let root = temporary_directory("missing").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &["remote-shell"]);
    let result = execute(&root, observation(&["remote-shell"], false, true, true, true), &[]);
    let data = result.data.unwrap();
    assert_eq!(data["summary"]["missing"], 1);
    let entry = data["entries"].as_array().unwrap().iter().find(|entry| entry["kind"] == "package").unwrap();
    assert_eq!(entry["status"], "missing");
    assert_eq!(entry["port"], PACKAGE_MANAGER_PORT.id);
    assert!(entry["connector"].is_null());
    assert!(entry["reason"].as_str().unwrap().contains("no eligible PackageManager Connector"));
    assert!(entry.get("diagnostics").is_some());
}

#[test]
fn unobserved_capability_and_unreported_requirement_are_unsupported_with_actionable_reasons() {
    let root = temporary_directory("unsupported").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &["remote-shell", "native-automation"]);
    let mut observed = observation(&["remote-shell"], true, true, true, true);
    observed.configurations.clear();
    let result = execute(&root, observed, &[]);
    let data = result.data.unwrap();
    assert_eq!(data["summary"]["unsupported"], 2);
    let entries = data["entries"].as_array().unwrap();
    let capability = entries.iter().find(|entry| entry["id"] == "native-automation").unwrap();
    assert_eq!(capability["status"], "unsupported");
    assert!(capability["reason"].as_str().unwrap().contains("no general reconciliation Port"));
    let configuration = entries.iter().find(|entry| entry["kind"] == "configuration").unwrap();
    assert_eq!(configuration["status"], "unsupported");
    assert!(configuration["reason"].as_str().unwrap().contains("did not report configuration state"));
}

#[test]
fn default_cli_can_inspect_reference_host_without_a_personal_stack() {
    let environment = CliEnvironment { configured_root: None, home: Some(temporary_directory("cli-home")) };
    let structured = run_cli(&["--json".to_owned(), "machine.inspect".to_owned()], &environment);
    assert_eq!(structured.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&structured.output).unwrap();
    assert_eq!(value["action"], "machine.inspect");
    assert_eq!(value["data"]["source"]["source_class"], "observed");
    assert_eq!(value["data"]["source"]["connector"]["id"], "reference.machine-host");

    let human = run_cli(&["machine".to_owned(), "inspect".to_owned()], &environment);
    assert_eq!(human.exit_code, 0);
    assert!(human.output.contains("Observed host:"));
    assert!(human.output.contains("[observed]"));
}
