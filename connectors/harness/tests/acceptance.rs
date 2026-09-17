use central_connector_sdk::{
    run_machine_inspector_conformance, Connector, ConnectorContext, ConnectorRegistry,
    MachineInspector, MachineInspectorConformanceFixture, MachineInspectionInput,
    MACHINE_INSPECTOR_PORT, TAG_STORE_PORT,
};
use central_harness_connector::{HarnessCapabilityConnector, CONNECTOR_ID};
use central_reference_connectors::StaticMachineInspectorConnector;
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-harness-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn make_executable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// What the fixture Actuation CLI does when invoked.
enum StubScript {
    /// Serve the canned detection and capability documents.
    Serve,
    /// Refuse every subcommand with a nonzero exit.
    Refuse,
    /// Serve detection but refuse the capability catalog.
    ServeDetectionRefuseCapability,
    /// Hang, exercising the command timeout.
    Wedged,
}

/// A fixture Actuation CLI: a shell stub that serves canned read-model
/// documents. `$2` is the subcommand because the connector invokes
/// `<stub> harness detect --json` / `<stub> harness capability --json`.
fn stub_actuation(dir: &Path, detect_document: &str, capability_document: &str, script: StubScript) -> PathBuf {
    let detect = dir.join("detect.json");
    fs::write(&detect, detect_document).unwrap();
    let capability = dir.join("capability.json");
    fs::write(&capability, capability_document).unwrap();
    let stub = dir.join("actuation");
    let body = match script {
        StubScript::Serve => format!(
            "#!/bin/sh\nif [ \"$2\" = detect ]; then\n  cat {}\nelif [ \"$2\" = capability ]; then\n  cat {}\nelse\n  exit 64\nfi\n",
            detect.display(),
            capability.display()
        ),
        StubScript::Refuse => "#!/bin/sh\nexit 1\n".to_owned(),
        StubScript::ServeDetectionRefuseCapability => format!(
            "#!/bin/sh\nif [ \"$2\" = detect ]; then\n  cat {}\nelse\n  exit 1\nfi\n",
            detect.display()
        ),
        StubScript::Wedged => "#!/bin/sh\nsleep 30\n".to_owned(),
    };
    fs::write(&stub, body).unwrap();
    make_executable(&stub);
    stub
}

const DETECT_TWO_HARNESSES: &str = r#"{
  "schema": "actuation.harness-detection/v1",
  "document": "detection",
  "catalog_revision": 6,
  "harnesses": [
    { "slug": "claude-code", "harness_ref": "harness/claude-code", "native_kind": "harness", "state": "detected" },
    { "slug": "gemini", "harness_ref": "harness/gemini", "native_kind": "harness", "state": "not-installed" },
    { "slug": "pi", "harness_ref": "harness/pi", "native_kind": "harness", "state": "detected" }
  ],
  "absent": ["gemini"],
  "availability": "complete"
}"#;

const CAPABILITY_CLAUDE_CODE_ONLY: &str = r#"{
  "schema": "actuation.harness-capability/v1",
  "document": "capability-catalog",
  "catalog_revision": 6,
  "capabilities": [
    { "schema": "actuation.harness-capability/v1", "document": "capability", "harness_slug": "claude-code", "summary": "hooks" }
  ]
}"#;

fn empty_registry() -> String {
    json!({
        "schema": "workcell.registry/v1",
        "workcell_ref": "workcell:local",
        "instances": {}
    })
    .to_string()
}

fn registry_with_instances() -> String {
    json!({
        "schema": "workcell.registry/v1",
        "workcell_ref": "workcell:local",
        "instances": {
            "instance:hermes:abc": {
                "schema": "workcell.harness-instance/v1",
                "harness_ref": "harness/hermes",
                "liveness": "live",
                "workcell_ref": "workcell:local"
            },
            "instance:pi:def": {
                "schema": "workcell.harness-instance/v1",
                "harness_ref": "harness/pi",
                "liveness": "stale",
                "workcell_ref": "workcell:local"
            }
        }
    })
    .to_string()
}

fn write_registry(dir: &Path, document: &str) -> PathBuf {
    let instances = dir.join("instances");
    fs::create_dir_all(&instances).unwrap();
    let registry = instances.join("registry.json");
    fs::write(&registry, document).unwrap();
    registry
}

