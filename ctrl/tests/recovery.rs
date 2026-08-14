use central_ctrl::{
    create_core_action_registry, initialize_central, run_synchronizer_conformance,
    ActionExecutionContext, CapabilityProbe, Connector, ConnectorContext, ConnectorManifest,
    ConnectorPortDeclaration, ConnectorRegistry, InMemoryMachineConnector, MachineInspectionInput,
    MachineInspectionOutput, MachineInspector, ObservedPackage, PackageManager, PackageStateRequest,
    PortContract, PortError, ResultStatus, RootOptions, SharedMachineState, StateChangePreview,
    StateChangeResult, SynchronizationRequest, Synchronizer, SynchronizerConformanceFixture,
    CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT, SYNCHRONIZER_PORT,
};
use serde_json::{json, Value};
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
        "central-recovery-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_machine(root: &PathBuf, role: &str, capabilities: &[&str], package_present: bool) {
    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": role,
        "capabilities": capabilities,
        "requirements": {
            "packages": [{ "id": "git", "state": "present" }],
            "configurations": [],
            "services": []
        }
    });
    fs::write(
        root.join("Control/machines").join(format!("{role}.json")),
        serde_json::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
    assert!(!package_present || declaration["requirements"]["packages"][0]["state"] == "present");
}

fn write_recovery(root: &PathBuf, role: &str) {
    let declaration = json!({
        "schema": "central.recovery",
        "version": 1,
        "role": role,
        "synchronization": {
            "id": "central-authored-source",
            "source": {
                "kind": "fixture",
                "reference": "fixture://central-source"
            }
        }
    });
    fs::write(
        root.join("Control/machines")
            .join(format!("{role}.recovery.json")),
        serde_json::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
}

fn observation(capabilities: &[&str], package_present: bool) -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "fixture-os".to_owned(),
        architecture: "fixture-arch".to_owned(),
        capabilities: capabilities.iter().map(|value| (*value).to_owned()).collect(),
        packages: vec![ObservedPackage {
            id: "git".to_owned(),
            present: package_present,
        }],
        configurations: Vec::new(),
        services: Vec::new(),
    }
}

#[derive(Clone)]
struct SyncState {
    changed: Arc<Mutex<bool>>,
    applies: Arc<Mutex<usize>>,
}

impl SyncState {
    fn new(changed: bool) -> Self {
        Self {
            changed: Arc::new(Mutex::new(changed)),
            applies: Arc::new(Mutex::new(0)),
        }
    }

    fn changed(&self) -> bool {
        *self.changed.lock().unwrap()
    }

    fn applies(&self) -> usize {
        *self.applies.lock().unwrap()
    }
}

struct FixtureSynchronizer {
    manifest: ConnectorManifest,
    state: SyncState,
}

impl FixtureSynchronizer {
    fn new(state: SyncState) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "fixture.synchronizer".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Recovery synchronization fixture".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: SYNCHRONIZER_PORT.id.to_owned(),
                    version: SYNCHRONIZER_PORT.version.to_owned(),
                }],
                platforms: vec!["*".to_owned()],
                entrypoint: "test:recovery::FixtureSynchronizer".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "externally-mutating".to_owned(),
            },
            state,
        }
    }
}

impl Synchronizer for FixtureSynchronizer {
    fn preview(&self, input: &SynchronizationRequest) -> Result<StateChangePreview, PortError> {
        assert_eq!(input.id, "central-authored-source");
        let source = input.source.as_ref().expect("fixture recovery keeps authored source");
        assert_eq!(source.kind, "fixture");
        assert_eq!(source.reference, "fixture://central-source");
        let changed = self.state.changed();
        Ok(StateChangePreview {
            changed,
            summary: if changed {
                "Central authored source would synchronize.".to_owned()
            } else {
                "Central authored source is synchronized.".to_owned()
            },
        })
    }

    fn apply(&self, input: &SynchronizationRequest) -> Result<StateChangeResult, PortError> {
        let changed = self.preview(input)?.changed;
        if changed {
            *self.state.changed.lock().unwrap() = false;
            *self.state.applies.lock().unwrap() += 1;
        }
        Ok(StateChangeResult {
            changed,
            summary: if changed {
                "Central authored source synchronized.".to_owned()
            } else {
                "Central authored source already synchronized.".to_owned()
            },
        })
    }
}

impl Connector for FixtureSynchronizer {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn synchronizer(&self) -> Option<&dyn Synchronizer> {
        Some(self)
    }
}

struct LyingPackageConnector {
    manifest: ConnectorManifest,
}

