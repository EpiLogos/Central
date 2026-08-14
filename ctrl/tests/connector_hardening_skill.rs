use central_connector_sdk::{
    run_work_discovery_conformance, CapabilityProbe, Connector, ConnectorContext,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, PortContract, PortError,
    WorkDiscovery, WorkDiscoveryConformanceFixture, WorkDiscoveryInput, WorkDiscoveryOutput,
    WorkItem, CONNECTOR_API_VERSION, WORK_DISCOVERY_PORT,
};
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ResultStatus,
    RootOptions,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SKILL: &str = include_str!("../../skills/connector-hardening/SKILL.md");
const FIXTURES: &str = include_str!("../../skills/connector-hardening/fixtures/failure-cases.json");

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-connector-hardening-skill-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn work_manifest() -> ConnectorManifest {
    ConnectorManifest {
        api_version: CONNECTOR_API_VERSION.to_owned(),
        id: "fixture.work-broken".to_owned(),
        version: "0.1.0".to_owned(),
        display_name: "Controlled WorkDiscovery fixture".to_owned(),
        ports: vec![ConnectorPortDeclaration {
            id: WORK_DISCOVERY_PORT.id.to_owned(),
            version: WORK_DISCOVERY_PORT.version.to_owned(),
        }],
        platforms: vec!["*".to_owned()],
        entrypoint: "test:connector_hardening_skill".to_owned(),
        runtime_requirements: Vec::new(),
        dependency_probes: Vec::new(),
        configuration_requirements: Vec::new(),
        mutation_scope: "read-only".to_owned(),
    }
}

struct BrokenWorkConnector {
    manifest: ConnectorManifest,
}

impl BrokenWorkConnector {
    fn new() -> Self {
        Self {
            manifest: work_manifest(),
        }
    }
}

impl WorkDiscovery for BrokenWorkConnector {
    fn list(&self, _input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError> {
        Err(PortError::provider("controlled WorkDiscovery provider failure"))
    }
}

impl Connector for BrokenWorkConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn work_discovery(&self) -> Option<&dyn WorkDiscovery> {
        Some(self)
    }
}

struct HardenedWorkConnector {
    manifest: ConnectorManifest,
}

impl HardenedWorkConnector {
    fn new() -> Self {
        Self {
            manifest: work_manifest(),
        }
    }
}

impl WorkDiscovery for HardenedWorkConnector {
    fn list(&self, input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError> {
        let mut items = fs::read_dir(&input.work_root)
            .map_err(|error| PortError::provider(error.to_string()))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir() {
                    return None;
                }
                Some(WorkItem {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path(),
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(WorkDiscoveryOutput { items })
    }
}

impl Connector for HardenedWorkConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn work_discovery(&self) -> Option<&dyn WorkDiscovery> {
        Some(self)
    }
}

fn action_context<'a>(
    root_options: &'a RootOptions,
    connectors: &'a ConnectorRegistry,
    connector_context: &'a ConnectorContext,
) -> ActionExecutionContext<'a> {
    ActionExecutionContext {
        root_options,
        connectors,
        connector_context,
    }
}

#[test]
fn skill_requires_reproduction_classification_owned_regression_conformance_and_leakage_check() {
    for required in [
        "reproducible failure or concrete friction report",
        "Classify the owner before proposing a change",
        "core Action",
        "Port contract",
        "SDK support",
        "Connector implementation",
        "Surface implementation",
        "target limitation",
        "local configuration",
        "Put the regression at the owning layer",
        "Run the relevant conformance suite after the fix",
        "Invoke the canonical Action when applicable",
        "no provider/tool/platform condition was added to a provider-neutral core Action",
        "public contract/SDK",
        "personal-stack exception in core",
        "Controlled reference proof",
    ] {
        assert!(SKILL.contains(required), "Connector-hardening Skill is missing: {required}");
    }
}

#[test]
fn controlled_failure_fixture_classifies_the_problem_before_selecting_the_fix() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    assert_eq!(fixtures["version"], 1);
    let case = &fixtures["cases"][0];
    assert_eq!(case["failure"]["action"], "work.list");
    assert_eq!(case["failure"]["port"], "WorkDiscovery");
    assert_eq!(case["classification"]["owner"], "Connector implementation");
    assert_eq!(case["regression"]["owner"], "Connector implementation");
    assert_eq!(case["regression"]["shared_conformance"], "run_work_discovery_conformance");
    assert_eq!(case["fix"]["layer"], "Connector implementation");
    assert_eq!(case["core_exception_allowed"], false);
}

#[test]
fn controlled_connector_failure_is_rejected_by_public_conformance_and_surfaces_through_the_canonical_action() {
    let fixture = temporary_directory("failure");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work/project-alpha")).unwrap();

    let conformance = run_work_discovery_conformance(
        &BrokenWorkConnector::new(),
        &WorkDiscoveryConformanceFixture {
            work_root: root.join("Work"),
            platform: "fixture-os".to_owned(),
            expected_names: Some(vec!["project-alpha".to_owned()]),
        },
    )
    .expect_err("broken Connector must fail public conformance");
    assert_eq!(conformance.check, "typed-operation");
    assert!(conformance.message.contains("controlled WorkDiscovery provider failure"));

    let mut connectors = ConnectorRegistry::default();
    connectors.register(BrokenWorkConnector::new()).unwrap();
    let connector_context = ConnectorContext {
        platform: "fixture-os".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let context = action_context(&root_options, &connectors, &connector_context);
    let result = create_core_action_registry().execute("work.list", &json!({}), &context);

    assert_eq!(result.status, ResultStatus::ConnectorFailure);
    let error = result.error.unwrap();
    let details = error.details.unwrap();
    assert_eq!(details["port"], "WorkDiscovery");
    assert_eq!(details["connector"], "fixture.work-broken");
    assert_eq!(details["provider_error"]["code"], "provider_operation_failed");
    assert!(details["provider_error"]["message"]
        .as_str()
        .unwrap()
        .contains("controlled WorkDiscovery provider failure"));

    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn connector_local_fix_passes_the_same_public_conformance_and_unchanged_canonical_action() {
    let fixture = temporary_directory("fixed");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work/project-alpha")).unwrap();

    let connector = HardenedWorkConnector::new();
    let report = run_work_discovery_conformance(
        &connector,
        &WorkDiscoveryConformanceFixture {
            work_root: root.join("Work"),
            platform: "fixture-os".to_owned(),
            expected_names: Some(vec!["project-alpha".to_owned()]),
        },
    )
    .expect("corrected Connector must pass public conformance");
    assert_eq!(report.port_id, "WorkDiscovery");
    assert!(report.checks.iter().any(|check| check == "typed-operation"));
    assert!(report.checks.iter().any(|check| check == "repeat-stability"));

    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();
    let connector_context = ConnectorContext {
        platform: "fixture-os".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let context = action_context(&root_options, &connectors, &connector_context);
    let result = create_core_action_registry().execute("work.list", &json!({}), &context);

    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["items"].as_array().unwrap().len(), 1);
    assert_eq!(data["items"][0]["name"], "project-alpha");
    assert_eq!(
        data["diagnostics"]["selected_connector"]["id"],
        "fixture.work-broken"
    );

    fs::remove_dir_all(fixture).unwrap();
}