#[test]
fn manifest_declares_read_only_machine_inspector_port() {
    let root = temporary_directory("manifest");
    let connector = HarnessCapabilityConnector::with_sources(None, root.join("absent.json"));
    let manifest = connector.manifest();
    assert_eq!(manifest.id, CONNECTOR_ID);
    assert_eq!(manifest.id, "read-models.harness-capability");
    assert_eq!(manifest.mutation_scope, "read-only");
    assert_eq!(manifest.ports.len(), 1);
    assert_eq!(manifest.ports[0].id, "MachineInspector");
}

#[test]
fn probe_is_eligible_when_a_source_exists_and_unavailable_when_neither_does() {
    let root = temporary_directory("probe");
    let registry = write_registry(&root, &empty_registry());
    let context = ConnectorContext {
        platform: std::env::consts::OS.to_owned(),
    };
    let eligible = HarnessCapabilityConnector::with_sources(None, registry);
    let probe = eligible.probe(&MACHINE_INSPECTOR_PORT, &context);
    assert!(probe.available);

    let actuation_only = HarnessCapabilityConnector::with_sources(
        Some(root.join("stub-actuation")),
        root.join("missing-registry.json"),
    );
    assert!(actuation_only
        .probe(&MACHINE_INSPECTOR_PORT, &context)
        .available);

    let absent =
        HarnessCapabilityConnector::with_sources(None, root.join("missing-registry.json"));
    let probe = absent.probe(&MACHINE_INSPECTOR_PORT, &context);
    assert!(!probe.available);
    let reason = probe.reason.unwrap();
    assert!(reason.contains("No harness capability source is reachable"));
    assert!(reason.contains("actuation"));
    assert!(reason.contains("registry"));

    let wrong_port = absent.probe(&TAG_STORE_PORT, &context);
    assert!(!wrong_port.available);
}

#[test]
fn inspect_reports_detected_harness_capabilities_with_sources() {
    let root = temporary_directory("sources");
    let actuation = stub_actuation(
        &root,
        DETECT_TWO_HARNESSES,
        CAPABILITY_CLAUDE_CODE_ONLY,
        StubScript::Serve,
    );
    let registry = write_registry(&root, &registry_with_instances());
    let connector = HarnessCapabilityConnector::with_sources(Some(actuation), registry);
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert_eq!(observation.platform, std::env::consts::OS);
    assert_eq!(observation.architecture, std::env::consts::ARCH);
    assert!(observation.packages.is_empty());
    assert!(observation.configurations.is_empty());
    assert!(observation.services.is_empty());
    assert_eq!(
        observation.capabilities,
        vec![
            "MachineInspector".to_owned(),
            "claude-code@source:actuation-harness-capability".to_owned(),
            "hermes@source:workcell-harness-instance".to_owned(),
            // Detected by Actuation but without a declared capability
            // descriptor; the stale registry record is not a capability.
            "pi@source:actuation-harness-detection".to_owned(),
        ]
    );
}

#[test]
fn absent_sources_are_disclosed_and_inspection_stays_clean() {
    let root = temporary_directory("absent");
    let connector =
        HarnessCapabilityConnector::with_sources(None, root.join("missing/registry.json"));
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert_eq!(
        observation.capabilities,
        vec![
            "MachineInspector".to_owned(),
            "actuation-harness-detect@source:absent".to_owned(),
            "workcell-harness-instances@source:absent".to_owned(),
        ]
    );
}

#[test]
fn refused_actuation_is_a_valid_state_disclosed_as_unavailable() {
    let root = temporary_directory("refused");
    let actuation = stub_actuation(&root, "", "", StubScript::Refuse);
    let connector =
        HarnessCapabilityConnector::with_sources(Some(actuation), root.join("missing.json"));
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert_eq!(
        observation.capabilities,
        vec![
            "MachineInspector".to_owned(),
            "actuation-harness-detect@source:unavailable".to_owned(),
            "workcell-harness-instances@source:absent".to_owned(),
        ]
    );
}

#[test]
fn wedged_actuation_is_bounded_by_the_command_timeout() {
    let root = temporary_directory("wedged");
    let actuation = stub_actuation(&root, "", "", StubScript::Wedged);
    let connector = HarnessCapabilityConnector::with_sources(
        Some(actuation),
        root.join("missing.json"),
    )
    .with_command_timeout(Duration::from_millis(300));
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert!(observation
        .capabilities
        .contains(&"actuation-harness-detect@source:unavailable".to_owned()));
}