impl LyingPackageConnector {
    fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "fixture.lying-package".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Verification failure fixture".to_owned(),
                ports: [MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT]
                    .iter()
                    .map(|port| ConnectorPortDeclaration {
                        id: port.id.to_owned(),
                        version: port.version.to_owned(),
                    })
                    .collect(),
                platforms: vec!["*".to_owned()],
                entrypoint: "test:recovery::LyingPackageConnector".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "locally-mutating".to_owned(),
            },
        }
    }
}

impl MachineInspector for LyingPackageConnector {
    fn inspect(&self, _input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        Ok(observation(&[], false))
    }
}

impl PackageManager for LyingPackageConnector {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError> {
        Ok(StateChangePreview {
            changed: input.present,
            summary: "fixture reports package would change".to_owned(),
        })
    }

    fn apply(&self, _input: &PackageStateRequest) -> Result<StateChangeResult, PortError> {
        Ok(StateChangeResult {
            changed: true,
            summary: "fixture claims apply succeeded but observation remains drifted".to_owned(),
        })
    }
}

impl Connector for LyingPackageConnector {
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

fn execute(
    root: &PathBuf,
    connectors: &ConnectorRegistry,
    platform: &str,
    action: &str,
    role: &str,
) -> central_ctrl::ActionResult {
    let connector_context = ConnectorContext {
        platform: platform.to_owned(),
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
    create_core_action_registry().execute(action, &json!({ "role": role }), &context)
}

#[test]
fn synchronizer_passes_shared_preview_apply_verify_and_idempotence_conformance() {
    let state = SyncState::new(true);
    let connector = FixtureSynchronizer::new(state.clone());
    let report = run_synchronizer_conformance(
        &connector,
        &SynchronizerConformanceFixture {
            platform: "linux".to_owned(),
            request: SynchronizationRequest {
                id: "central-authored-source".to_owned(),
                source: Some(central_ctrl::ReconciliationSourceReference {
                    kind: "fixture".to_owned(),
                    reference: "fixture://central-source".to_owned(),
                }),
            },
        },
    )
    .unwrap();
    assert_eq!(report.port_id, SYNCHRONIZER_PORT.id);
    assert!(report.checks.iter().any(|check| check == "post-apply-preview"));
    assert!(report.checks.iter().any(|check| check == "idempotent-apply"));
    assert!(!state.changed());
    assert_eq!(state.applies(), 1);
}

#[test]
fn recovery_actions_are_canonical_and_configured_sync_is_authored_control() {
    let fixture = temporary_directory("descriptor");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, "primary-workstation", &[], true);
    write_recovery(&root, "primary-workstation");

    let state = SyncState::new(false);
    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(FixtureSynchronizer::new(state))
        .unwrap();
    connectors
        .register(InMemoryMachineConnector::new(observation(&[], true)))
        .unwrap();

    let registry = create_core_action_registry();
    let plan_descriptor = registry.get("central.recovery.plan").unwrap();
    assert_eq!(plan_descriptor.mutation_class.as_str(), "read-only");
    assert!(plan_descriptor.preview_supported);
    let recover_descriptor = registry.get("central.recover").unwrap();
    assert_eq!(recover_descriptor.mutation_class.as_str(), "externally-mutating");
    assert!(recover_descriptor.description.contains("machine.apply"));
    assert!(recover_descriptor.description.contains("machine.verify"));

    let result = execute(
        &root,
        &connectors,
        "macos",
        "central.recovery.plan",
        "primary-workstation",
    );
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["synchronization"]["status"], "satisfied");
    assert_eq!(
        data["synchronization"]["authored"]["source"]["source_class"],
        "authored"
    );
    assert_eq!(
        data["synchronization"]["authored"]["source"]["path"],
        "Control/machines/primary-workstation.recovery.json"
    );
}

#[test]
fn unavailable_configured_synchronization_fails_before_machine_mutation() {
    let fixture = temporary_directory("unavailable-sync");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, "home-server", &[], false);
    write_recovery(&root, "home-server");

    let machine = InMemoryMachineConnector::new(observation(&[], false));
    let state = machine.state();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(machine).unwrap();

    let plan = execute(
        &root,
        &connectors,
        "linux",
        "central.recovery.plan",
        "home-server",
    );
    assert_eq!(plan.status, ResultStatus::Success);
    assert_eq!(plan.data.unwrap()["synchronization"]["status"], "unavailable");

    let recover = execute(
        &root,
        &connectors,
        "linux",
        "central.recover",
        "home-server",
    );
    assert_eq!(recover.status, ResultStatus::UnavailableCapability);
    assert!(!state.snapshot().packages[0].present);
}

