use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, CapabilityProbe,
    Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry,
    InMemoryMachineConnector, MachineInspectionInput, MachineInspectionOutput, MachineInspector,
    MutationClass, ObservedPackage, PackageManager, PackageStateRequest, PortContract, PortError,
    ResultStatus, RootOptions, StateChangePreview, StateChangeResult, CONNECTOR_API_VERSION,
    MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-reconcile-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_role(root: &PathBuf, capabilities: &[&str]) {
    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": "test-role",
        "capabilities": capabilities,
        "requirements": {
            "packages": [{ "id": "git", "state": "present" }],
            "configurations": [],
            "services": []
        }
    });
    fs::write(
        root.join("Control/machines/test-role.json"),
        serde_json::to_string_pretty(&declaration).unwrap(),
    ).unwrap();
}

fn observation(capabilities: &[&str], git_present: bool) -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: capabilities.iter().map(|value| (*value).to_owned()).collect(),
        packages: vec![ObservedPackage { id: "git".to_owned(), present: git_present }],
        configurations: Vec::new(),
        services: Vec::new(),
    }
}

fn execute(
    root: &PathBuf,
    connectors: &ConnectorRegistry,
    action: &str,
) -> central_ctrl::ActionResult {
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors,
        connector_context: &connector_context,
    };
    create_core_action_registry().execute(action, &json!({ "role": "test-role" }), &context)
}

#[test]
fn plan_preview_is_structured_and_non_mutating() {
    let root = temporary_directory("preview").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &[]);
    let connector = InMemoryMachineConnector::new(observation(&[], false));
    let state = connector.state();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();

    let result = execute(&root, &connectors, "machine.plan");
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    let package = data["entries"].as_array().unwrap().iter().find(|entry| entry["kind"] == "package").unwrap();
    assert_eq!(package["status"], "changeable");
    assert_eq!(package["port"], PACKAGE_MANAGER_PORT.id);
    assert_eq!(package["connector"]["id"], "reference.machine-reconciler");
    assert_eq!(package["preview"]["changed"], true);
    assert!(package["preview"]["summary"].as_str().unwrap().contains("git"));
    assert!(!state.snapshot().packages[0].present, "planning must not mutate observed state");
}

#[test]
fn complete_apply_delegates_through_planned_port_verifies_and_is_repeat_stable() {
    let root = temporary_directory("complete").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &[]);
    let connector = InMemoryMachineConnector::new(observation(&[], false));
    let state = connector.state();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();

    let first = execute(&root, &connectors, "machine.apply");
    assert_eq!(first.status, ResultStatus::Success);
    let data = first.data.unwrap();
    assert_eq!(data["outcome"], "complete");
    assert_eq!(data["operations"].as_array().unwrap().len(), 1);
    assert_eq!(data["operations"][0]["port"], PACKAGE_MANAGER_PORT.id);
    assert_eq!(data["operations"][0]["connector"]["id"], "reference.machine-reconciler");
    assert_eq!(data["verification"]["satisfied"], true);
    assert!(state.snapshot().packages[0].present);

    let repeated = execute(&root, &connectors, "machine.apply");
    assert_eq!(repeated.status, ResultStatus::Success);
    let data = repeated.data.unwrap();
    assert_eq!(data["outcome"], "complete");
    assert_eq!(data["operations"].as_array().unwrap().len(), 0);
    assert_eq!(data["verification"]["satisfied"], true);
}

#[test]
fn partial_result_applies_supported_change_but_preserves_unreconciled_capability_mismatch() {
    let root = temporary_directory("partial").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &["native-automation"]);
    let connector = InMemoryMachineConnector::new(observation(&[], false));
    let state = connector.state();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();

    let result = execute(&root, &connectors, "machine.apply");
    assert_eq!(result.status, ResultStatus::PartialCompletion);
    let details = result.error.unwrap().details.unwrap();
    assert_eq!(details["outcome"], "partial");
    assert_eq!(details["operations"].as_array().unwrap().len(), 1);
    assert_eq!(details["verification"]["satisfied"], false);
    assert_eq!(details["verification"]["plan"]["summary"]["unsupported"], 1);
    assert!(state.snapshot().packages[0].present);
}