#[test]
fn malformed_detection_output_is_disclosed_as_unavailable() {
    let root = temporary_directory("malformed");
    let actuation = stub_actuation(
        &root,
        "not json",
        CAPABILITY_CLAUDE_CODE_ONLY,
        StubScript::Serve,
    );
    let connector =
        HarnessCapabilityConnector::with_sources(Some(actuation), root.join("missing.json"));
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert!(observation
        .capabilities
        .contains(&"actuation-harness-detect@source:unavailable".to_owned()));
}

#[test]
fn detection_without_capability_catalog_still_discloses_detected_harnesses() {
    let root = temporary_directory("detection-only");
    let actuation = stub_actuation(
        &root,
        DETECT_TWO_HARNESSES,
        "",
        StubScript::ServeDetectionRefuseCapability,
    );
    let connector =
        HarnessCapabilityConnector::with_sources(Some(actuation), root.join("missing.json"));
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert!(observation
        .capabilities
        .contains(&"claude-code@source:actuation-harness-detection".to_owned()));
    assert!(observation
        .capabilities
        .contains(&"pi@source:actuation-harness-detection".to_owned()));
    assert!(!observation
        .capabilities
        .iter()
        .any(|value| value.contains("actuation-harness-capability")));
}

#[test]
fn registry_with_unexpected_schema_is_disclosed_as_unavailable() {
    let root = temporary_directory("schema");
    let registry = write_registry(&root, r#"{ "schema": "something.else/v9" }"#);
    let connector = HarnessCapabilityConnector::with_sources(None, registry);
    let observation = connector.inspect(&MachineInspectionInput::default()).unwrap();
    assert!(observation
        .capabilities
        .contains(&"workcell-harness-instances@source:unavailable".to_owned()));
}

#[test]
fn passes_public_machine_inspector_conformance_with_a_stub_source() {
    let root = temporary_directory("conformance");
    let actuation = stub_actuation(
        &root,
        DETECT_TWO_HARNESSES,
        CAPABILITY_CLAUDE_CODE_ONLY,
        StubScript::Serve,
    );
    let registry = write_registry(&root, &registry_with_instances());
    let connector = HarnessCapabilityConnector::with_sources(Some(actuation), registry);
    let report = run_machine_inspector_conformance(
        &connector,
        &MachineInspectorConformanceFixture {
            platform: std::env::consts::OS.to_owned(),
            expected: None,
        },
    )
    .unwrap();
    assert_eq!(report.port_id, "MachineInspector");
    assert_eq!(report.connector.id, CONNECTOR_ID);
}

#[test]
fn registry_selection_prefers_the_harness_inspector_when_eligible_and_falls_back_when_not() {
    let root = temporary_directory("selection");
    let context = ConnectorContext {
        platform: std::env::consts::OS.to_owned(),
    };

    // A reachable source makes the harness connector the selected inspector
    // even with the reference inspector registered (id ordering:
    // read-models.* < reference.*).
    let actuation = stub_actuation(
        &root,
        DETECT_TWO_HARNESSES,
        CAPABILITY_CLAUDE_CODE_ONLY,
        StubScript::Serve,
    );
    let mut eligible = ConnectorRegistry::default();
    eligible
        .register(StaticMachineInspectorConnector::current_host())
        .unwrap();
    eligible
        .register(HarnessCapabilityConnector::with_sources(
            Some(actuation),
            root.join("missing.json"),
        ))
        .unwrap();
    let resolution = eligible.resolve(&MACHINE_INSPECTOR_PORT, &context);
    assert_eq!(
        resolution.connector.unwrap().manifest().id,
        "read-models.harness-capability"
    );

    // Without any reachable source the probe reports unavailable and the
    // reference inspector stays selected, exactly as before this connector
    // existed.
    let mut fallback = ConnectorRegistry::default();
    fallback
        .register(StaticMachineInspectorConnector::current_host())
        .unwrap();
    fallback
        .register(HarnessCapabilityConnector::with_sources(
            None,
            root.join("missing.json"),
        ))
        .unwrap();
    let resolution = fallback.resolve(&MACHINE_INSPECTOR_PORT, &context);
    assert_eq!(
        resolution.connector.unwrap().manifest().id,
        "reference.machine-host"
    );
    assert!(resolution.diagnostics.ineligible.iter().any(|ineligible| {
        ineligible.connector.id == "read-models.harness-capability"
    }));
}