#[test]
fn recovery_can_complete_partially_without_hiding_unsupported_authored_intent() {
    let fixture = temporary_directory("partial");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, "home-server", &["remote-shell"], false);

    let machine = InMemoryMachineConnector::new(observation(&[], false));
    let state = machine.state();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(machine).unwrap();

    let recover = execute(
        &root,
        &connectors,
        "linux",
        "central.recover",
        "home-server",
    );
    assert_eq!(recover.status, ResultStatus::PartialCompletion);
    assert!(state.snapshot().packages[0].present);
    let details = recover.error.unwrap().details.unwrap();
    assert_eq!(details["machine_apply"]["status"], "partial_completion");
}

#[test]
fn recovery_reports_verification_failure_when_provider_claims_apply_but_state_does_not_change() {
    let fixture = temporary_directory("verification-failure");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, "home-server", &[], false);

    let mut connectors = ConnectorRegistry::default();
    connectors.register(LyingPackageConnector::new()).unwrap();

    let recover = execute(
        &root,
        &connectors,
        "linux",
        "central.recover",
        "home-server",
    );
    assert_eq!(recover.status, ResultStatus::VerificationFailure);
    let details = recover.error.unwrap().details.unwrap();
    assert_eq!(details["machine_apply"]["status"], "verification_failure");
}

fn repeated_recovery_is_stable_on(platform: &str, role: &str) {
    let fixture = temporary_directory(&format!("repeat-{platform}"));
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, role, &[], false);
    write_recovery(&root, role);

    let sync_state = SyncState::new(true);
    let machine = InMemoryMachineConnector::new(observation(&[], false));
    let machine_state: SharedMachineState = machine.state();
    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(FixtureSynchronizer::new(sync_state.clone()))
        .unwrap();
    connectors.register(machine).unwrap();

    let first = execute(&root, &connectors, platform, "central.recover", role);
    assert_eq!(first.status, ResultStatus::Success);
    let first_data = first.data.unwrap();
    assert_eq!(first_data["outcome"], "complete");
    assert_eq!(first_data["synchronization"]["changed"], true);
    assert_eq!(
        first_data["machine_apply"]["operations"].as_array().unwrap().len(),
        1
    );
    assert_eq!(first_data["verification"]["satisfied"], true);
    assert!(machine_state.snapshot().packages[0].present);
    assert_eq!(sync_state.applies(), 1);

    let second = execute(&root, &connectors, platform, "central.recover", role);
    assert_eq!(second.status, ResultStatus::Success);
    let second_data = second.data.unwrap();
    assert_eq!(second_data["outcome"], "complete");
    assert!(second_data["synchronization"].is_null());
    assert_eq!(
        second_data["machine_apply"]["operations"].as_array().unwrap().len(),
        0
    );
    assert_eq!(second_data["verification"]["satisfied"], true);
    assert_eq!(sync_state.applies(), 1);
}

#[test]
fn repeated_recovery_is_stable_in_macos_and_ubuntu_shaped_contexts() {
    repeated_recovery_is_stable_on("macos", "primary-workstation");
    repeated_recovery_is_stable_on("linux", "home-server");
}

#[test]
fn malformed_recovery_declaration_is_an_explicit_failure_not_silent_fallback() {
    let fixture = temporary_directory("invalid-declaration");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, "home-server", &[], true);
    fs::write(
        root.join("Control/machines/home-server.recovery.json"),
        "{ not valid json",
    )
    .unwrap();

    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(InMemoryMachineConnector::new(observation(&[], true)))
        .unwrap();
    let plan = execute(
        &root,
        &connectors,
        "linux",
        "central.recovery.plan",
        "home-server",
    );
    assert_eq!(plan.status, ResultStatus::InvalidInput);
    assert!(plan.error.unwrap().message.contains("Recovery declaration"));
}

#[test]
fn recovery_without_configured_sync_reuses_machine_apply_directly() {
    let fixture = temporary_directory("no-sync");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    write_machine(&root, "home-server", &[], false);

    let machine = InMemoryMachineConnector::new(observation(&[], false));
    let mut connectors = ConnectorRegistry::default();
    connectors.register(machine).unwrap();

    let plan = execute(
        &root,
        &connectors,
        "linux",
        "central.recovery.plan",
        "home-server",
    );
    assert_eq!(plan.status, ResultStatus::Success);
    assert_eq!(plan.data.unwrap()["synchronization"]["status"], "not_configured");

    let recover = execute(
        &root,
        &connectors,
        "linux",
        "central.recover",
        "home-server",
    );
    assert_eq!(recover.status, ResultStatus::Success);
    let data = recover.data.unwrap();
    assert_eq!(data["machine_apply"]["operations"].as_array().unwrap().len(), 1);
    assert_eq!(data["verification"]["satisfied"], true);
}