#[test]
fn unavailable_result_does_not_mutate_when_no_reconciliation_connector_exists() {
    let root = temporary_directory("unavailable").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &[]);
    let mut connectors = ConnectorRegistry::default();
    connectors.register(central_ctrl::StaticMachineInspectorConnector::new(observation(&[], false))).unwrap();

    let result = execute(&root, &connectors, "machine.apply");
    assert_eq!(result.status, ResultStatus::UnavailableCapability);
    let details = result.error.unwrap().details.unwrap();
    assert_eq!(details["outcome"], "unavailable");
    assert_eq!(details["operations"].as_array().unwrap().len(), 0);
    assert_eq!(details["initial_plan"]["summary"]["missing"], 1);
}

struct FaultyPackageConnector {
    manifest: ConnectorManifest,
    fail_apply: bool,
}

impl FaultyPackageConnector {
    fn new(fail_apply: bool) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: if fail_apply { "test.package-failure" } else { "test.package-liar" }.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Faulty package test Connector".to_owned(),
                ports: [MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT].iter().map(|port| ConnectorPortDeclaration {
                    id: port.id.to_owned(),
                    version: port.version.to_owned(),
                }).collect(),
                platforms: vec!["test".to_owned()],
                entrypoint: "test:faulty-package".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "locally-mutating".to_owned(),
            },
            fail_apply,
        }
    }
}

impl MachineInspector for FaultyPackageConnector {
    fn inspect(&self, _input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        Ok(observation(&[], false))
    }
}

impl PackageManager for FaultyPackageConnector {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError> {
        Ok(StateChangePreview { changed: true, summary: format!("would install {}", input.id) })
    }

    fn apply(&self, input: &PackageStateRequest) -> Result<StateChangeResult, PortError> {
        if self.fail_apply {
            Err(PortError::provider(format!("provider refused {}", input.id)))
        } else {
            Ok(StateChangeResult { changed: true, summary: format!("claimed install {}", input.id) })
        }
    }
}

impl Connector for FaultyPackageConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }

    fn package_manager(&self) -> Option<&dyn PackageManager> {
        Some(self)
    }
}

#[test]
fn provider_failure_is_distinct_from_unavailable_and_partial() {
    let root = temporary_directory("failed").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &[]);
    let mut connectors = ConnectorRegistry::default();
    connectors.register(FaultyPackageConnector::new(true)).unwrap();

    let result = execute(&root, &connectors, "machine.apply");
    assert_eq!(result.status, ResultStatus::ConnectorFailure);
    assert!(result.error.unwrap().message.contains("applying planned package"));
}

#[test]
fn verification_mismatch_is_a_first_class_post_mutation_failure() {
    let root = temporary_directory("verify-mismatch").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &[]);
    let mut connectors = ConnectorRegistry::default();
    connectors.register(FaultyPackageConnector::new(false)).unwrap();

    let result = execute(&root, &connectors, "machine.apply");
    assert_eq!(result.status, ResultStatus::VerificationFailure);
    let details = result.error.unwrap().details.unwrap();
    assert_eq!(details["outcome"], "partial");
    assert_eq!(details["operations"].as_array().unwrap().len(), 1);
    assert_eq!(details["verification"]["satisfied"], false);
}

#[test]
fn machine_verify_reports_mismatch_without_mutation_and_actions_advertise_the_right_surface_contract() {
    let root = temporary_directory("verify").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, &[]);
    let connector = InMemoryMachineConnector::new(observation(&[], false));
    let state = connector.state();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();

    let result = execute(&root, &connectors, "machine.verify");
    assert_eq!(result.status, ResultStatus::VerificationFailure);
    assert!(!state.snapshot().packages[0].present);

    let registry = create_core_action_registry();
    let apply = registry.get("machine.apply").unwrap();
    assert_eq!(apply.mutation_class, MutationClass::LocallyMutating);
    assert!(apply.preview_supported);
    let verify = registry.get("machine.verify").unwrap();
    assert_eq!(verify.mutation_class, MutationClass::ReadOnly);
    assert_eq!(verify.required_ports, vec![MACHINE_INSPECTOR_PORT.id]);
}
